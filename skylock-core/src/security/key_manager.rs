use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use crate::{
    Result, Error, ErrorCategory, ErrorSeverity,
    security::error::SecurityErrorType,
};
use super::types::{KeyType, SecureKey, EncryptionEngine};
use super::hsm::HsmProvider;

/// Represents the current status of key storage
#[derive(Debug)]
pub struct StorageStatus {
    pub available_space_bytes: u64,
    pub total_space_bytes: u64,
    pub usage_percentage: f64,
}

/// Overall health status of the key manager
#[derive(Debug)]
pub struct KeyManagerHealth {
    pub total_keys: usize,
    pub keys_needing_rotation: usize,
    pub storage_status: StorageStatus,
    pub last_successful_backup: Option<DateTime<Utc>>,
    pub consecutive_failures: u32,
}

/// Tracks operations and their outcomes
#[derive(Debug, Clone)]
struct MetricsCollector {
    operations: Arc<RwLock<Vec<(String, bool)>>>,
}

impl MetricsCollector {
    fn new() -> Self {
        Self {
            operations: Arc::new(RwLock::new(Vec::new())),
        }
    }

    async fn record_operation(&self, operation: String, success: bool) {
        self.operations.write().await.push((operation, success));
    }
}

/// Handles key backup and restore operations
#[derive(Debug)]
struct BackupManager {
    backup_path: PathBuf,
    metrics: MetricsCollector,
}

impl BackupManager {
    fn new(backup_path: PathBuf, metrics: MetricsCollector) -> Self {
        Self {
            backup_path,
            metrics,
        }
    }

    async fn create_backup(&self, _keys: Vec<(String, Vec<u8>, KeyMetadata)>) -> Result<PathBuf> {
        let backup_file = self.backup_path.join(format!("backup_{}.zip", Utc::now().timestamp()));
        // Implementation details omitted for brevity
        Ok(backup_file)
    }

    async fn restore_from_backup(&self, _backup_path: PathBuf) -> Result<Vec<(String, Vec<u8>, KeyMetadata)>> {
        // Implementation details omitted for brevity
        Ok(Vec::new())
    }

    async fn list_backups(&self) -> Result<Vec<(PathBuf, BackupManifest)>> {
        Ok(Vec::new())
    }
}

/// Represents a backup snapshot
#[derive(Debug)]
struct BackupManifest {
    created_at: DateTime<Utc>,
}

/// Generate a new encryption key with secure randomness
fn generate_encryption_key() -> Result<Vec<u8>> {
    let mut key = vec![0u8; 32];
    getrandom::getrandom(&mut key)
        .map_err(|e| Error::new(
            ErrorCategory::Security(crate::error_types::SecurityErrorType::KeyGenerationFailed),
            ErrorSeverity::High,
            format!("Random key generation failed: {}", e),
            "key_manager".to_string(),
        ))?;
    Ok(key)
}

/// Metadata associated with a key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMetadata {
    pub key_id: String,
    pub key_type: KeyType,
    pub created_at: DateTime<Utc>,
    pub rotated_at: Option<DateTime<Utc>>,
    pub status: KeyStatus,
    pub version: u32,
}

/// Possible states of a key
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyStatus {
    Active,
    Rotating,
    Archived,
    Compromised,
}

/// Configuration for key rotation behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationPolicy {
    pub rotation_interval_days: u32,
    pub retain_old_keys_days: u32,
    pub emergency_rotation_enabled: bool,
}

impl Default for KeyRotationPolicy {
    fn default() -> Self {
        Self {
            rotation_interval_days: 90,  // Rotate keys every 90 days
            retain_old_keys_days: 365,   // Keep old keys for a year
            emergency_rotation_enabled: true,
        }
    }
}

/// Main key management system
pub struct KeyManager {
    storage_path: PathBuf,
    active_keys: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    metadata: Arc<RwLock<HashMap<String, KeyMetadata>>>,
    policy: KeyRotationPolicy,
    engine: Box<dyn EncryptionEngine>,
    hsm: Box<dyn HsmProvider>,
    metrics: MetricsCollector,
    backup_manager: BackupManager,
    rate_limiter: Arc<RwLock<HashMap<String, (DateTime<Utc>, u32)>>>,
}

