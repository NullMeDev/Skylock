//! Resume state management for interrupted uploads
//!
//! Tracks which files have been successfully uploaded so we can resume
//! from where we left off if the backup is interrupted.

use std::path::{Path, PathBuf};
use std::collections::HashSet;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::error::{Result, SkylockError};

/// State file for tracking upload progress
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResumeState {
    /// Backup ID being created
    pub backup_id: String,
    
    /// When the backup started
    pub started_at: DateTime<Utc>,
    
    /// Source paths being backed up
    pub source_paths: Vec<PathBuf>,
    
    /// Files that have been successfully uploaded (local paths)
    pub uploaded_files: HashSet<PathBuf>,
    
    /// Total number of files to upload
    pub total_files: usize,
    
    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
}

impl ResumeState {
    /// Create a new resume state
    pub fn new(backup_id: String, source_paths: Vec<PathBuf>, total_files: usize) -> Self {
        Self {
            backup_id,
            started_at: Utc::now(),
            source_paths,
            uploaded_files: HashSet::new(),
            total_files,
            last_updated: Utc::now(),
        }
    }
    
    /// Get the state file path for a backup ID
    pub fn state_file_path(backup_id: &str) -> PathBuf {
        let data_dir = directories::ProjectDirs::from("com", "skylock", "skylock-hybrid")
            .map(|dirs| dirs.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        
        data_dir.join("resume_state").join(format!("{}.json", backup_id))
    }
    
    /// Check if a state file exists for the given backup ID
    pub async fn exists(backup_id: &str) -> bool {
        Self::state_file_path(backup_id).exists()
    }
    
    /// Load resume state from disk
    pub async fn load(backup_id: &str) -> Result<Self> {
        let path = Self::state_file_path(backup_id);
        
        if !path.exists() {
            return Err(SkylockError::Backup(format!(
                "Resume state not found for backup {}",
                backup_id
            )));
        }
        
        let json = fs::read_to_string(&path).await
            .map_err(|e| SkylockError::Backup(format!("Failed to read resume state: {}", e)))?;
        
        let state: ResumeState = serde_json::from_str(&json)
            .map_err(|e| SkylockError::Backup(format!("Failed to parse resume state: {}", e)))?;
        
        Ok(state)
    }
    
    /// Save resume state to disk
    pub async fn save(&self) -> Result<()> {
        let path = Self::state_file_path(&self.backup_id);
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| SkylockError::Backup(format!("Failed to create state directory: {}", e)))?;
        }
        
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| SkylockError::Backup(format!("Failed to serialize state: {}", e)))?;
        
        // Write atomically using a temp file
        let temp_path = path.with_extension("json.tmp");
        
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_path)
            .await
            .map_err(|e| SkylockError::Backup(format!("Failed to create temp state file: {}", e)))?;
        
        file.write_all(json.as_bytes()).await
            .map_err(|e| SkylockError::Backup(format!("Failed to write state: {}", e)))?;
        
        file.sync_all().await
            .map_err(|e| SkylockError::Backup(format!("Failed to sync state file: {}", e)))?;
        
        drop(file);
        
        // Atomic rename
        fs::rename(&temp_path, &path).await
            .map_err(|e| SkylockError::Backup(format!("Failed to rename state file: {}", e)))?;
        
        Ok(())
    }
    
    /// Mark a file as uploaded
    pub fn mark_uploaded(&mut self, file_path: PathBuf) {
        self.uploaded_files.insert(file_path);
        self.last_updated = Utc::now();
    }
    
    /// Check if a file has been uploaded
    pub fn is_uploaded(&self, file_path: &Path) -> bool {
        self.uploaded_files.contains(file_path)
    }
    
    /// Get the number of files uploaded
    pub fn uploaded_count(&self) -> usize {
        self.uploaded_files.len()
    }
    
    /// Get the progress percentage
    pub fn progress_percent(&self) -> f64 {
        if self.total_files == 0 {
            return 0.0;
        }
        (self.uploaded_count() as f64 / self.total_files as f64) * 100.0
    }
    
    /// Delete the resume state file
    pub async fn delete(backup_id: &str) -> Result<()> {
        let path = Self::state_file_path(backup_id);
        
        if path.exists() {
            fs::remove_file(&path).await
                .map_err(|e| SkylockError::Backup(format!("Failed to delete resume state: {}", e)))?;
        }
        
        Ok(())
    }
    
    /// Clean up old resume state files (older than 7 days)
    pub async fn cleanup_old_states(days: u64) -> Result<usize> {
        let state_dir = directories::ProjectDirs::from("com", "skylock", "skylock-hybrid")
            .map(|dirs| dirs.data_dir().join("resume_state"))
            .unwrap_or_else(|| PathBuf::from("./resume_state"));
        
        if !state_dir.exists() {
            return Ok(0);
        }
        
        let cutoff = Utc::now() - chrono::Duration::days(days as i64);
        let mut cleaned = 0;
        
        let mut entries = fs::read_dir(&state_dir).await
            .map_err(|e| SkylockError::Backup(format!("Failed to read state directory: {}", e)))?;
        
        while let Some(entry) = entries.next_entry().await
            .map_err(|e| SkylockError::Backup(format!("Failed to read directory entry: {}", e)))? {
            
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                // Try to read and check age
                match fs::read_to_string(&path).await {
                    Ok(json) => {
                        if let Ok(state) = serde_json::from_str::<ResumeState>(&json) {
                            if state.last_updated < cutoff {
                                if fs::remove_file(&path).await.is_ok() {
                                    cleaned += 1;
                                }
                            }
                        }
                    }
                    Err(_) => {} // Ignore read errors
                }
            }
        }
        
        Ok(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_resume_state_save_load() {
        let backup_id = "test_20251107_123456".to_string();
        let source_paths = vec![PathBuf::from("/test/path1"), PathBuf::from("/test/path2")];
        
        let mut state = ResumeState::new(backup_id.clone(), source_paths.clone(), 10);
        state.mark_uploaded(PathBuf::from("/test/path1/file1.txt"));
        state.mark_uploaded(PathBuf::from("/test/path1/file2.txt"));
        
        // Save
        state.save().await.unwrap();
        
        // Load
        let loaded = ResumeState::load(&backup_id).await.unwrap();
        
        assert_eq!(loaded.backup_id, backup_id);
        assert_eq!(loaded.source_paths, source_paths);
        assert_eq!(loaded.uploaded_count(), 2);
        assert!(loaded.is_uploaded(&PathBuf::from("/test/path1/file1.txt")));
        assert!(!loaded.is_uploaded(&PathBuf::from("/test/path1/file3.txt")));
        
        // Cleanup
        ResumeState::delete(&backup_id).await.unwrap();
    }
    
    #[test]
    fn test_progress_calculation() {
        let mut state = ResumeState::new(
            "test".to_string(),
            vec![],
            100
        );
        
        assert_eq!(state.progress_percent(), 0.0);
        
        for i in 0..50 {
            state.mark_uploaded(PathBuf::from(format!("/file{}.txt", i)));
        }
        
        assert_eq!(state.progress_percent(), 50.0);
    }
}
