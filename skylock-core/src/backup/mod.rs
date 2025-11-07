mod storage;
mod dedup;
mod retention;
mod restore;
mod verification;
mod processor;
mod compression;

pub use self::storage::BackupStorage;
pub use self::dedup::DedupEngine;
pub use self::retention::RetentionManager;
pub use self::restore::RestoreManager;
pub use self::verification::BackupVerifier;
pub use self::compression::{CompressionConfig, CompressionAlgorithm};
use self::processor::BlockProcessor;

use std::collections::HashMap;
use tokio::sync::mpsc;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use crate::{Result, error::SystemError};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub backup_root: PathBuf,
    pub retention_policy: RetentionPolicy,
    pub dedup_block_size: usize,
    pub compression: CompressionConfig,
    pub encryption_enabled: bool,
    pub verify_after_backup: bool,
    pub max_concurrent_operations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub keep_last_n: usize,
    pub keep_daily: usize,
    pub keep_weekly: usize,
    pub keep_monthly: usize,
    pub keep_yearly: usize,
    pub min_age_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSet {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub backup_type: BackupType,
    pub source_path: PathBuf,
    pub size: u64,
    pub files_count: usize,
    pub status: BackupStatus,
    pub verification_status: Option<VerificationStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupType {
    Full,
    Incremental { parent_id: String },
    Differential { base_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupStatus {
    Pending,
    InProgress(f32), // Progress percentage
    Completed,
    Failed(String),
    Verified,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationStatus {
    Pending,
    InProgress(f32),
    Success,
    Failed(String),
}

pub struct BackupManager {
    config: BackupConfig,
    storage: BackupStorage,
    dedup_engine: DedupEngine,
    retention_manager: RetentionManager,
    restore_manager: RestoreManager,
    verifier: BackupVerifier,
    active_backups: Arc<RwLock<HashMap<String, BackupSet>>>,
    error_tx: mpsc::Sender<SystemError>,
}

impl BackupManager {
    pub fn new(config: BackupConfig, error_tx: mpsc::Sender<SystemError>) -> Result<Self> {
        let storage = BackupStorage::new(&config.backup_root)?;
        let dedup_engine = DedupEngine::new(config.dedup_block_size)?;
        let retention_manager = RetentionManager::new(config.retention_policy.clone());
        let restore_manager = RestoreManager::new(storage.clone())?;
        let verifier = BackupVerifier::new(storage.clone());

        Ok(Self {
            config,
            storage,
            dedup_engine,
            retention_manager,
            restore_manager,
            verifier,
            active_backups: Arc::new(RwLock::new(HashMap::new())),
            error_tx,
        })
    }

    pub async fn create_backup(&self, source: PathBuf, backup_type: BackupType) -> Result<String> {
        let backup_id = uuid::Uuid::new_v4().to_string();
        let backup_set = BackupSet {
            id: backup_id.clone(),
            timestamp: Utc::now(),
            backup_type,
            source_path: source.clone(),
            size: 0,
            files_count: 0,
            status: BackupStatus::Pending,
            verification_status: None,
        };

        // Register backup
        self.active_backups.write().await.insert(backup_id.clone(), backup_set);

        // Start backup process
        self.start_backup_process(backup_id.clone(), source).await?;

        Ok(backup_id)
    }

    async fn start_backup_process(&self, backup_id: String, source: PathBuf) -> Result<()> {
        let storage = self.storage.clone();
        let dedup_engine = self.dedup_engine.clone();
        let error_tx = self.error_tx.clone();
        let active_backups = self.active_backups.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            let result = async {
                // Update status to in progress
                {
                    let mut backups = active_backups.write().await;
                    if let Some(backup) = backups.get_mut(&backup_id) {
                        backup.status = BackupStatus::InProgress(0.0);
                    }
                }

                // Process files using concurrent block processor
                let processor = BlockProcessor::new(
                    dedup_engine,
                    storage,
                    config.max_concurrent_operations
                );

                let stats = processor.process_directory(&source).await?;

                // Update final status
                let mut backups = active_backups.write().await;
                if let Some(backup) = backups.get_mut(&backup_id) {
                    backup.size = stats.bytes_processed;
                    backup.files_count = stats.files_processed;
                    backup.status = BackupStatus::Completed;
                }

                Ok::<(), crate::error::SkylockError>(())
            }.await;

            if let Err(e) = result {
                // Update status to failed
                let mut backups = active_backups.write().await;
                if let Some(backup) = backups.get_mut(&backup_id) {
                    backup.status = BackupStatus::Failed(e.to_string());
                }

                // Send error notification
                let _ = error_tx.send(SystemError {
                    id: uuid::Uuid::new_v4().to_string(),
                    category: crate::error::ErrorCategory::Backup,
                    severity: crate::error::ErrorSeverity::High,
                    message: format!("Backup failed: {}", e),
                    timestamp: Utc::now(),
                    context: Some(serde_json::json!({
                        "backup_id": backup_id,
                        "source": source,
                    })),
                    status: crate::error::ErrorStatus::New,
                }).await;
            }
        });

        Ok(())
    }

    pub async fn get_backup_status(&self, backup_id: &str) -> Result<Option<BackupStatus>> {
        Ok(self.active_backups.read().await
            .get(backup_id)
            .map(|backup| backup.status.clone()))
    }

    pub async fn verify_backup(&self, backup_id: &str) -> Result<()> {
        let mut backups = self.active_backups.write().await;
        if let Some(backup) = backups.get_mut(backup_id) {
            backup.verification_status = Some(VerificationStatus::Pending);

            let verifier = self.verifier.clone();
            let backup_clone = backup.clone();
            let active_backups = self.active_backups.clone();

            tokio::spawn(async move {
                let result = verifier.verify_backup(&backup_clone).await;

                let mut backups = active_backups.write().await;
                if let Some(backup) = backups.get_mut(&backup_clone.id) {
                    backup.verification_status = Some(match result {
                        Ok(_) => VerificationStatus::Success,
                        Err(e) => VerificationStatus::Failed(e.to_string()),
                    });
                }
            });
        }
        Ok(())
    }

    pub async fn list_backups(&self) -> Result<Vec<BackupSet>> {
        Ok(self.active_backups.read().await
            .values()
            .cloned()
            .collect())
    }

    pub async fn prune_old_backups(&self) -> Result<()> {
        self.retention_manager.prune_backups(&self.storage).await
    }
}
