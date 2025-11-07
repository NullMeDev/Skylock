use skylock_core::security::key_manager::*;
use skylock_core::encryption::KeyType;
use tempfile::tempdir;
use tokio::fs;
use std::time::Duration;
use chrono::{Utc, TimeZone};

#[tokio::test]
async fn test_key_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let manager = KeyManager::new(
        temp_dir.path().to_path_buf(),
        KeyRotationPolicy::default()
    )?;

    // Test initialization
    manager.init().await?;

    // Test key creation for different types
    let master_key_id = manager.create_key(KeyType::Master).await?;
    let block_key_id = manager.create_key(KeyType::Block).await?;

    // Verify keys exist and are different
    let master_key = manager.get_key(&master_key_id).await?
        .ok_or("Master key not found")?;
    let block_key = manager.get_key(&block_key_id).await?
        .ok_or("Block key not found")?;
    assert_ne!(master_key, block_key);

    // Test key metadata
    let keys = manager.list_keys().await?;
    assert_eq!(keys.len(), 2);

    let master_meta = keys.iter().find(|k| k.key_id == master_key_id)
        .ok_or("Master key metadata not found")?;
    assert_eq!(master_meta.key_type, KeyType::Master);
    assert_eq!(master_meta.status, KeyStatus::Active);
    assert_eq!(master_meta.version, 1);

    // Test key compromise handling
    manager.mark_key_compromised(&master_key_id).await?;
    let keys = manager.list_keys().await?;
    let compromised_meta = keys.iter().find(|k| k.key_id == master_key_id)
        .ok_or("Compromised key metadata not found")?;
    assert_eq!(compromised_meta.status, KeyStatus::Compromised);

    // Test automatic rotation
    let old_key = manager.get_key(&master_key_id).await?
        .ok_or("Old key not found before rotation")?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    manager.mark_key_compromised(&master_key_id).await?;
    let new_key = manager.get_key(&master_key_id).await?
        .ok_or("New key not found after rotation")?;
    assert_ne!(old_key, new_key);

    // Test persistence
    drop(manager);

    // Create new manager instance
    let manager2 = KeyManager::new(
        temp_dir.path().to_path_buf(),
        KeyRotationPolicy::default()
    )?;
    manager2.init().await?;

    // Verify keys are still available
    let loaded_key = manager2.get_key(&master_key_id).await?.unwrap();
    assert_eq!(loaded_key, new_key);

    Ok(())
}

#[tokio::test]
async fn test_key_rotation_policy() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;

    // Create policy with short rotation interval
    let policy = KeyRotationPolicy {
        rotation_interval_days: 1,
        retain_old_keys_days: 7,
        emergency_rotation_enabled: true,
    };

    let manager = KeyManager::new(temp_dir.path().to_path_buf(), policy)?;
    manager.init().await?;

    // Create a key with old timestamp
    let key_id = manager.create_key(KeyType::Master).await?;

    // Manually modify the metadata file to simulate an old key
    let meta_path = temp_dir.path().join(format!("{}.meta", key_id));
    let mut meta: KeyMetadata = serde_json::from_str(&fs::read_to_string(&meta_path).await?)?;
    meta.created_at = Utc.timestamp_opt(Utc::now().timestamp() - 86400 * 2, 0).unwrap();
    fs::write(&meta_path, serde_json::to_string_pretty(&meta)?).await?;

    // Force rotation check
    manager.check_rotation_schedule().await?;

    // Verify key was rotated
    let keys = manager.list_keys().await?;
    let rotated_meta = keys.iter().find(|k| k.key_id == key_id).unwrap();
    assert!(rotated_meta.rotated_at.is_some());
    assert!(rotated_meta.version > 1);

    Ok(())
}

#[tokio::test]
async fn test_concurrent_key_operations() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let manager = KeyManager::new(
        temp_dir.path().to_path_buf(),
        KeyRotationPolicy::default()
    )?;
    manager.init().await?;

    // Spawn multiple tasks that create and access keys
    let mut handles = vec![];
    for _ in 0..10 {
        let manager = manager.clone();
        handles.push(tokio::spawn(async move {
            let key_id = manager.create_key(KeyType::Block).await?;
            let _key = manager.get_key(&key_id).await?;
            Result::<_, Box<dyn std::error::Error + Send + Sync>>::Ok(())
        }));
    }

    // Wait for all operations to complete
    for handle in handles {
        handle.await??;
    }

    // Verify all keys were created successfully
    let keys = manager.list_keys().await?;
    assert_eq!(keys.len(), 10);

    Ok(())
}
