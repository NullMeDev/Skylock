use crate::{
    Result, 
    error_types::{Error, ErrorCategory, ErrorSeverity, SecurityErrorType},
};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use super::key_manager::{KeyMetadata, KeyStatus};
use super::monitoring::MetricsCollector;
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize, Deserialize)]
struct BackupManifest {
    created_at: DateTime<Utc>,
    key_count: usize,
    version: u32,
    checksum: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct KeyBackup {
    key_id: String,
    metadata: KeyMetadata,
    encrypted_key: Vec<u8>,
}

pub struct BackupManager {
    backup_path: PathBuf,
    metrics: MetricsCollector,
}

impl BackupManager {
    pub fn new(backup_path: PathBuf, metrics: MetricsCollector) -> Self {
        Self {
            backup_path,
            metrics,
        }
    }

    pub async fn create_backup(&self, keys: Vec<(String, Vec<u8>, KeyMetadata)>) -> Result<PathBuf> {
        // Create backup directory if it doesn't exist
        fs::create_dir_all(&self.backup_path).await?;

        // Create a unique backup file name
        let backup_file = self.backup_path.join(format!(
            "key_backup_{}.enc",
            Utc::now().format("%Y%m%d_%H%M%S")
        ));

        // Create backup entries
        let backups: Vec<KeyBackup> = keys.into_iter()
            .map(|(id, key, metadata)| KeyBackup {
                key_id: id,
                metadata,
                encrypted_key: key,
            })
            .collect();

        // Calculate checksum
        let backup_data = serde_json::to_vec(&backups)?;
        let checksum = {
            let mut hasher = sha2::Sha256::new();
            sha2::Digest::update(&mut hasher, &backup_data);
            format!("{:x}", hasher.finalize())
        };

        // Create manifest
        let manifest = BackupManifest {
            created_at: Utc::now(),
            key_count: backups.len(),
            version: 1,
            checksum,
        };

        // Write backup file
        let mut file = File::create(&backup_file).await?;
        
        // Write manifest
        let manifest_json = serde_json::to_vec(&manifest)?;
        file.write_u32_le(manifest_json.len() as u32).await?;
        file.write_all(&manifest_json).await?;
        
        // Write data
        file.write_u32_le(backup_data.len() as u32).await?;
        file.write_all(&backup_data).await?;

        // Record the operation
        self.metrics.record_operation(
            format!("Created backup with {} keys", backups.len()),
            true
        ).await;

        Ok(backup_file)
    }

    pub async fn restore_from_backup(&self, backup_file: PathBuf) -> Result<Vec<(String, Vec<u8>, KeyMetadata)>> {
        let mut file = File::open(&backup_file).await?;

        // Read manifest
        let manifest_len = file.read_u32_le().await? as usize;
        let mut manifest_data = vec![0u8; manifest_len];
        file.read_exact(&mut manifest_data).await?;
        let manifest: BackupManifest = serde_json::from_slice(&manifest_data)?;

        // Read backup data
        let data_len = file.read_u32_le().await? as usize;
        let mut backup_data = vec![0u8; data_len];
        file.read_exact(&mut backup_data).await?;

        // Verify checksum
        let computed_checksum = {
            let mut hasher = sha2::Sha256::new();
            sha2::Digest::update(&mut hasher, &backup_data);
            format!("{:x}", hasher.finalize())
        };

        if computed_checksum != manifest.checksum {
            return Err(Error::new(
                ErrorCategory::Security(SecurityErrorType::IntegrityCheckFailed),
                ErrorSeverity::High,
                "Backup checksum verification failed".to_string(),
                "backup_manager".to_string()
            ).into());
        }

        // Parse backup data
        let backups: Vec<KeyBackup> = serde_json::from_slice(&backup_data)?;
        let restored: Vec<(String, Vec<u8>, KeyMetadata)> = backups.into_iter()
            .map(|backup| (
                backup.key_id,
                backup.encrypted_key,
                backup.metadata
            ))
            .collect();

        // Record the operation
        self.metrics.record_operation(
            format!("Restored {} keys from backup", restored.len()),
            true
        ).await;

        Ok(restored)
    }

    pub async fn list_backups(&self) -> Result<Vec<(PathBuf, BackupManifest)>> {
        let mut backups = Vec::new();
        let mut entries = fs::read_dir(&self.backup_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("enc") {
                if let Ok(manifest) = self.read_backup_manifest(&path).await {
                    backups.push((path, manifest));
                }
            }
        }

        Ok(backups)
    }

    async fn read_backup_manifest(&self, path: &PathBuf) -> Result<BackupManifest> {
        let mut file = File::open(path).await?;
        let manifest_len = file.read_u32_le().await? as usize;
        let mut manifest_data = vec![0u8; manifest_len];
        file.read_exact(&mut manifest_data).await?;
        Ok(serde_json::from_slice(&manifest_data)?)
    }
}