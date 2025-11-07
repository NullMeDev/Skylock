//! Backup service module

use std::sync::Arc;
use skylock_core::{Result, SkylockError, BackupConfig};
use crate::platform::PlatformBackup;

pub struct BackupService {
    platform_backup: Arc<Box<dyn PlatformBackup + Send + Sync>>,
}

impl BackupService {
    pub fn new(platform_backup: Box<dyn PlatformBackup + Send + Sync>) -> Self {
        Self {
            platform_backup: Arc::new(platform_backup),
        }
    }
    
    pub async fn create_backup(&self, config: &BackupConfig) -> Result<String> {
        // Stub implementation for testing
        if config.backup_paths.is_empty() {
            return Err(SkylockError::Config("No backup paths specified".to_string()));
        }
        println!("Creating backup with {} paths, VSS enabled: {}", config.backup_paths.len(), config.vss_enabled);
        Ok("backup_123".to_string())
    }
    
    pub async fn list_backups(&self) -> Result<Vec<String>> {
        // Stub implementation for testing
        Ok(vec!["backup_123".to_string()])
    }
    
    pub async fn restore_backup(&self, backup_id: &str, destination: &str, dry_run: bool) -> Result<()> {
        // Stub implementation for testing
        println!("Restoring backup {} to {} (dry_run: {})", backup_id, destination, dry_run);
        Ok(())
    }
    
    pub async fn delete_backup(&self, backup_id: &str) -> Result<()> {
        // Stub implementation for testing
        println!("Deleting backup {}", backup_id);
        Ok(())
    }
}
