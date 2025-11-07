use std::path::PathBuf;
use tokio::sync::mpsc;
use chrono::{DateTime, Utc};
use skylock_core::Result;
use crate::restore::{RestoreManager, RestorePoint, RetentionPolicy};
use crate::dedup::{Deduplicator, StorageStats};
use serde::{Serialize, Deserialize};

pub struct BackupCoordinator {
    base_path: PathBuf,
    restore_manager: RestoreManager,
    deduplicator: Deduplicator,
    progress_tx: mpsc::Sender<BackupProgress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub retention_policy: RetentionPolicy,
    pub dedup_block_size: usize,
    pub compression_level: u32,
    pub encryption_enabled: bool,
    pub schedule: BackupSchedule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSchedule {
    pub full_backup_interval_days: i64,
    pub incremental_backup_interval_hours: i64,
    pub verification_interval_days: i64,
    pub optimization_interval_days: i64,
}

#[derive(Debug, Clone)]
pub enum BackupProgress {
    Started {
        operation: BackupOperation,
        timestamp: DateTime<Utc>,
    },
    Progress {
        operation: BackupOperation,
        files_processed: u64,
        total_files: u64,
        bytes_processed: u64,
        total_bytes: u64,
    },
    Completed {
        operation: BackupOperation,
        timestamp: DateTime<Utc>,
        stats: Option<StorageStats>,
    },
    Error {
        operation: BackupOperation,
        error: String,
    },
}

#[derive(Debug, Clone)]
pub enum BackupOperation {
    FullBackup,
    IncrementalBackup,
    Restore(String), // restore point id
    Verify(String),  // restore point id
    Optimize,
}

impl BackupCoordinator {
    pub fn new(
        base_path: PathBuf,
        config: BackupConfig,
        progress_tx: mpsc::Sender<BackupProgress>,
    ) -> Self {
        Self {
            base_path: base_path.clone(),
            restore_manager: RestoreManager::new(
                base_path.clone(),
                config.retention_policy,
            ),
            deduplicator: Deduplicator::new(
                base_path,
                config.dedup_block_size,
            ),
            progress_tx,
        }
    }

    pub async fn initialize(&mut self) -> Result<()> {
        // Load existing state
        self.restore_manager.load_restore_points().await?;
        self.deduplicator.load_state().await?;
        Ok(())
    }

    pub async fn create_full_backup(&mut self, description: String) -> Result<String> {
        self.progress_tx.send(BackupProgress::Started {
            operation: BackupOperation::FullBackup,
            timestamp: Utc::now(),
        }).await?;

        // Process all files through deduplicator
        let mut total_size = 0;
        let mut files_processed = 0;
        let mut stack = vec![self.base_path.clone()];

        // First pass to calculate totals
        {
            let mut count_stack = vec![self.base_path.clone()];
            while let Some(dir) = count_stack.pop() {
                let mut entries = tokio::fs::read_dir(&dir).await?;
                while let Some(entry) = entries.next_entry().await? {
                    let path = entry.path();
                    if path.is_dir() {
                        count_stack.push(path);
                    } else {
                        total_size += entry.metadata().await?.len();
                        files_processed += 1;
                    }
                }
            }
        }

        let mut current_files = 0;
        let mut current_bytes = 0;

        // Process files
        while let Some(dir) = stack.pop() {
            let mut entries = tokio::fs::read_dir(&dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();

                if path.is_dir() {
                    stack.push(path);
                    continue;
                }

                let file_size = entry.metadata().await?.len();
                self.deduplicator.process_file(&path).await?;

                current_files += 1;
                current_bytes += file_size;

                self.progress_tx.send(BackupProgress::Progress {
                    operation: BackupOperation::FullBackup,
                    files_processed: current_files,
                    total_files: files_processed,
                    bytes_processed: current_bytes,
                    total_bytes: total_size,
                }).await?;
            }
        }

        // Save deduplication state
        self.deduplicator.save_state().await?;

        // Create restore point
        let restore_point_id = self.restore_manager
            .create_restore_point(description)
            .await?;

        let stats = self.deduplicator.get_storage_stats();

        self.progress_tx.send(BackupProgress::Completed {
            operation: BackupOperation::FullBackup,
            timestamp: Utc::now(),
            stats: Some(stats),
        }).await?;

        Ok(restore_point_id)
    }

