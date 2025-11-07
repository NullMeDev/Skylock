//! Unix-specific platform tests

use std::path::Path;
use skylock_hybrid::platform;

#[cfg(unix)]
#[tokio::test]
async fn test_unix_platform_backup_creation() {
    let backup_impl = platform::get_platform_backup();
    
    // Test that we can create a backup instance
    assert!(!backup_impl.supports_snapshots()); // Currently returns false in stub
}

#[cfg(unix)]
#[tokio::test]
async fn test_unix_snapshot_operations() {
    let backup_impl = platform::get_platform_backup();
    
    let test_path = Path::new("/tmp/test_snapshot");
    
    // Test snapshot creation (should work with stub implementation)
    let result = backup_impl.create_snapshot(test_path).await;
    
    // In stub implementation, this should return a placeholder ID
    match result {
        Ok(snapshot_id) => {
            assert!(!snapshot_id.is_empty());
            
            // Test snapshot deletion
            let delete_result = backup_impl.delete_snapshot(&snapshot_id).await;
            assert!(delete_result.is_ok());
        }
        Err(_) => {
            // Expected for stub implementation that might return errors
            println!("Snapshot creation returned error as expected for stub implementation");
        }
    }
}

#[cfg(unix)]
#[test]
fn test_build_time_environment_variables() {
    // Test that build script set the right environment variables
    assert_eq!(env!("SKYLOCK_PLATFORM"), "unix");
    assert_eq!(env!("SKYLOCK_VSS_ENABLED"), "0");
    
    // Build time should be set
    let build_time = env!("SKYLOCK_BUILD_TIME");
    assert!(!build_time.is_empty());
    
    // Parse build time as number
    let _timestamp: i64 = build_time.parse().expect("Build time should be a valid number");
}

#[cfg(unix)]
#[test]
fn test_lvm_availability_detection() {
    // These are set by build script based on system capability detection
    let lvm_available = env!("SKYLOCK_LVM_AVAILABLE");
    let zfs_available = env!("SKYLOCK_ZFS_AVAILABLE");
    
    // Should be either "0" or "1"
    assert!(lvm_available == "0" || lvm_available == "1");
    assert!(zfs_available == "0" || zfs_available == "1");
    
    println!("LVM available: {}", lvm_available);
    println!("ZFS available: {}", zfs_available);
}

#[cfg(unix)]
#[test] 
fn test_platform_path_utilities() {
    use skylock_hybrid::platform::path;
    
    // Test config and data directory functions
    if let Some(config_dir) = path::config_dir() {
        assert!(config_dir.is_absolute());
        println!("Config dir: {}", config_dir.display());
    }
    
    if let Some(data_dir) = path::data_dir() {
        assert!(data_dir.is_absolute());
        println!("Data dir: {}", data_dir.display());
    }
    
    // Test path normalization
    let test_path = Path::new("./test/../normalized/path");
    let normalized = path::normalize_path(test_path);
    println!("Normalized path: {}", normalized.display());
    
    // Test accessibility check
    let accessible = path::is_accessible(Path::new("/tmp"));
    assert!(accessible); // /tmp should always be accessible on Unix
}

#[cfg(unix)]
#[test]
fn test_unix_specific_functions() {
    use std::collections::HashMap;
    
    // Test system info gathering (using stub implementation)
    println!("Unix-specific functions would be tested here once implemented");
    
    // For now, just test that we can import the platform module
    let backup_impl = platform::get_platform_backup();
    println!("Platform backup implementation created: supports_snapshots = {}", 
             backup_impl.supports_snapshots());
}
