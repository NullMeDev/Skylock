//! SQLite-based Sync State Tracking
//!
//! Tracks file states, modification times, and sync history using SQLite.
//! Persists state across restarts for efficient incremental syncing.

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error, debug};

/// File state stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileState {
    /// Absolute path to the file
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// Local modification time
    pub local_mtime: DateTime<Utc>,
    /// Remote modification time (if known)
    pub remote_mtime: Option<DateTime<Utc>>,
    /// SHA-256 hash of file content
    pub content_hash: Option<String>,
    /// Last time this file was synced
    pub last_synced: Option<DateTime<Utc>>,
    /// Current sync status
    pub status: SyncStatus,
    /// Number of sync attempts
    pub sync_attempts: u32,
    /// Last error message (if any)
    pub last_error: Option<String>,
}

/// Status of a file's synchronization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncStatus {
    /// File has never been synced
    New,
    /// File is up to date
    Synced,
    /// Local file has changes that need to be uploaded
    Modified,
    /// File was deleted locally
    Deleted,
    /// Sync is currently in progress
    Syncing,
    /// Sync failed
    Failed,
    /// File is in conflict (both local and remote changed)
    Conflict,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self::New
    }
}

/// Sync history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncHistoryEntry {
    /// Unique ID
    pub id: i64,
    /// File path
    pub path: PathBuf,
    /// Action taken
    pub action: SyncAction,
    /// Whether it succeeded
    pub success: bool,
    /// Bytes transferred
    pub bytes_transferred: u64,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Error message if failed
    pub error: Option<String>,
    /// When the sync occurred
    pub synced_at: DateTime<Utc>,
}

/// Types of sync actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncAction {
    Upload,
    Download,
    Delete,
    Rename,
}

/// Configuration for the sync state database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStateConfig {
    /// Path to the SQLite database file
    pub db_path: PathBuf,
    /// How long to keep history entries (in days)
    pub history_retention_days: u32,
    /// Maximum history entries to keep
    pub max_history_entries: usize,
}

impl Default for SyncStateConfig {
    fn default() -> Self {
        let data_dir = directories::ProjectDirs::from("dev", "skylock", "skylock-hybrid")
            .map(|d| d.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        
        Self {
            db_path: data_dir.join("sync_state.db"),
            history_retention_days: 30,
            max_history_entries: 100000,
        }
    }
}

/// In-memory sync state manager (SQLite simulation)
/// In production, this would use rusqlite or sqlx
pub struct SyncStateManager {
    config: SyncStateConfig,
    /// File states indexed by path
    states: HashMap<PathBuf, FileState>,
    /// Sync history
    history: Vec<SyncHistoryEntry>,
    /// Next history ID
    next_history_id: i64,
    /// Whether state has changed since last save
    dirty: bool,
}

impl SyncStateManager {
    /// Create a new sync state manager
    pub fn new(config: SyncStateConfig) -> Result<Self, SyncStateError> {
        let mut manager = Self {
            config,
            states: HashMap::new(),
            history: Vec::new(),
            next_history_id: 1,
            dirty: false,
        };

        // Try to load existing state
        if manager.config.db_path.exists() {
            manager.load()?;
        }

        Ok(manager)
    }

    /// Load state from disk
    fn load(&mut self) -> Result<(), SyncStateError> {
        let data = std::fs::read_to_string(&self.config.db_path)
            .map_err(|e| SyncStateError::IoError(e))?;
        
        let saved: SavedState = serde_json::from_str(&data)
            .map_err(|e| SyncStateError::ParseError(e.to_string()))?;
        
        self.states = saved.states;
        self.history = saved.history;
        self.next_history_id = saved.next_history_id;
        self.dirty = false;
        
        info!("Loaded sync state: {} files tracked", self.states.len());
        Ok(())
    }

    /// Save state to disk
    pub fn save(&mut self) -> Result<(), SyncStateError> {
        if !self.dirty {
            return Ok(());
        }

        // Ensure parent directory exists
        if let Some(parent) = self.config.db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| SyncStateError::IoError(e))?;
        }

