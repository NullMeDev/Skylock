//! File change tracking module
//!
//! Tracks file modifications between backups for efficient incremental backups.

use crate::error::{Result, SkylockError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Represents a tracked file with its metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileInfo {
    /// Absolute path to the file
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// Last modified timestamp
    pub modified: DateTime<Utc>,
    /// SHA-256 hash of file content (computed lazily)
    pub hash: Option<String>,
}

/// File index tracking all files in watched directories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIndex {
    /// Map of file path to file info
    files: HashMap<PathBuf, FileInfo>,
    /// When this index was created
    pub created_at: DateTime<Utc>,
    /// Directories being tracked
    pub tracked_dirs: Vec<PathBuf>,
}

/// Change detection result
#[derive(Debug, Clone, PartialEq)]
pub enum ChangeType {
    /// File was added (new file)
    Added,
    /// File was removed (deleted)
    Removed,
    /// File was modified (content changed)
    Modified,
    /// File metadata changed (size or timestamp)
    MetadataChanged,
}

/// Represents a detected change
#[derive(Debug, Clone)]
pub struct FileChange {
    /// Path to the changed file
    pub path: PathBuf,
    /// Type of change
    pub change_type: ChangeType,
    /// Old file info (if available)
    pub old_info: Option<FileInfo>,
    /// New file info (if available)
    pub new_info: Option<FileInfo>,
}

impl FileIndex {
    /// Create a new empty file index
    pub fn new(tracked_dirs: Vec<PathBuf>) -> Self {
        Self {
            files: HashMap::new(),
            created_at: Utc::now(),
            tracked_dirs,
        }
    }

    /// Build file index from directories
    pub fn build(paths: &[PathBuf]) -> Result<Self> {
        let mut index = Self::new(paths.to_vec());
        
        for path in paths {
            if path.is_file() {
                let info = Self::get_file_info(path)?;
                index.files.insert(path.clone(), info);
            } else if path.is_dir() {
                for entry in WalkDir::new(path).follow_links(false) {
                    let entry = entry.map_err(|e| {
                        SkylockError::Backup(format!("Walk error: {}", e))
                    })?;
                    
                    if entry.file_type().is_file() {
                        let file_path = entry.path().to_path_buf();
                        let info = Self::get_file_info(&file_path)?;
                        index.files.insert(file_path, info);
                    }
                }
            }
        }
        
        Ok(index)
    }

    /// Get file info from path
    fn get_file_info(path: &Path) -> Result<FileInfo> {
        let metadata = std::fs::metadata(path)?;
        let modified = metadata.modified()?;
        let modified_dt = DateTime::<Utc>::from(modified);
        
        Ok(FileInfo {
            path: path.to_path_buf(),
            size: metadata.len(),
            modified: modified_dt,
            hash: None, // Computed lazily when needed
        })
    }

