use skylock_core::backup::{
    BackupConfig, BackupManager, CompressionConfig, CompressionAlgorithm
};
use skylock_core::security::key_manager::{KeyManager, KeyRotationPolicy};
use skylock_core::monitoring::PerformanceMonitor;
use tempfile::tempdir;
use tokio::sync::mpsc;
use std::path::PathBuf;
use std::fs::File;
use std::io::Write;

#[tokio::test]
async fn test_full_backup_system() -> Result<(), Box<dyn std::error::Error>> {
    // Set up test directories
    let backup_dir = tempdir()?;
    let source_dir = tempdir()?;
    let keys_dir = tempdir()?;

    // Create some test files with varying content
    create_test_files(&source_dir.path())?;

    // Initialize key manager
    let key_manager = KeyManager::new(
        keys_dir.path().to_path_buf(),
        KeyRotationPolicy::default()
    )?;
    key_manager.init().await?;

    // Create error channel
    let (error_tx, mut error_rx) = mpsc::channel(100);

    // Initialize backup configuration
    let config = BackupConfig {
        backup_root: backup_dir.path().to_path_buf(),
        retention_policy: Default::default(),
        dedup_block_size: 4096,
        compression: CompressionConfig {
            algorithm: CompressionAlgorithm::Zstd,
            level: 3,
            min_size: 4096,
            skip_extensions: vec!["jpg".into(), "zip".into()],
        },
        encryption_enabled: true,
        verify_after_backup: true,
        max_concurrent_operations: 4,
    };

    // Initialize backup manager
    let backup_manager = BackupManager::new(config, error_tx.clone())?;

    // Initialize performance monitor
    let perf_monitor = PerformanceMonitor::new()?;
    perf_monitor.start_operation("backup").await;

    // Create backup
    let backup_id = backup_manager.create_backup(
        source_dir.path().to_path_buf(),
        Default::default()
    ).await?;

    // Monitor backup progress
    loop {
        match backup_manager.get_backup_status(&backup_id).await? {
            Some(status) => {
                if status.is_completed() {
                    break;
                }
                if status.is_failed() {
                    return Err("Backup failed".into());
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            _ => return Err("Backup not found".into()),
        }
    }

    perf_monitor.end_operation("backup").await;

    // Verify backup
    backup_manager.verify_backup(&backup_id).await?;

    // Check performance metrics
    let analysis = perf_monitor.analyze_performance().await;
    println!("Performance Analysis:");
    println!("- Average compression ratio: {:.2}", analysis.avg_compression_ratio);
    println!("- Average dedup ratio: {:.2}", analysis.avg_dedup_ratio);
    println!("- Backup duration: {:?}", analysis.avg_backup_duration);
    if !analysis.bottlenecks.is_empty() {
        println!("Detected bottlenecks:");
        for bottleneck in analysis.bottlenecks {
            println!("  - {}", bottleneck);
        }
    }

    // Check for any errors
    match error_rx.try_recv() {
        Ok(error) => {
            return Err(format!("Received error during backup: {}", error).into());
        }
        Err(mpsc::error::TryRecvError::Empty) => (),
        Err(e) => return Err(Box::new(e)),
    }

    Ok(())
}

fn create_test_files(dir: &std::path::Path) -> std::io::Result<()> {
    // Create text file
    let mut text_file = File::create(dir.join("test.txt"))?;
    writeln!(text_file, "This is a test file with some content that should compress well.\n")?;
    writeln!(text_file, "This line is repeated to test compression.\n")?;
    writeln!(text_file, "This line is repeated to test compression.\n")?;

    // Create binary file
    let mut bin_file = File::create(dir.join("test.bin"))?;
    let mut data = Vec::with_capacity(8192);
    for i in 0..8192 {
        data.push((i % 256) as u8);
    }
    bin_file.write_all(&data)?;

    // Create incompressible file
    let mut random_file = File::create(dir.join("random.dat"))?;
    let mut rng = rand::thread_rng();
    let mut random_data = vec![0u8; 4096];
    rand::Rng::fill(&mut rng, random_data.as_mut_slice());
    random_file.write_all(&random_data)?;

    // Create small file
    let mut small_file = File::create(dir.join("small.txt"))?;
    writeln!(small_file, "Small file")?;

    // Create subdirectory with files
    std::fs::create_dir(dir.join("subdir"))?;
    let mut nested_file = File::create(dir.join("subdir/nested.txt"))?;
    writeln!(nested_file, "This is a nested file.\n")?;

    Ok(())
}

#[tokio::test]
async fn test_key_rotation() -> Result<(), Box<dyn std::error::Error>> {
    let keys_dir = tempdir()?;
    let manager = KeyManager::new(
        keys_dir.path().to_path_buf(),
        KeyRotationPolicy {
            rotation_interval_days: 1,
            retain_old_keys_days: 7,
            emergency_rotation_enabled: true,
        }
    )?;

    manager.init().await?;

    // Create a key
    let key_id = manager.create_key(skylock_core::encryption::KeyType::Master).await?;

    // Verify initial key state
    let initial_key = manager.get_key(&key_id).await?.expect("Key should exist");
    let initial_metadata = manager.list_keys().await?;
    assert_eq!(initial_metadata.len(), 1);
    assert_eq!(initial_metadata[0].version, 1);

    // Simulate key compromise
    manager.mark_key_compromised(&key_id).await?;

    // Verify key was rotated
    let rotated_key = manager.get_key(&key_id).await?.expect("Key should exist");
    assert_ne!(initial_key, rotated_key, "Key should have been rotated");

    let final_metadata = manager.list_keys().await?;
    assert_eq!(final_metadata[0].version, 2);

    Ok(())
}

#[tokio::test]
async fn test_compression_optimization() -> Result<(), Box<dyn std::error::Error>> {
    use skylock_core::backup::compression::CompressionEngine;

    let config = CompressionConfig {
        algorithm: CompressionAlgorithm::Zstd,
        level: 3,
        min_size: 1024,
        skip_extensions: vec!["jpg".into(), "zip".into()],
    };

    let engine = CompressionEngine::new(config);

    // Test compressible data
    let test_data = b"This is a test string that should compress well because it has lots of repetition. ".repeat(100);
    let compressed = engine.compress(&test_data)?;
    assert!(compressed.len() < test_data.len(), "Data should be compressed");
    let decompressed = engine.decompress(&compressed, CompressionAlgorithm::Zstd)?;
    assert_eq!(test_data.to_vec(), decompressed, "Decompression should match original");

    // Test small file handling
    let small_data = b"Small file";
    assert!(!engine.should_compress(&PathBuf::from("small.txt"), small_data.len() as u64));

    // Test extension skipping
    assert!(!engine.should_compress(&PathBuf::from("image.jpg"), 10000));
    assert!(engine.should_compress(&PathBuf::from("doc.txt"), 10000));

    Ok(())
}

#[tokio::test]
async fn test_performance_monitoring() -> Result<(), Box<dyn std::error::Error>> {
    let monitor = PerformanceMonitor::new()?;

    // Record start of test operation
    monitor.start_operation("test_operation").await;

    // Simulate some work
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Record some metrics
    monitor.record_metrics(skylock_core::monitoring::PerformanceMetrics {
        timestamp: chrono::Utc::now(),
        backup_duration: std::time::Duration::from_secs(1),
        bytes_processed: 1024 * 1024,
        compression_ratio: 0.5,
        dedup_ratio: 0.7,
        cpu_usage: 50.0,
        memory_usage: 1024 * 1024 * 100,
        io_read_bytes: 1024 * 1024,
        io_write_bytes: 512 * 1024,
        concurrent_operations: 4,
    }).await;

    // End operation
    monitor.end_operation("test_operation").await;

    // Get timing info
    let timing = monitor.get_operation_timing("test_operation").await
        .expect("Should have timing info");
    assert!(timing.duration.is_some());
    assert!(timing.duration.unwrap().as_secs_f64() >= 1.0);

    // Analyze performance
    let analysis = monitor.analyze_performance().await;
    assert_eq!(analysis.avg_compression_ratio, 0.5);
    assert_eq!(analysis.avg_dedup_ratio, 0.7);
    assert!(analysis.operation_timings.len() > 0);

    Ok(())
}