        let saved = SavedState {
            states: self.states.clone(),
            history: self.history.clone(),
            next_history_id: self.next_history_id,
        };

        let data = serde_json::to_string_pretty(&saved)
            .map_err(|e| SyncStateError::ParseError(e.to_string()))?;
        
        std::fs::write(&self.config.db_path, data)
            .map_err(|e| SyncStateError::IoError(e))?;
        
        self.dirty = false;
        debug!("Saved sync state: {} files", self.states.len());
        Ok(())
    }

    /// Get the state of a file
    pub fn get_state(&self, path: &Path) -> Option<&FileState> {
        self.states.get(path)
    }

    /// Get all file states
    pub fn all_states(&self) -> impl Iterator<Item = &FileState> {
        self.states.values()
    }

    /// Get files with a specific status
    pub fn files_with_status(&self, status: SyncStatus) -> Vec<&FileState> {
        self.states.values()
            .filter(|s| s.status == status)
            .collect()
    }

    /// Update or insert a file state
    pub fn upsert_state(&mut self, state: FileState) {
        self.states.insert(state.path.clone(), state);
        self.dirty = true;
    }

    /// Mark a file as modified
    pub fn mark_modified(&mut self, path: &Path, size: u64, mtime: DateTime<Utc>) {
        if let Some(state) = self.states.get_mut(path) {
            state.size = size;
            state.local_mtime = mtime;
            state.status = SyncStatus::Modified;
            self.dirty = true;
        } else {
            self.upsert_state(FileState {
                path: path.to_path_buf(),
                size,
                local_mtime: mtime,
                remote_mtime: None,
                content_hash: None,
                last_synced: None,
                status: SyncStatus::New,
                sync_attempts: 0,
                last_error: None,
            });
        }
    }

    /// Mark a file as deleted
    pub fn mark_deleted(&mut self, path: &Path) {
        if let Some(state) = self.states.get_mut(path) {
            state.status = SyncStatus::Deleted;
            self.dirty = true;
        }
    }

    /// Mark a file as syncing
    pub fn mark_syncing(&mut self, path: &Path) {
        if let Some(state) = self.states.get_mut(path) {
            state.status = SyncStatus::Syncing;
            state.sync_attempts += 1;
            self.dirty = true;
        }
    }

    /// Mark a file as synced
    pub fn mark_synced(&mut self, path: &Path, content_hash: Option<String>) {
        if let Some(state) = self.states.get_mut(path) {
            state.status = SyncStatus::Synced;
            state.last_synced = Some(Utc::now());
            state.content_hash = content_hash;
            state.last_error = None;
            self.dirty = true;
        }
    }

    /// Mark a file sync as failed
    pub fn mark_failed(&mut self, path: &Path, error: String) {
        if let Some(state) = self.states.get_mut(path) {
            state.status = SyncStatus::Failed;
            state.last_error = Some(error);
            self.dirty = true;
        }
    }

    /// Mark a file as having a conflict
    pub fn mark_conflict(&mut self, path: &Path) {
        if let Some(state) = self.states.get_mut(path) {
            state.status = SyncStatus::Conflict;
            self.dirty = true;
        }
    }

    /// Remove a file from tracking
    pub fn remove(&mut self, path: &Path) -> Option<FileState> {
        self.dirty = true;
        self.states.remove(path)
    }

    /// Add a history entry
    pub fn add_history(&mut self, entry: SyncHistoryEntry) {
        let mut entry = entry;
        entry.id = self.next_history_id;
        self.next_history_id += 1;
        self.history.push(entry);
        self.dirty = true;
        
        // Prune old history if needed
        self.prune_history();
    }

    /// Record a sync operation
    pub fn record_sync(
        &mut self,
        path: &Path,
        action: SyncAction,
        success: bool,
        bytes: u64,
        duration_ms: u64,
        error: Option<String>,
    ) {
        self.add_history(SyncHistoryEntry {
            id: 0, // Will be set in add_history
            path: path.to_path_buf(),
            action,
            success,
            bytes_transferred: bytes,
            duration_ms,
            error,
            synced_at: Utc::now(),
        });
    }

    /// Get recent history entries
    pub fn recent_history(&self, limit: usize) -> Vec<&SyncHistoryEntry> {
        self.history.iter().rev().take(limit).collect()
    }

    /// Get history for a specific file
    pub fn file_history(&self, path: &Path) -> Vec<&SyncHistoryEntry> {
        self.history.iter()
            .filter(|e| e.path == path)
            .collect()
    }

    /// Prune old history entries
    fn prune_history(&mut self) {
        // Remove entries exceeding max count
        while self.history.len() > self.config.max_history_entries {
            self.history.remove(0);
        }

        // Remove entries older than retention period
        let cutoff = Utc::now() - chrono::Duration::days(self.config.history_retention_days as i64);
        self.history.retain(|e| e.synced_at > cutoff);
    }

    /// Get sync statistics
    pub fn stats(&self) -> SyncStats {
        let mut stats = SyncStats::default();
        
        for state in self.states.values() {
            stats.total_files += 1;
            stats.total_bytes += state.size;
            
            match state.status {
                SyncStatus::New => stats.new_files += 1,
                SyncStatus::Synced => stats.synced_files += 1,
                SyncStatus::Modified => stats.modified_files += 1,
                SyncStatus::Deleted => stats.deleted_files += 1,
                SyncStatus::Syncing => stats.syncing_files += 1,
                SyncStatus::Failed => stats.failed_files += 1,
                SyncStatus::Conflict => stats.conflict_files += 1,
            }
        }
        
        stats.history_entries = self.history.len();
        
        // Calculate success rate from recent history
        let recent: Vec<_> = self.history.iter().rev().take(100).collect();
        if !recent.is_empty() {
            let successes = recent.iter().filter(|e| e.success).count();
            stats.success_rate = (successes as f64 / recent.len() as f64) * 100.0;
        }
        
        stats
    }

    /// Find files that need syncing
    pub fn pending_sync(&self) -> Vec<&FileState> {
        self.states.values()
            .filter(|s| matches!(s.status, SyncStatus::New | SyncStatus::Modified | SyncStatus::Deleted))
            .collect()
    }

    /// Find files that failed syncing and can be retried
    pub fn retryable(&self, max_attempts: u32) -> Vec<&FileState> {
        self.states.values()
            .filter(|s| s.status == SyncStatus::Failed && s.sync_attempts < max_attempts)
            .collect()
    }

    /// Clear all state (for testing or reset)
    pub fn clear(&mut self) {
        self.states.clear();
        self.history.clear();
        self.next_history_id = 1;
        self.dirty = true;
    }
}

