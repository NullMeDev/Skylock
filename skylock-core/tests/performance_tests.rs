use skylock_core::monitoring::*;
use std::time::Duration;
use tokio::time::sleep;
use chrono::Utc;

#[tokio::test]
async fn test_performance_monitoring_detailed() -> Result<(), Box<dyn std::error::Error>> {
    let monitor = PerformanceMonitor::new()?;

    // Test multiple concurrent operations
    monitor.start_operation("op1").await;
    monitor.start_operation("op2").await;

    sleep(Duration::from_millis(100)).await;

    // Record metrics periodically
    for i in 0..5 {
        monitor.record_metrics(PerformanceMetrics {
            timestamp: Utc::now(),
            backup_duration: Duration::from_secs(i),
            bytes_processed: 1024 * 1024 * i as u64,
            compression_ratio: 0.5 + (i as f64 * 0.1),
            dedup_ratio: 0.7 - (i as f64 * 0.1),
            cpu_usage: 50.0 + (i as f64 * 10.0),
            memory_usage: 1024 * 1024 * 100 * i as u64,
            io_read_bytes: 1024 * 1024 * i as u64,
            io_write_bytes: 512 * 1024 * i as u64,
            concurrent_operations: 2,
        }).await;

        sleep(Duration::from_millis(50)).await;
    }

    // End operations in reverse order
    monitor.end_operation("op2").await;
    monitor.end_operation("op1").await;

    // Get operation timings
    let op1_timing = monitor.get_operation_timing("op1").await
        .ok_or("Failed to get op1 timing")?;
    let op2_timing = monitor.get_operation_timing("op2").await
        .ok_or("Failed to get op2 timing")?;

    let op1_duration = op1_timing.duration.ok_or("Missing op1 duration")?;
    let op2_duration = op2_timing.duration.ok_or("Missing op2 duration")?;
    assert!(op1_duration > op2_duration);

    // Test performance analysis
    let analysis = monitor.analyze_performance().await;

    // Check timing analysis
    assert_eq!(analysis.operation_timings.len(), 2);
    assert!(analysis.avg_backup_duration.as_millis() > 0);

    // Check metric averages
    assert!(analysis.avg_compression_ratio > 0.0);
    assert!(analysis.avg_dedup_ratio > 0.0);

    // Check bottleneck detection
    assert!(!analysis.bottlenecks.is_empty(), "Should detect some bottlenecks");

    // Test metrics history
    let history = monitor.get_metrics_history().await;
    assert_eq!(history.len(), 5);

    // Verify metrics are sorted by timestamp
    for i in 1..history.len() {
        assert!(history[i].timestamp > history[i-1].timestamp);
    }

    Ok(())
}

#[tokio::test]
async fn test_performance_bottleneck_detection() -> Result<(), Box<dyn std::error::Error>> {
    let monitor = PerformanceMonitor::new()?;

    // Simulate high CPU usage
    monitor.record_metrics(PerformanceMetrics {
        timestamp: Utc::now(),
        backup_duration: Duration::from_secs(1),
        bytes_processed: 1024 * 1024,
        compression_ratio: 0.5,
        dedup_ratio: 0.7,
        cpu_usage: 95.0, // High CPU usage
        memory_usage: 1024 * 1024 * 100,
        io_read_bytes: 1024 * 1024,
        io_write_bytes: 512 * 1024,
        concurrent_operations: 4,
    }).await;

    // Simulate high memory usage
    monitor.record_metrics(PerformanceMetrics {
        timestamp: Utc::now(),
        backup_duration: Duration::from_secs(1),
        bytes_processed: 1024 * 1024,
        compression_ratio: 0.5,
        dedup_ratio: 0.7,
        cpu_usage: 50.0,
        memory_usage: 1024 * 1024 * 1024 * 15, // High memory usage (15GB)
        io_read_bytes: 1024 * 1024,
        io_write_bytes: 512 * 1024,
        concurrent_operations: 4,
    }).await;

    // Simulate high I/O
    monitor.record_metrics(PerformanceMetrics {
        timestamp: Utc::now(),
        backup_duration: Duration::from_secs(1),
        bytes_processed: 1024 * 1024,
        compression_ratio: 0.5,
        dedup_ratio: 0.7,
        cpu_usage: 50.0,
        memory_usage: 1024 * 1024 * 100,
        io_read_bytes: 1024 * 1024 * 1024, // 1GB read
        io_write_bytes: 1024 * 1024 * 1024, // 1GB write
        concurrent_operations: 4,
    }).await;

    let analysis = monitor.analyze_performance().await;

    // Check detected bottlenecks
    let bottlenecks: Vec<_> = analysis.bottlenecks.iter().collect();
    assert!(bottlenecks.iter().any(|&b| b.contains("CPU")));
    assert!(bottlenecks.iter().any(|&b| b.contains("memory")));
    assert!(bottlenecks.iter().any(|&b| b.contains("disk")));

    Ok(())
}

#[tokio::test]
async fn test_metrics_persistence() -> Result<(), Box<dyn std::error::Error>> {
    use tempfile::tempdir;
    use std::path::PathBuf;

    let temp_dir = tempdir()?;
    let metrics_file = temp_dir.path().join("metrics.json");

    // Create monitor and record some metrics
    let monitor = PerformanceMonitor::new()?;

    for i in 0..3 {
        monitor.record_metrics(PerformanceMetrics {
            timestamp: Utc::now(),
            backup_duration: Duration::from_secs(i),
            bytes_processed: 1024 * i as u64,
            compression_ratio: 0.5,
            dedup_ratio: 0.7,
            cpu_usage: 50.0,
            memory_usage: 1024 * 1024 * 100,
            io_read_bytes: 1024 * i as u64,
            io_write_bytes: 512 * i as u64,
            concurrent_operations: 1,
        }).await;
    }

    // Save metrics
    monitor.save_metrics(&metrics_file).await?;

    // Create new monitor and load metrics
    let monitor2 = PerformanceMonitor::new()?;
    monitor2.load_metrics(&metrics_file).await?;

    // Verify loaded metrics match
    let original_metrics = monitor.get_metrics_history().await;
    let loaded_metrics = monitor2.get_metrics_history().await;

    assert_eq!(original_metrics.len(), loaded_metrics.len());
    for (orig, loaded) in original_metrics.iter().zip(loaded_metrics.iter()) {
        assert_eq!(orig.bytes_processed, loaded.bytes_processed);
        assert_eq!(orig.compression_ratio, loaded.compression_ratio);
        assert_eq!(orig.dedup_ratio, loaded.dedup_ratio);
    }

    Ok(())
}
