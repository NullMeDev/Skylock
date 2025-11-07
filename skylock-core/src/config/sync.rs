use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use chrono::Duration;
use crate::sync::{ConflictResolution, ChecksumAlgorithm};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSettings {
    /// Directories to synchronize
    pub directories: Vec<PathBuf>,

    /// Patterns to ignore during sync
    pub ignore_patterns: Vec<String>,

    /// How often to sync in seconds
    pub sync_interval_secs: i64,

    /// Maximum number of files to sync in a batch
    pub batch_size: usize,

    /// How to resolve conflicts
    pub conflict_resolution: ConflictResolution,

    /// Which checksum algorithm to use
    pub checksum_algorithm: ChecksumAlgorithm,

    /// Remote sync base directory
    pub remote_base: PathBuf,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            directories: vec![],
            ignore_patterns: vec![
                String::from("*.tmp"),
                String::from("*.temp"),
                String::from("~*"),
                String::from(".git/"),
                String::from("node_modules/"),
            ],
            sync_interval_secs: 300, // 5 minutes
            batch_size: 1000,
            conflict_resolution: ConflictResolution::KeepNewest,
            checksum_algorithm: ChecksumAlgorithm::XXHash,
            remote_base: PathBuf::from("/remote/sync"),
        }
    }
}

impl SyncSettings {
    pub fn sync_interval(&self) -> Duration {
        Duration::seconds(self.sync_interval_secs)
    }
}