/// Statistics about sync state
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncStats {
    pub total_files: usize,
    pub total_bytes: u64,
    pub new_files: usize,
    pub synced_files: usize,
    pub modified_files: usize,
    pub deleted_files: usize,
    pub syncing_files: usize,
    pub failed_files: usize,
    pub conflict_files: usize,
    pub history_entries: usize,
    pub success_rate: f64,
}

/// Serializable state for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SavedState {
    states: HashMap<PathBuf, FileState>,
    history: Vec<SyncHistoryEntry>,
    next_history_id: i64,
}

/// Errors that can occur in sync state management
#[derive(Debug, thiserror::Error)]
pub enum SyncStateError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config() -> SyncStateConfig {
        let temp_dir = TempDir::new().unwrap();
        SyncStateConfig {
            db_path: temp_dir.path().join("test_sync.db"),
            ..Default::default()
        }
    }

    #[test]
    fn test_manager_creation() {
        let config = test_config();
        let manager = SyncStateManager::new(config).unwrap();
        assert_eq!(manager.states.len(), 0);
    }

    #[test]
    fn test_upsert_state() {
        let config = test_config();
        let mut manager = SyncStateManager::new(config).unwrap();
        
        let state = FileState {
            path: PathBuf::from("/test/file.txt"),
            size: 1024,
            local_mtime: Utc::now(),
            remote_mtime: None,
            content_hash: None,
            last_synced: None,
            status: SyncStatus::New,
            sync_attempts: 0,
            last_error: None,
        };
        
        manager.upsert_state(state.clone());
        
        let retrieved = manager.get_state(&PathBuf::from("/test/file.txt"));
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().size, 1024);
    }

    #[test]
    fn test_mark_operations() {
        let config = test_config();
        let mut manager = SyncStateManager::new(config).unwrap();
        
        let path = PathBuf::from("/test/file.txt");
        manager.mark_modified(&path, 2048, Utc::now());
        
        let state = manager.get_state(&path).unwrap();
        assert_eq!(state.status, SyncStatus::New); // First time, so New
        
        manager.mark_syncing(&path);
        assert_eq!(manager.get_state(&path).unwrap().status, SyncStatus::Syncing);
        
        manager.mark_synced(&path, Some("abc123".to_string()));
        assert_eq!(manager.get_state(&path).unwrap().status, SyncStatus::Synced);
        
        manager.mark_modified(&path, 4096, Utc::now());
        assert_eq!(manager.get_state(&path).unwrap().status, SyncStatus::Modified);
    }

    #[test]
    fn test_pending_sync() {
        let config = test_config();
        let mut manager = SyncStateManager::new(config).unwrap();
        
        manager.mark_modified(&PathBuf::from("/new.txt"), 100, Utc::now());
        manager.mark_modified(&PathBuf::from("/modified.txt"), 200, Utc::now());
        manager.mark_syncing(&PathBuf::from("/modified.txt"));
        manager.mark_synced(&PathBuf::from("/modified.txt"), None);
        manager.mark_modified(&PathBuf::from("/modified.txt"), 300, Utc::now());
        
        let pending = manager.pending_sync();
        assert_eq!(pending.len(), 2); // new.txt (New) and modified.txt (Modified)
    }

    #[test]
    fn test_history() {
        let config = test_config();
        let mut manager = SyncStateManager::new(config).unwrap();
        
        manager.record_sync(
            &PathBuf::from("/test.txt"),
            SyncAction::Upload,
            true,
            1024,
            100,
            None,
        );
        
        manager.record_sync(
            &PathBuf::from("/test.txt"),
            SyncAction::Upload,
            false,
            0,
            50,
            Some("Network error".to_string()),
        );
        
        let history = manager.recent_history(10);
        assert_eq!(history.len(), 2);
        
        let file_history = manager.file_history(&PathBuf::from("/test.txt"));
        assert_eq!(file_history.len(), 2);
    }

    #[test]
    fn test_stats() {
        let config = test_config();
        let mut manager = SyncStateManager::new(config).unwrap();
        
        manager.mark_modified(&PathBuf::from("/a.txt"), 100, Utc::now());
        manager.mark_modified(&PathBuf::from("/b.txt"), 200, Utc::now());
        manager.mark_syncing(&PathBuf::from("/b.txt"));
        manager.mark_synced(&PathBuf::from("/b.txt"), None);
        
        let stats = manager.stats();
        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.new_files, 1);
        assert_eq!(stats.synced_files, 1);
    }

    #[test]
    fn test_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let config = SyncStateConfig {
            db_path: temp_dir.path().join("persist_test.db"),
            ..Default::default()
        };
        
        // Create and populate manager
        {
            let mut manager = SyncStateManager::new(config.clone()).unwrap();
            manager.mark_modified(&PathBuf::from("/persist.txt"), 999, Utc::now());
            manager.save().unwrap();
        }
        
        // Load in new manager
        {
            let manager = SyncStateManager::new(config).unwrap();
            let state = manager.get_state(&PathBuf::from("/persist.txt"));
            assert!(state.is_some());
            assert_eq!(state.unwrap().size, 999);
        }
    }
}
