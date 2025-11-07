use std::path::PathBuf;
use chrono::{Utc, Duration};
use crate::security::{
    key_manager::{KeyManager, KeyRotationPolicy, KeyStatus},
    types::{KeyType, EncryptionEngine, SecureKey},
};

struct MockEncryptionEngine;

impl EncryptionEngine for MockEncryptionEngine {
    fn encrypt(&self, data: &[u8]) -> crate::Result<Vec<u8>> {
        // Simple XOR encryption for testing
        Ok(data.iter().map(|b| b ^ 0xFF).collect())
    }

    fn decrypt(&self, data: &[u8]) -> crate::Result<Vec<u8>> {
        // XOR is its own inverse
        Ok(data.iter().map(|b| b ^ 0xFF).collect())
    }

    fn get_key_type(&self) -> KeyType {
        KeyType::Master
    }

    fn get_key_status(&self) -> SecureKey {
        SecureKey {
            key_type: KeyType::Master,
            key_data: vec![],
            created_at: Utc::now(),
            last_used: Utc::now(),
        }
    }
}

#[tokio::test]
async fn test_key_manager_basic_operations() {
    let temp_dir = tempfile::tempdir().unwrap();
    let policy = KeyRotationPolicy::default();
    let engine = Box::new(MockEncryptionEngine);

    let manager = KeyManager::new(
        temp_dir.path().to_path_buf(),
        policy,
        engine,
    ).await.unwrap();

    // Test key storage and retrieval
    let key_data = b"test key data".to_vec();
    manager.store_key("test-key-1", key_data.clone()).await.unwrap();
    
    let retrieved = manager.get_key("test-key-1").await.unwrap();
    assert_eq!(retrieved, key_data);
}

#[tokio::test]
async fn test_key_rotation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut policy = KeyRotationPolicy::default();
    policy.rotation_interval_days = 0; // Force rotation check to trigger
    let engine = Box::new(MockEncryptionEngine);

    let manager = KeyManager::new(
        temp_dir.path().to_path_buf(),
        policy,
        engine,
    ).await.unwrap();

    // Store a key
    let key_data = b"test key data".to_vec();
    manager.store_key("test-key-1", key_data).await.unwrap();

    // Check rotation status
    let keys_to_rotate = manager.check_rotation_status().await.unwrap();
    assert_eq!(keys_to_rotate.len(), 1);
    assert_eq!(keys_to_rotate[0], "test-key-1");

    // Rotate the key
    manager.rotate_key("test-key-1").await.unwrap();

    // Verify the key was rotated
    let metadata = manager.metadata.read().await;
    let meta = metadata.get("test-key-1").unwrap();
    assert!(meta.rotated_at.is_some());
    assert_eq!(meta.version, 2);
}

#[tokio::test]
async fn test_comprehensive_key_management() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut policy = KeyRotationPolicy::default();
    policy.retain_old_keys_days = 0; // Force cleanup to trigger
    let engine = Box::new(MockEncryptionEngine);
    let hsm = Box::new(SoftwareHsm::new());

    let manager = KeyManager::new(
        temp_dir.path().to_path_buf(),
        policy,
        engine,
        hsm,
    ).await.unwrap();

    // Test key operations
    let key_data = b"test key data".to_vec();
    manager.store_key("test-key-1", key_data.clone()).await.unwrap();
    
    // Test backup
    let backup_path = manager.create_backup().await.unwrap();
    assert!(backup_path.exists());

    // Test key rotation
    manager.rotate_key("test-key-1").await.unwrap();
    
    // Check health
    let health = manager.check_health().await.unwrap();
    assert_eq!(health.total_keys, 1);
    assert!(health.last_successful_backup.is_some());

    // Test cleanup
    manager.cleanup_old_keys().await.unwrap();

    // Test metrics
    let metrics = manager.metrics.get_metrics().await;
    assert_eq!(metrics.total_keys, 1);
    assert_eq!(metrics.failed_operations, 0);

    // Test rate limiting
    let mut operations = Vec::new();
    for i in 0..61 {
        operations.push(manager.store_key(&format!("test-key-{}", i), key_data.clone()));
    }
    let results = futures::future::join_all(operations).await;
    assert!(results.last().unwrap().is_err()); // Last operation should fail due to rate limit

    // Test backup restore
    manager.restore_from_backup(backup_path).await.unwrap();

    // Verify key data
    let restored_key = manager.get_key("test-key-1").await.unwrap();
    assert_eq!(restored_key, key_data);
}