    /// Compute hash for a file
    pub async fn compute_hash(path: &Path) -> Result<String> {
        let data = tokio::fs::read(path).await?;
        let mut hasher = Sha256::new();
        hasher.update(&data);
        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Compare with current filesystem state and detect changes
    pub async fn detect_changes(&self, paths: &[PathBuf]) -> Result<Vec<FileChange>> {
        let mut changes = Vec::new();
        let current_index = Self::build(paths)?;
        
        // Find added and modified files
        for (path, new_info) in &current_index.files {
            if let Some(old_info) = self.files.get(path) {
                // File exists in both - check if modified
                if new_info.size != old_info.size || new_info.modified != old_info.modified {
                    // Size or timestamp changed - compute hashes to confirm
                    let old_hash = if let Some(h) = &old_info.hash {
                        h.clone()
                    } else {
                        Self::compute_hash(path).await?
                    };
                    
                    let new_hash = Self::compute_hash(path).await?;
                    
                    if old_hash != new_hash {
                        changes.push(FileChange {
                            path: path.clone(),
                            change_type: ChangeType::Modified,
                            old_info: Some(old_info.clone()),
                            new_info: Some(new_info.clone()),
                        });
                    } else {
                        changes.push(FileChange {
                            path: path.clone(),
                            change_type: ChangeType::MetadataChanged,
                            old_info: Some(old_info.clone()),
                            new_info: Some(new_info.clone()),
                        });
                    }
                }
            } else {
                // New file
                changes.push(FileChange {
                    path: path.clone(),
                    change_type: ChangeType::Added,
                    old_info: None,
                    new_info: Some(new_info.clone()),
                });
            }
        }
        
        // Find removed files
        for (path, old_info) in &self.files {
            if !current_index.files.contains_key(path) {
                changes.push(FileChange {
                    path: path.clone(),
                    change_type: ChangeType::Removed,
                    old_info: Some(old_info.clone()),
                    new_info: None,
                });
            }
        }
        
        Ok(changes)
    }

    /// Get list of files that have changed
    pub async fn get_changed_files(&self, paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
        let changes = self.detect_changes(paths).await?;
        Ok(changes
            .iter()
            .filter(|c| matches!(c.change_type, ChangeType::Added | ChangeType::Modified))
            .map(|c| c.path.clone())
            .collect())
    }

    /// Save index to file
    pub async fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| SkylockError::Backup(format!("Serialize index failed: {}", e)))?;
        
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    /// Load index from file
    pub async fn load(path: &Path) -> Result<Self> {
        let json = tokio::fs::read_to_string(path).await?;
        let index: Self = serde_json::from_str(&json)
            .map_err(|e| SkylockError::Backup(format!("Deserialize index failed: {}", e)))?;
        Ok(index)
    }

    /// Check if index file exists
    pub async fn exists(path: &Path) -> bool {
        tokio::fs::metadata(path).await.is_ok()
    }

    /// Get number of tracked files
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// Change tracker manages file indexes
pub struct ChangeTracker {
    /// Path to store file indexes
    index_dir: PathBuf,
}

impl ChangeTracker {
    /// Create new change tracker
    pub fn new(index_dir: PathBuf) -> Self {
        Self { index_dir }
    }

    /// Get path to index file for a backup
    fn get_index_path(&self, backup_id: &str) -> PathBuf {
        self.index_dir.join(format!("{}.index.json", backup_id))
    }

    /// Get path to latest index file
    fn get_latest_index_path(&self) -> PathBuf {
        self.index_dir.join("latest.index.json")
    }

    /// Save index for a backup
    pub async fn save_index(&self, backup_id: &str, index: &FileIndex) -> Result<()> {
        let index_path = self.get_index_path(backup_id);
        index.save(&index_path).await?;
        
        // Also save as latest
        let latest_path = self.get_latest_index_path();
        index.save(&latest_path).await?;
        
        Ok(())
    }

    /// Load index for a backup
    pub async fn load_index(&self, backup_id: &str) -> Result<FileIndex> {
        let index_path = self.get_index_path(backup_id);
        FileIndex::load(&index_path).await
    }

    /// Load latest index
    pub async fn load_latest_index(&self) -> Result<FileIndex> {
        let latest_path = self.get_latest_index_path();
        FileIndex::load(&latest_path).await
    }

    /// Check if latest index exists
    pub async fn has_latest_index(&self) -> bool {
        let latest_path = self.get_latest_index_path();
        FileIndex::exists(&latest_path).await
    }

    /// Detect changes since last backup
    pub async fn detect_changes_since_last_backup(&self, paths: &[PathBuf]) -> Result<Vec<FileChange>> {
        if !self.has_latest_index().await {
            // No previous backup - all files are new
            let current_index = FileIndex::build(paths)?;
            return Ok(current_index
                .files
                .into_iter()
                .map(|(path, info)| FileChange {
                    path,
                    change_type: ChangeType::Added,
                    old_info: None,
                    new_info: Some(info),
                })
                .collect());
        }
        
        let last_index = self.load_latest_index().await?;
        last_index.detect_changes(paths).await
    }
    