impl std::fmt::Debug for KeyManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyManager")
            .field("storage_path", &self.storage_path)
            .field("active_keys", &"[REDACTED_KEYS]")
            .field("metadata", &"[METADATA_STORE]")
            .field("policy", &self.policy)
            .field("engine", &"[ENCRYPTION_ENGINE]")
            .field("hsm", &"[HSM_PROVIDER]")
            .field("metrics", &"[METRICS_COLLECTOR]")
            .field("backup_manager", &"[BACKUP_MANAGER]")
            .field("rate_limiter", &"[RATE_LIMITER]")
            .finish()
    }
}

impl KeyManager {
    /// Create a new KeyManager instance
    pub async fn new(
        storage_path: PathBuf,
        policy: KeyRotationPolicy,
        engine: Box<dyn EncryptionEngine>,
        hsm: Box<dyn HsmProvider>
    ) -> Result<Self> {
        tokio::fs::create_dir_all(&storage_path).await?;
        let metrics = MetricsCollector::new();
        let backup_manager = BackupManager::new(storage_path.join("backups"), metrics.clone());
        
        Ok(Self {
            storage_path: storage_path.clone(),
            active_keys: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            policy,
            engine,
            hsm,
            metrics,
            backup_manager,
            rate_limiter: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    /// Initialize the key manager
    pub async fn init(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.storage_path).await?;
        self.load_keys().await?;
        self.spawn_rotation_task();
        Ok(())
    }

    /// Create a new encryption key
    pub async fn create_key(&self, key_type: KeyType) -> Result<String> {
        let key_id = Uuid::new_v4().to_string();
        let key = generate_encryption_key()?;
        
        let metadata = KeyMetadata {
            key_id: key_id.clone(),
            key_type,
            created_at: Utc::now(),
            rotated_at: None,
            status: KeyStatus::Active,
            version: 1,
        };

        self.store_key(&key_id, key, metadata).await?;
        Ok(key_id)
    }

    /// Store a key and its metadata
    async fn store_key(&self, key_id: &str, key_data: Vec<u8>, metadata: KeyMetadata) -> Result<()> {
        // Encrypt the key before storing
        let encrypted_key = self.engine.encrypt(&key_data)?;
        
        // Store in HSM if available
        self.hsm.store_key(key_id, &encrypted_key).await?;

        // Store to disk
        let key_path = self.storage_path.join(format!("{}.key", key_id));
        let meta_path = self.storage_path.join(format!("{}.meta", key_id));

        tokio::fs::write(&key_path, &encrypted_key).await?;
        tokio::fs::write(&meta_path, serde_json::to_string(&metadata)?).await?;

        // Update in-memory state
        {
            let mut active_keys = self.active_keys.write().await;
            let mut meta = self.metadata.write().await;
            
            active_keys.insert(key_id.to_string(), encrypted_key);
            meta.insert(key_id.to_string(), metadata);
        }

        Ok(())
    }

    /// Retrieve a key by ID
    pub async fn get_key(&self, key_id: &str) -> Result<Vec<u8>> {
        // Try memory cache first
        if let Some(encrypted_key) = self.active_keys.read().await.get(key_id) {
            return self.engine.decrypt(encrypted_key);
        }

        // Try HSM
        if let Ok(key_data) = self.hsm.get_key(key_id).await {
            return Ok(key_data);
        }

        // Load from disk
        let key_path = self.storage_path.join(format!("{}.key", key_id));
        let encrypted_key = tokio::fs::read(&key_path).await?;
        let decrypted_key = self.engine.decrypt(&encrypted_key)?;

        // Cache it
        self.active_keys.write().await.insert(key_id.to_string(), encrypted_key);

        Ok(decrypted_key)
    }

    /// Mark a key as compromised and optionally trigger emergency rotation
    pub async fn mark_key_compromised(&self, key_id: &str) -> Result<()> {
        let mut meta_guard = self.metadata.write().await;
        if let Some(meta) = meta_guard.get_mut(key_id) {
            meta.status = KeyStatus::Compromised;

            // Save updated metadata
            let meta_path = self.storage_path.join(format!("{}.meta", key_id));
            tokio::fs::write(meta_path, serde_json::to_string_pretty(meta)?).await?;

            if self.policy.emergency_rotation_enabled {
                // Release lock before rotation
                drop(meta_guard);
                self.rotate_key(key_id).await?;
            }
        }
        Ok(())
    }

    /// List all keys and their metadata
    pub async fn list_keys(&self) -> Result<Vec<KeyMetadata>> {
        Ok(self.metadata.read().await.values().cloned().collect())
    }

    /// Load keys from disk storage
    async fn load_keys(&self) -> Result<()> {
        let mut keys = HashMap::new();
        let mut metadata = HashMap::new();

        let mut entries = tokio::fs::read_dir(&self.storage_path).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_file() {
                if let Some(file_name) = entry.file_name().to_str() {
                    if file_name.ends_with(".key") {
                        let key_id = file_name.trim_end_matches(".key").to_string();
                        let key_data = tokio::fs::read(entry.path()).await?;
                        let meta_path = entry.path().with_extension("meta");

                        if meta_path.exists() {
                            let meta_data = tokio::fs::read_to_string(meta_path).await?;
                            let meta: KeyMetadata = serde_json::from_str(&meta_data)?;

                            keys.insert(key_id.clone(), key_data);
                            metadata.insert(key_id, meta);
                        }
                    }
                }
            }
        }

        *self.active_keys.write().await = keys;
        *self.metadata.write().await = metadata;

        Ok(())
    }

    /// Start background key rotation task
    fn spawn_rotation_task(&self) {
        let storage_path = self.storage_path.clone();
        let active_keys = self.active_keys.clone();
        let metadata = self.metadata.clone();
        let policy = self.policy.clone();

        tokio::spawn(async move {
            loop {
                // Check for keys that need rotation
                let now = Utc::now();
                let mut keys_to_rotate = Vec::new();

                {
                    let metadata_guard = metadata.read().await;
                    for (key_id, meta) in metadata_guard.iter() {
                        if meta.status == KeyStatus::Active {
                            let last_rotation = meta.rotated_at.unwrap_or(meta.created_at);
                            let days_since_rotation = (now - last_rotation).num_days();

                            if days_since_rotation >= policy.rotation_interval_days as i64 {
                                keys_to_rotate.push(key_id.clone());
                            }
                        }
                    }
                }

                // Rotate keys that need it
                for key_id in keys_to_rotate {
                    if let Err(e) = Self::rotate_key_internal(
                        &storage_path,
                        &active_keys,
                        &metadata,
                        &key_id
                    ).await {
                        eprintln!("Error rotating key {}: {}", key_id, e);
                    }
                }

                // Sleep for a day before next check
                tokio::time::sleep(tokio::time::Duration::from_secs(86400)).await;
            }
        });
    }

    /// Internal key rotation implementation
    async fn rotate_key_internal(
        storage_path: &PathBuf,
        active_keys: &RwLock<HashMap<String, Vec<u8>>>,
        metadata: &RwLock<HashMap<String, KeyMetadata>>,
        key_id: &str,
    ) -> Result<()> {
        // Generate new key
        let new_key = generate_encryption_key()?;

        // Update metadata
        let mut meta_guard = metadata.write().await;
        if let Some(meta) = meta_guard.get_mut(key_id) {
            meta.rotated_at = Some(Utc::now());
            meta.version += 1;
            meta.status = KeyStatus::Active;

            // Save updated metadata
            let meta_path = storage_path.join(format!("{}.meta", key_id));
            tokio::fs::write(meta_path, serde_json::to_string_pretty(meta)?).await?;
        }

        // Update active key
        let mut keys_guard = active_keys.write().await;
        keys_guard.insert(key_id.to_string(), new_key.clone());

        // Save new key
        let key_path = storage_path.join(format!("{}.key", key_id));
        tokio::fs::write(key_path, new_key).await?;

        Ok(())
    }

    /// Public interface for key rotation
    pub async fn rotate_key(&self, key_id: &str) -> Result<()> {
        Self::rotate_key_internal(
            &self.storage_path,
            &self.active_keys,
            &self.metadata,
            key_id
        ).await
    }

    /// Check health status
    pub async fn check_health(&self) -> Result<KeyManagerHealth> {
        let total_keys = self.active_keys.read().await.len();
        let keys_needing_rotation = self.check_rotation_status().await?.len();
        
        // Check storage status
        let storage_status = StorageStatus {
            available_space_bytes: fs2::available_space(&self.storage_path)?,
            total_space_bytes: fs2::total_space(&self.storage_path)?,
            usage_percentage: (fs2::available_space(&self.storage_path)? as f64 
                / fs2::total_space(&self.storage_path)? as f64) * 100.0,
        };

        let backups = self.backup_manager.list_backups().await?;
        let last_successful_backup = backups.into_iter()
            .map(|(_, manifest)| manifest.created_at)
            .max();

        Ok(KeyManagerHealth {
            total_keys,
            keys_needing_rotation,
            storage_status,
            last_successful_backup,
            consecutive_failures: 0,
        })
    }

    /// Find keys that need rotation
    pub async fn check_rotation_status(&self) -> Result<Vec<String>> {
        let mut keys_to_rotate = Vec::new();
        let now = Utc::now();
        
        let metadata = self.metadata.read().await;
        for (key_id, meta) in metadata.iter() {
            if meta.status == KeyStatus::Active {
                let age = now - meta.created_at;
                if age.num_days() as u32 >= self.policy.rotation_interval_days {
                    keys_to_rotate.push(key_id.clone());
                }
            }
        }
        
        Ok(keys_to_rotate)
    }

    /// Rate limiting check for operations
    async fn check_rate_limit(&self, operation: &str) -> Result<()> {
        const MAX_OPERATIONS_PER_MINUTE: u32 = 60;
        const RATE_LIMIT_WINDOW_SECS: i64 = 60;

        let now = Utc::now();
        let mut limiter = self.rate_limiter.write().await;

        if let Some((last_time, count)) = limiter.get_mut(operation) {
            let age = now - *last_time;
            if age.num_seconds() < RATE_LIMIT_WINDOW_SECS {
                if *count >= MAX_OPERATIONS_PER_MINUTE {
                    return Err(Error::new(
                        ErrorCategory::Security(crate::error_types::SecurityErrorType::RateLimitExceeded),
                        ErrorSeverity::High,
                        format!("Rate limit exceeded for operation: {}", operation),
                        "key_manager".to_string(),
                    ).into());
                }
                *count += 1;
            } else {
                *last_time = now;
                *count = 1;
            }
        } else {
            limiter.insert(operation.to_string(), (now, 1));
        }

        Ok(())
    }

    /// Create a backup of all keys
    pub async fn create_backup(&self) -> Result<PathBuf> {
        self.check_rate_limit("create_backup").await?;
        
        let keys = {
            let active_keys = self.active_keys.read().await;
            let metadata = self.metadata.read().await;
            active_keys.iter()
                .filter_map(|(id, key)| {
                    metadata.get(id).map(|meta| 
                        (id.clone(), key.clone(), meta.clone())
                    )
                })
                .collect::<Vec<_>>()
        };

        self.backup_manager.create_backup(keys).await
    }

    /// Restore keys from a backup file
    pub async fn restore_from_backup(&self, backup_path: PathBuf) -> Result<()> {
        self.check_rate_limit("restore_backup").await?;
        
        let restored = self.backup_manager.restore_from_backup(backup_path).await?;
        
        // Clear existing keys
        {
            let mut active_keys = self.active_keys.write().await;
            let mut metadata = self.metadata.write().await;
            active_keys.clear();
            metadata.clear();
        }

        // Restore keys
        for (id, key, meta) in restored {
            self.store_key(&id, key, meta).await?;
        }

        Ok(())
    }

    /// Clean up old rotated keys
    pub async fn cleanup_old_keys(&self) -> Result<()> {
        let now = Utc::now();
        let metadata = self.metadata.read().await;
        
        for (key_id, meta) in metadata.iter() {
            if let Some(rotated_at) = meta.rotated_at {
                let age = now - rotated_at;
                if age.num_days() as u32 >= self.policy.retain_old_keys_days {
                    let key_path = self.storage_path.join(format!("{}.key", key_id));
                    let meta_path = self.storage_path.join(format!("{}.meta", key_id));
                    
                    tokio::fs::remove_file(key_path).await?;
                    tokio::fs::remove_file(meta_path).await?;

                    // Record the cleanup operation
                    self.metrics.record_operation(
                        format!("Cleaned up old key: {}", key_id),
                        true
                    ).await;
                }
            }
        }
        
        Ok(())
    }

    /// Securely clear sensitive data from memory
    fn secure_clear(data: &mut [u8]) {
        use zeroize::Zeroize;
        data.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Mock implementation of EncryptionEngine for testing
    #[derive(Debug)]
    struct MockEncryptionEngine;
    impl EncryptionEngine for MockEncryptionEngine {
        fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
            Ok(data.to_vec())
        }
        fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
            Ok(data.to_vec())
        }
        fn get_key_type(&self) -> KeyType {
            KeyType::Master
        }
        fn get_key_status(&self) -> SecureKey {
            SecureKey {
                key_type: KeyType::Master,
                key_data: vec![0u8; 32],
                created_at: chrono::Utc::now(),
                last_used: chrono::Utc::now(),
            }
        }
    }

