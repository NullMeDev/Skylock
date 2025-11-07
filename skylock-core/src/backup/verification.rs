use std::collections::HashMap;
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use crate::Result;
use crate::error::{Error, ErrorCategory, ErrorSeverity};
use super::{BackupStorage, BackupSet};
use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use tracing::{debug, info, warn, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub size: u64,
    pub modified: DateTime<Utc>,
    pub blocks: Vec<String>,
    pub checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStatus {
    pub backup_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub total_blocks: usize,
    pub verified_blocks: usize,
    pub corrupted_blocks: Vec<String>,
    pub missing_blocks: Vec<String>,
    pub total_files: usize,
    pub verified_files: usize,
    pub failed_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub status: VerificationStatus,
    pub errors: Vec<Error>,
    pub is_complete: bool,
    pub is_consistent: bool,
}

#[derive(Debug)]
pub struct BackupVerifier {
    storage: BackupStorage,
}

impl BackupVerifier {
    pub fn new(storage: BackupStorage) -> Self {
        Self { storage }
    }

    #[tracing::instrument(skip(self, backup))]
    pub async fn verify_backup(&self, backup: &BackupSet) -> Result<VerificationResult> {
        info!("Starting backup verification for backup {}", backup.id);

        let mut status = VerificationStatus {
            backup_id: backup.id.clone(),
            started_at: Utc::now(),
            completed_at: None,
            total_blocks: 0,
            verified_blocks: 0,
            corrupted_blocks: Vec::new(),
            missing_blocks: Vec::new(),
            total_files: 0,
            verified_files: 0,
            failed_files: Vec::new(),
        };

        let mut errors = Vec::new();

        // Get backup metadata
        let metadata = match self.storage.get_metadata(&backup.id).await {
            Ok(Some(m)) => m,
            Ok(None) => {
                let error = Error::new(
                    ErrorCategory::Backup,
                    ErrorSeverity::High,
                    format!("Backup {} not found", backup.id),
                    "backup_verifier".to_string(),
                );
                errors.push(error.clone());
                return Ok(VerificationResult {
                    status,
                    errors: vec![error],
                    is_complete: false,
                    is_consistent: false,
                });
            }
            Err(e) => {
                errors.push(e.clone());
                return Ok(VerificationResult {
                    status,
                    errors: vec![e],
                    is_complete: false,
                    is_consistent: false,
                });
            }
        };

        status.total_files = metadata.len();
        let mut verified_blocks = HashMap::new();

        // Verify each file and its blocks
        for (file_path, file_info) in metadata.iter() {
            let file_info: FileInfo = match serde_json::from_str(file_info) {
                Ok(info) => info,
                Err(e) => {
                    let error = Error::new(
                        ErrorCategory::Backup,
                        ErrorSeverity::High,
                        format!("Failed to parse metadata for {}: {}", file_path.display(), e),
                        "backup_verifier".to_string(),
                    );
                    errors.push(error);
                    status.failed_files.push(file_path.clone());
                    continue;
                }
            };

            status.total_blocks += file_info.blocks.len();
            let mut file_verified = true;

            for block_hash in &file_info.blocks {
                if verified_blocks.contains_key(block_hash) {
                    status.verified_blocks += 1;
                    continue;
                }

                match self.verify_block(block_hash).await {
                    Ok(true) => {
                        status.verified_blocks += 1;
                        verified_blocks.insert(block_hash.clone(), true);
                    }
                    Ok(false) => {
                        file_verified = false;
                        status.corrupted_blocks.push(block_hash.clone());
                        let error = Error::new(
                            ErrorCategory::Backup,
                            ErrorSeverity::High,
                            format!("Block {} is corrupted", block_hash),
                            "backup_verifier".to_string(),
                        );
                        errors.push(error);
                    }
                    Err(e) => {
                        file_verified = false;
                        status.missing_blocks.push(block_hash.clone());
                        errors.push(e);
                    }
                }
            }

            if file_verified {
                status.verified_files += 1;
            } else {
                status.failed_files.push(file_path.clone());
            }
        }

        status.completed_at = Some(Utc::now());
        let is_complete = status.verified_blocks == status.total_blocks;
        let is_consistent = errors.is_empty();

        info!(
            "Backup verification completed: {}/{} blocks verified, {}/{} files verified",
            status.verified_blocks,
            status.total_blocks,
            status.verified_files,
            status.total_files
        );

        if !is_consistent {
            warn!(
                "Backup verification found {} errors, {} corrupted blocks, {} missing blocks",
                errors.len(),
                status.corrupted_blocks.len(),
                status.missing_blocks.len()
            );
        }

        Ok(VerificationResult {
            status,
            errors,
            is_complete,
            is_consistent,
        })
    }

    #[tracing::instrument(skip(self, hash))]
    async fn verify_block(&self, hash: &str) -> Result<bool> {
        // Get block data
        let block_data = match self.storage.get_block(hash).await? {
            Some(data) => data,
            None => {
                return Err(Error::new(
                    ErrorCategory::Backup,
                    ErrorSeverity::High,
                    format!("Block {} not found", hash),
                    "backup_verifier".to_string(),
                ));
            }
        };

        // Calculate hash of block data
        let mut hasher = Sha256::new();
        hasher.update(&block_data);
        let calculated_hash = hex::encode(hasher.finalize());

        // Compare hashes
        Ok(calculated_hash == hash)
    }
}