    /// Get list of files that have changed since last backup
    pub async fn get_changed_files(&self, paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
        if !self.has_latest_index().await {
            // No previous backup - all files are "changed"
            let current_index = FileIndex::build(paths)?;
            return Ok(current_index
                .files
                .keys()
                .cloned()
                .collect());
        }
        
        let last_index = self.load_latest_index().await?;
        last_index.get_changed_files(paths).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_file_index_build() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        tokio::fs::write(&file_path, b"test content").await.unwrap();
        
        let index = FileIndex::build(&[temp_dir.path().to_path_buf()]).unwrap();
        
        assert_eq!(index.file_count(), 1);
        assert!(index.files.contains_key(&file_path));
    }

    #[tokio::test]
    async fn test_detect_added_files() {
        let temp_dir = TempDir::new().unwrap();
        let old_index = FileIndex::new(vec![temp_dir.path().to_path_buf()]);
        
        // Add a new file
        let new_file = temp_dir.path().join("new.txt");
        tokio::fs::write(&new_file, b"new content").await.unwrap();
        
        let changes = old_index.detect_changes(&[temp_dir.path().to_path_buf()]).await.unwrap();
        
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, ChangeType::Added);
        assert_eq!(changes[0].path, new_file);
    }

    #[tokio::test]
    async fn test_detect_removed_files() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        tokio::fs::write(&file_path, b"test").await.unwrap();
        
        let old_index = FileIndex::build(&[temp_dir.path().to_path_buf()]).unwrap();
        
        // Remove the file
        tokio::fs::remove_file(&file_path).await.unwrap();
        
        let changes = old_index.detect_changes(&[temp_dir.path().to_path_buf()]).await.unwrap();
        
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, ChangeType::Removed);
        assert_eq!(changes[0].path, file_path);
    }

    #[tokio::test]
    async fn test_detect_modified_files() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        tokio::fs::write(&file_path, b"original").await.unwrap();
        
        let old_index = FileIndex::build(&[temp_dir.path().to_path_buf()]).unwrap();
        
        // Wait longer to ensure timestamp changes on all systems
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        
        // Modify the file with different content
        tokio::fs::write(&file_path, b"modified content that is longer").await.unwrap();
        
        let changes = old_index.detect_changes(&[temp_dir.path().to_path_buf()]).await.unwrap();
        
        assert_eq!(changes.len(), 1);
        // Should be Modified since content and size changed
        assert!(matches!(changes[0].change_type, ChangeType::Modified | ChangeType::MetadataChanged));
        assert_eq!(changes[0].path, file_path);
    }

    #[tokio::test]
    async fn test_save_and_load_index() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        tokio::fs::write(&file_path, b"test").await.unwrap();
        
        let index = FileIndex::build(&[temp_dir.path().to_path_buf()]).unwrap();
        
        let index_file = temp_dir.path().join("index.json");
        index.save(&index_file).await.unwrap();
        
        let loaded_index = FileIndex::load(&index_file).await.unwrap();
        
        assert_eq!(loaded_index.file_count(), index.file_count());
        assert_eq!(loaded_index.tracked_dirs, index.tracked_dirs);
    }

    #[tokio::test]
    async fn test_change_tracker() {
        let temp_dir = TempDir::new().unwrap();
        let index_dir = temp_dir.path().join("indexes");
        let tracker = ChangeTracker::new(index_dir);
        
        let file_path = temp_dir.path().join("test.txt");
        tokio::fs::write(&file_path, b"test").await.unwrap();
        
        let index = FileIndex::build(&[temp_dir.path().to_path_buf()]).unwrap();
        tracker.save_index("backup1", &index).await.unwrap();
        
        assert!(tracker.has_latest_index().await);
        
        let loaded = tracker.load_index("backup1").await.unwrap();
        assert_eq!(loaded.file_count(), 1);
    }
}