    pub async fn create_incremental_backup(&mut self) -> Result<String> {
        self.progress_tx.send(BackupProgress::Started {
            operation: BackupOperation::IncrementalBackup,
            timestamp: Utc::now(),
        }).await?;

        // Get last restore point for comparison
        let last_point = self.restore_manager.list_restore_points()
            .last()
            .cloned()
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No previous restore point found"
            ))?;

        let mut changed_files = Vec::new();
        let mut total_size = 0;

        // Scan for changed files
        let mut stack = vec![self.base_path.clone()];
        while let Some(dir) = stack.pop() {
            let mut entries = tokio::fs::read_dir(&dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();

                if path.is_dir() {
                    stack.push(path);
                    continue;
                }

                let metadata = entry.metadata().await?;
                let modified = DateTime::from(metadata.modified()?);

                // Check if file is newer than last backup
                if modified > last_point.timestamp {
                    changed_files.push(path);
                    total_size += metadata.len();
                }
            }
        }

        let total_files = changed_files.len() as u64;
        let mut current_files = 0;
        let mut current_bytes = 0;

        // Process changed files
        for path in changed_files {
            let file_size = tokio::fs::metadata(&path).await?.len();
            self.deduplicator.process_file(&path).await?;

            current_files += 1;
            current_bytes += file_size;

            self.progress_tx.send(BackupProgress::Progress {
                operation: BackupOperation::IncrementalBackup,
                files_processed: current_files,
                total_files,
                bytes_processed: current_bytes,
                total_bytes: total_size,
            }).await?;
        }

        // Save deduplication state
        self.deduplicator.save_state().await?;

        // Create restore point
        let description = format!("Incremental backup after {}", last_point.id);
        let restore_point_id = self.restore_manager
            .create_restore_point(description)
            .await?;

        let stats = self.deduplicator.get_storage_stats();

        self.progress_tx.send(BackupProgress::Completed {
            operation: BackupOperation::IncrementalBackup,
            timestamp: Utc::now(),
            stats: Some(stats),
        }).await?;

        Ok(restore_point_id)
    }

    pub async fn restore(&mut self, point_id: &str, target_path: Option<PathBuf>) -> Result<()> {
        self.progress_tx.send(BackupProgress::Started {
            operation: BackupOperation::Restore(point_id.to_string()),
            timestamp: Utc::now(),
        }).await?;

        self.restore_manager.restore(point_id, target_path).await?;

        self.progress_tx.send(BackupProgress::Completed {
            operation: BackupOperation::Restore(point_id.to_string()),
            timestamp: Utc::now(),
            stats: None,
        }).await?;

        Ok(())
    }

    pub async fn verify_restore_point(&mut self, point_id: &str) -> Result<bool> {
        self.progress_tx.send(BackupProgress::Started {
            operation: BackupOperation::Verify(point_id.to_string()),
            timestamp: Utc::now(),
        }).await?;

        let is_valid = self.restore_manager.verify_restore_point(point_id).await?;

        self.progress_tx.send(BackupProgress::Completed {
            operation: BackupOperation::Verify(point_id.to_string()),
            timestamp: Utc::now(),
            stats: None,
        }).await?;

        Ok(is_valid)
    }

    pub async fn optimize_storage(&mut self) -> Result<()> {
        self.progress_tx.send(BackupProgress::Started {
            operation: BackupOperation::Optimize,
            timestamp: Utc::now(),
        }).await?;

        self.deduplicator.optimize_storage().await?;
        self.deduplicator.save_state().await?;

        let stats = self.deduplicator.get_storage_stats();

        self.progress_tx.send(BackupProgress::Completed {
            operation: BackupOperation::Optimize,
            timestamp: Utc::now(),
            stats: Some(stats),
        }).await?;

        Ok(())
    }

    pub fn list_restore_points(&self) -> Vec<&RestorePoint> {
        self.restore_manager.list_restore_points()
    }

    pub fn get_storage_stats(&self) -> StorageStats {
        self.deduplicator.get_storage_stats()
    }
}
