use std::sync::Arc;
use tokio::sync::Mutex;
use skylock_core::{Config, Result};

#[tokio::test]
async fn test_thread_safety() -> Result<()> {
    // Initialize test components
    let config = Config::load(None)?;
    let config = Arc::new(config);

    let notification_manager = Arc::new(NotificationManager::new()?);
    let credential_manager = Arc::new(CredentialManager::new(
        config.data_dir.join("secrets/test_key")
    )?);

    // Test concurrent access
    let nm_clone = notification_manager.clone();
    let cm_clone = credential_manager.clone();

    let handle1 = tokio::spawn(async move {
        for i in 0..100 {
            nm_clone.notify_backup_started()?;
            nm_clone.notify_backup_completed(&format!("test_{}", i))?;
        }
        Ok::<_, anyhow::Error>(())
    });

    let handle2 = tokio::spawn(async move {
        for i in 0..100 {
            let _ = cm_clone.store_credential(&format!("key_{}", i), "test_value").await?;
            let _ = cm_clone.get_credential(&format!("key_{}", i)).await?;
        }
        Ok::<_, anyhow::Error>(())
    });

    let (result1, result2) = tokio::join!(handle1, handle2);
    result1??;
    result2??;

    Ok(())
}

#[tokio::test]
async fn test_application_state() -> Result<()> {
    let state = setup_test_state().await?;

    // Test concurrent backup and file monitoring
    let state_clone = state.clone();
    let backup_handle = tokio::spawn(async move {
        let mut backup_manager = state_clone.backup_manager.lock().await;
        backup_manager.create_backup().await
    });

    let state_clone = state.clone();
    let monitor_handle = tokio::spawn(async move {
        let mut monitor = state_clone.file_monitor.lock().await;
        monitor.process_events().await
    });

    let (backup_result, monitor_result) = tokio::join!(backup_handle, monitor_handle);
    backup_result??;
    monitor_result??;

    Ok(())
}

async fn setup_test_state() -> Result<Arc<ApplicationState>> {
    // Initialize test configuration
    let config = Config::load(None)?;
    let config = Arc::new(config);

    // Initialize test components with proper thread safety
    let credential_manager = CredentialManager::new(
        config.data_dir.join("secrets/test_key")
    )?;

    let hetzner_key = credential_manager.get_credential("test_hetzner_key").await
        .unwrap_or_else(|_| "test_key".to_string());

    let hetzner = Arc::new(HetznerClient::new(
        config.hetzner.clone(),
        &hetzner_key
    )?);

    let syncthing = Arc::new(SyncthingClient::new(
        "http://localhost:8384",
        "test_key"
    )?);

    let notification_manager = Arc::new(NotificationManager::new()?);

    let backup_manager = Arc::new(Mutex::new(BackupManager::new(
        config.backup.clone(),
        hetzner.clone(),
        notification_manager.clone(),
    )));

    let (file_monitor, _) = FileMonitor::new(
        hetzner.clone(),
        syncthing.clone(),
        vec![],
        notification_manager.clone(),
    )?;
    let file_monitor = Arc::new(Mutex::new(file_monitor));

    let recovery_manager = Arc::new(RecoveryManager::new(
        config.data_dir.join("recovery/test_state.json")
    ));

    Ok(Arc::new(ApplicationState {
        config,
        backup_manager,
        file_monitor,
        notification_manager,
        hetzner_client: hetzner,
        syncthing_client: syncthing,
        recovery_manager,
    }))
}
