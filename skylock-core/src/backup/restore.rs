use std::path::PathBuf;
use tokio::fs::{File, create_dir_all};
use tokio::io::AsyncWriteExt;
use std::collections::HashMap;
use crate::Result;
use super::{BackupStorage, BackupSet};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct RestoreManager {
    storage: BackupStorage,
}

impl RestoreManager {
    pub fn new(storage: BackupStorage) -> Result<Self> {
        Ok(Self { storage })
    }

    pub async fn restore_backup(
        &self,
        backup_id: &str,
        target_path: &PathBuf,
        filters: Option<RestoreFilters>,
    ) -> Result<()> {
        // Get backup metadata
        let metadata = self.storage.get_metadata(backup_id).await?
            .ok_or_else(|| crate::error::SkylockError::Backup(
                format!("Backup {} not found", backup_id)
            ))?;

        // Create target directory if it doesn't exist
        create_dir_all(target_path).await?;

        // Process each file in the backup
        for (file_path, file_info) in metadata.iter() {
            let file_info: FileInfo = serde_json::from_str(file_info)?;

            // Apply filters if any
            if let Some(ref filters) = filters {
                if !self.should_restore_file(file_path, &file_info, filters) {
                    continue;
                }
            }

            // Reconstruct file from blocks
            self.restore_file(file_path, &file_info, target_path).await?;
        }

        Ok(())
    }

    async fn restore_file(
        &self,
        file_path: &str,
        file_info: &FileInfo,
        target_path: &PathBuf,
    ) -> Result<()> {
        let target_file = target_path.join(file_path);

        // Create parent directories if needed
        if let Some(parent) = target_file.parent() {
            create_dir_all(parent).await?;
        }

        let mut file = File::create(&target_file).await?;

        // Restore file from blocks
        for block_hash in &file_info.blocks {
            if let Some(block_data) = self.storage.get_block(block_hash).await? {
                file.write_all(&block_data).await?;
            } else {
                return Err(crate::error::SkylockError::Backup(
                    format!("Block {} not found", block_hash)
                ));
            }
        }

        // Set file metadata
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file_info.unix_mode {
                let permissions = std::fs::Permissions::from_mode(mode);
                tokio::fs::set_permissions(&target_file, permissions).await?;
            }
        }

        Ok(())
    }

    fn should_restore_file(
        &self,
        file_path: &str,
        file_info: &FileInfo,
        filters: &RestoreFilters,
    ) -> bool {
        // Check path filters
        if let Some(ref include_paths) = filters.include_paths {
            if !include_paths.iter().any(|p| file_path.starts_with(p)) {
                return false;
            }
        }

        if let Some(ref exclude_paths) = filters.exclude_paths {
            if exclude_paths.iter().any(|p| file_path.starts_with(p)) {
                return false;
            }
        }

        // Check time filters
        if let Some(after) = filters.modified_after {
            if file_info.modified < after {
                return false;
            }
        }

        if let Some(before) = filters.modified_before {
            if file_info.modified > before {
                return false;
            }
        }

        true
    }
}

#[derive(Debug, Clone)]
pub struct RestoreFilters {
    pub include_paths: Option<Vec<String>>,
    pub exclude_paths: Option<Vec<String>>,
    pub modified_after: Option<DateTime<Utc>>,
    pub modified_before: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileInfo {
    modified: DateTime<Utc>,
    size: u64,
    blocks: Vec<String>,
    #[cfg(unix)]
    unix_mode: Option<u32>,
}
