use std::path::{Path, PathBuf};
use crate::error::{Result, SkylockError};
use tracing::{info, warn};

pub struct VssSnapshot {
    source_path: PathBuf,
}

impl VssSnapshot {
    pub fn new(source_path: &Path) -> Result<Self> {
        info!("Creating backup without VSS for path: {:?}", source_path);
        warn!("VSS support is currently disabled - using direct file copy");

        Ok(Self {
            source_path: source_path.to_path_buf(),
        })
    }

    pub fn create(&self) -> Result<()> {
        info!("Preparing backup for path: {:?}", self.source_path);
        Ok(())
    }

    pub fn get_snapshot_path(&self, original_path: &Path) -> Result<PathBuf> {
        // In non-VSS mode, just return the original path
        Ok(original_path.to_path_buf())
    }

    pub fn cleanup(&self) -> Result<()> {
        Ok(())
    }
}