    /// Mock implementation of HsmProvider for testing
    #[derive(Debug)]
    struct MockHsmProvider;
    #[async_trait::async_trait]
    impl HsmProvider for MockHsmProvider {
        async fn store_key(&self, _key_id: &str, _key_data: &[u8]) -> Result<()> {
            Ok(())
        }
        async fn retrieve_key(&self, _key_id: &str) -> Result<Vec<u8>> {
            Ok(vec![0u8; 32])
        }
        async fn delete_key(&self, _key_id: &str) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_key_lifecycle() -> Result<()> {
        let temp_dir = tempdir()?;
        let manager = KeyManager::new(
            temp_dir.path().to_path_buf(),
            KeyRotationPolicy::default(),
            Box::new(MockEncryptionEngine),
            Box::new(MockHsmProvider)
        ).await?;

        // Initialize the manager
        manager.init().await?;

        // Create a new key
        let key_id = manager.create_key(KeyType::Master).await?;

        // Verify key exists
        let key = manager.get_key(&key_id).await?;
        assert!(!key.is_empty());

        // List keys
        let keys = manager.list_keys().await?;
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key_id, key_id);

        // Mark key as compromised
        manager.mark_key_compromised(&key_id).await?;

        // Verify key status updated
        let keys = manager.list_keys().await?;
        assert_eq!(keys[0].status, KeyStatus::Active); // Should be Active after emergency rotation

        // Check rotation status
        let status = manager.check_rotation_status().await?;
        assert!(status.is_empty()); // No keys should need rotation yet

        // Check health
        let health = manager.check_health().await?;
        assert_eq!(health.total_keys, 1);
        assert_eq!(health.keys_needing_rotation, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_key_rotation() -> Result<()> {
        let temp_dir = tempdir()?;
        let manager = KeyManager::new(
            temp_dir.path().to_path_buf(),
            KeyRotationPolicy {
                rotation_interval_days: 0, // Force immediate rotation
                retain_old_keys_days: 1,
                emergency_rotation_enabled: true,
            },
            Box::new(MockEncryptionEngine),
            Box::new(MockHsmProvider)
        ).await?;

        manager.init().await?;

        // Create a key
        let key_id = manager.create_key(KeyType::Master).await?;

        // Get initial version
        let initial_version = manager.metadata.read().await
            .get(&key_id)
            .map(|meta| meta.version)
            .unwrap();

        // Force rotation
        manager.rotate_key(&key_id).await?;

        // Verify version increased
        let new_version = manager.metadata.read().await
            .get(&key_id)
            .map(|meta| meta.version)
            .unwrap();

        assert!(new_version > initial_version);

        Ok(())
    }
}