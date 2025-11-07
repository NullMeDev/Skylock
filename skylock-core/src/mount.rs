use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use crate::{Result, error::SkylockError};
use crate::virtual_drive::{VirtualDrive, DriveConfig};
use crate::storage::StorageProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountConfig {
    pub drives: HashMap<String, DriveConfig>,
    pub global_cache_limit_mb: u64,
    pub enable_network_discovery: bool,
}

#[derive(Debug)]
pub struct MountManager {
    config: MountConfig,
    drives: Arc<RwLock<HashMap<String, Arc<VirtualDrive>>>>,
}

impl MountManager {
    pub fn new(config: MountConfig) -> Self {
        Self {
            config,
            drives: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn mount_drive(
        &self,
        name: &str,
        storage: Box<dyn StorageProvider>,
    ) -> Result<()> {
        let config = self.config.drives.get(name).ok_or_else(|| {
            SkylockError::Config(format!("No configuration found for drive {}", name))
        })?.clone();

        let drive = VirtualDrive::new(config, storage).await?;
        drive.mount().await?;

        self.drives.write().await.insert(name.to_string(), Arc::new(drive));

        Ok(())
    }

    pub async fn unmount_drive(&self, name: &str) -> Result<()> {
        self.drives.write().await.remove(name).ok_or_else(|| {
            SkylockError::NotFound(format!("Drive {} not found", name))
        })?;

        Ok(())
    }

    pub async fn get_drive(&self, name: &str) -> Result<Arc<VirtualDrive>> {
        self.drives.read().await.get(name).cloned().ok_or_else(|| {
            SkylockError::NotFound(format!("Drive {} not found", name))
        })
    }

    pub async fn list_drives(&self) -> Vec<String> {
        self.drives.read().await.keys().cloned().collect()
    }

    pub async fn sync_all(&self) -> Result<()> {
        let drives = self.drives.read().await;
        for drive in drives.values() {
            // Trigger sync on each drive
            // Implementation will depend on the sync mode of each drive
        }
        Ok(())
    }
}
