use skylock_core::{
    Result, SkylockError,
    error::{StorageErrorType, SystemErrorType, NetworkErrorType},
};
use skylock_hetzner::HetznerClient;
use skylock_sync::SyncthingClient;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::fs;

// Helper function to create a temporary test environment
async fn setup_test_env() -> Result<TempDir> {
    let temp = tempfile::tempdir()?;
    fs::create_dir_all(&temp.path().join("test_data")).await?;
    Ok(temp)
}

#[tokio::test]
async fn test_file_operation_errors() -> Result<()> {
    let temp_dir = setup_test_env().await?;
    let nonexistent_path = temp_dir.path().join("nonexistent");
    let readonly_path = temp_dir.path().join("readonly");

    // Test missing file error
    let result = fs::read(&nonexistent_path).await;
    assert!(matches!(
        SkylockError::from(result.unwrap_err()),
        SkylockError::Io(_)
    ));

    // Test permissions error
    fs::write(&readonly_path, "test").await?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&readonly_path, fs::Permissions::from_mode(0o444)).await?;
    }
    let result = fs::write(&readonly_path, "test").await;
    assert!(matches!(
        SkylockError::from(result.unwrap_err()),
        SkylockError::Io(_)
    ));

    Ok(())
}

#[tokio::test]
async fn test_network_operation_errors() -> Result<()> {
    // Test invalid Syncthing connection
    let client = SyncthingClient::new(
        "http://localhost:12345", // Invalid port
        "invalid_api_key",
    )?;

    let result = client.get_folders().await;
    assert!(matches!(
        result.unwrap_err(),
        SkylockError::Sync(SyncErrorType::NetworkError)
    ));

    // Test invalid storage connection
    let client = SFTPClient::new(
        "invalid_host",
        "invalid_user",
        "invalid_pass",
        22,
    )?;

    let result = client.connect().await;
    assert!(matches!(
        result.unwrap_err(),
        SkylockError::Storage(StorageErrorType::StorageBoxUnavailable)
    ));

    Ok(())
}

#[tokio::test]
async fn test_storage_operation_errors() -> Result<()> {
    let temp_dir = setup_test_env().await?;
    let test_file = temp_dir.path().join("test_file");

    // Create a test file that's too large for available space
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .mode(0o666)
            .open(&test_file)?;

        // Try to allocate more space than available
        let result = file.set_len(u64::MAX);
        assert!(matches!(
            result.unwrap_err().into(),
            SkylockError::Storage(StorageErrorType::InsufficientSpace)
        ));
    }

    Ok(())
}

#[tokio::test]
async fn test_error_recovery_scenarios() -> Result<()> {
    let temp_dir = setup_test_env().await?;
    let config_path = temp_dir.path().join("config.toml");

    // Test config error recovery
    fs::write(&config_path, "invalid_toml").await?;
    let config_result = Config::from_file(&config_path);
    assert!(matches!(
        config_result.unwrap_err(),
        SkylockError::Config(_)
    ));

    // Write valid config and verify recovery
    fs::write(&config_path, r#"
        [storage]
        type = "local"
        path = "/tmp/backup"

        [sync]
        enabled = true
        interval = 3600
    "#).await?;

    let config = Config::from_file(&config_path)?;
    assert!(config.sync.enabled);

    // Test circuit breaker recovery
    let client = SyncthingClient::new(
        "http://localhost:12345",
        "test_key",
    )?;

    // Multiple failed attempts should trigger circuit breaker
    for _ in 0..5 {
        let _ = client.get_folders().await;
    }

    // Wait for circuit breaker timeout
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Should be able to try again
    let result = client.get_folders().await;
    assert!(matches!(
        result.unwrap_err(),
        SkylockError::Sync(SyncErrorType::NetworkError)
    ));

    Ok(())
}
