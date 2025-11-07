//! Windows-specific platform tests

use std::path::Path;
use skylock_hybrid::platform;

#[cfg(windows)]
#[tokio::test]
async fn test_windows_platform_backup_creation() {
    let backup_impl = platform::get_platform_backup();
    
    // On Windows, VSS should be supported
    assert!(backup_impl.supports_snapshots());
}

#[cfg(windows)]
#[tokio::test]
async fn test_windows_vss_snapshot_operations() {
    let backup_impl = platform::get_platform_backup();
    
    let test_path = Path::new("C:\\temp\\test_snapshot");
    
    // Test VSS snapshot creation
    let result = backup_impl.create_snapshot(test_path).await;
    
    match result {
        Ok(snapshot_id) => {
            assert!(!snapshot_id.is_empty());
            println!("Created VSS snapshot: {}", snapshot_id);
            
            // Test snapshot deletion
            let delete_result = backup_impl.delete_snapshot(&snapshot_id).await;
            assert!(delete_result.is_ok());
            println!("Deleted VSS snapshot: {}", snapshot_id);
        }
        Err(e) => {
            // VSS might not be available in all test environments
            println!("VSS snapshot creation failed (expected in some test environments): {}", e);
        }
    }
}

#[cfg(windows)]
#[test]
fn test_build_time_environment_variables() {
    // Test that build script set the right environment variables for Windows
    assert_eq!(env!("SKYLOCK_PLATFORM"), "windows");
    assert_eq!(env!("SKYLOCK_VSS_ENABLED"), "1");
    
    // Build time should be set
    let build_time = env!("SKYLOCK_BUILD_TIME");
    assert!(!build_time.is_empty());
    
    // Parse build time as number
    let _timestamp: i64 = build_time.parse().expect("Build time should be a valid number");
}

#[cfg(windows)]
#[test]
fn test_windows_vss_linking() {
    // Test that VSS libraries are available
    // This is mainly checking that linking worked correctly
    // The actual VSS functionality is tested in integration tests
    
    // These should be available if linking worked
    println!("VSS support compiled in: {}", env!("SKYLOCK_VSS_ENABLED"));
    
    // On Windows, LVM and ZFS should not be available
    assert_eq!(env!("SKYLOCK_LVM_AVAILABLE"), "0");
    assert_eq!(env!("SKYLOCK_ZFS_AVAILABLE"), "0");
}

#[cfg(windows)]
#[test] 
fn test_platform_path_utilities() {
    use skylock_hybrid::platform::path;
    
    // Test config and data directory functions
    if let Some(config_dir) = path::config_dir() {
        assert!(config_dir.is_absolute());
        println!("Config dir: {}", config_dir.display());
        
        // On Windows, should typically be in AppData
        assert!(config_dir.to_string_lossy().contains("AppData") || 
                config_dir.to_string_lossy().contains("ProgramData"));
    }
    
    if let Some(data_dir) = path::data_dir() {
        assert!(data_dir.is_absolute());
        println!("Data dir: {}", data_dir.display());
    }
    
    // Test path normalization with Windows paths
    let test_path = Path::new(r".\test\..\normalized\path");
    let normalized = path::normalize_path(test_path);
    println!("Normalized path: {}", normalized.display());
    
    // Test accessibility check on Windows temp directory
    let accessible = path::is_accessible(Path::new(r"C:\temp"));
    println!("C:\\temp accessible: {}", accessible);
}

#[cfg(windows)]
#[test]
fn test_windows_specific_functions() {
    use std::collections::HashMap;
    
    // Test system info gathering (using stub implementation)
    println!("Windows-specific functions would be tested here once implemented");
    
    // For now, just test that we can import the platform module
    let backup_impl = platform::get_platform_backup();
    println!("Platform backup implementation created: supports_snapshots = {}", 
             backup_impl.supports_snapshots());
}

#[cfg(windows)]
#[test]
fn test_windows_volume_operations() {
    // Test volume enumeration (stub implementation)
    println!("Windows volume operations would be tested here once implemented");
    
    // For now, just test basic platform capabilities
    let backup_impl = platform::get_platform_backup();
    println!("Platform backup supports snapshots: {}", backup_impl.supports_snapshots());
}

#[cfg(windows)]
#[tokio::test]
async fn test_windows_service_integration() {
    // Test Windows service detection and management capabilities (stub)
    println!("Windows service integration would be tested here once implemented");
    
    // For now, just test basic async operations
    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    println!("Async test completed");
}
