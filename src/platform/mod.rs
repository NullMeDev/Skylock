//! Platform-specific functionality
//! 
//! This module provides cross-platform abstractions for OS-specific features
//! like Volume Shadow Copy on Windows or LVM snapshots on Linux.

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

#[cfg(windows)]
pub mod windows;
#[cfg(unix)]
pub mod unix;

/// Platform-specific backup functionality
#[async_trait]
pub trait PlatformBackup: Send + Sync {
    /// Create a platform-specific snapshot for backup
    async fn create_snapshot(&self, path: &Path) -> Result<String>;
    
    /// Delete a platform-specific snapshot
    async fn delete_snapshot(&self, snapshot_id: &str) -> Result<()>;
    
    /// Check if platform supports snapshotting
    fn supports_snapshots(&self) -> bool;
}

/// Get the platform-specific backup implementation
#[cfg(windows)]
pub fn get_platform_backup() -> Box<dyn PlatformBackup + Send + Sync> {
    Box::new(windows::WindowsBackup::new())
}

#[cfg(unix)]
pub fn get_platform_backup() -> Box<dyn PlatformBackup + Send + Sync> {
    Box::new(unix::UnixBackup::new())
}

/// Platform-specific path utilities
pub mod path {
    use std::path::{Path, PathBuf};
    
    /// Get the default configuration directory for the application
    pub fn config_dir() -> Option<PathBuf> {
        directories::ProjectDirs::from("com", "skylock", "skylock-hybrid")
            .map(|dirs| dirs.config_dir().to_path_buf())
    }
    
    /// Get the default data directory for the application  
    pub fn data_dir() -> Option<PathBuf> {
        directories::ProjectDirs::from("com", "skylock", "skylock-hybrid")
            .map(|dirs| dirs.data_dir().to_path_buf())
    }
    
    /// Normalize a path for the current platform
    pub fn normalize_path(path: &Path) -> PathBuf {
        // Convert to absolute path and normalize separators
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }
    
    /// Check if a path is accessible
    pub fn is_accessible(path: &Path) -> bool {
        path.exists() && path.metadata().is_ok()
    }
}
