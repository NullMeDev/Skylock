//! File Watcher Daemon
//!
//! Provides real-time file system monitoring for continuous backup.
//! Uses the `notify` crate with 500ms debounce to batch rapid changes.
//!
//! # Requirements
//! - Root/sudo access recommended for watching system directories
//! - Uses inotify on Linux, FSEvents on macOS, ReadDirectoryChangesW on Windows

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock, broadcast};
use tracing::{info, warn, error, debug};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

/// Default debounce delay in milliseconds
pub const DEFAULT_DEBOUNCE_MS: u64 = 500;

/// Maximum number of pending events before forcing a flush
pub const MAX_PENDING_EVENTS: usize = 1000;

/// Event types for file system changes
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileEventKind {
    /// File or directory was created
    Create,
    /// File was modified (content changed)
    Modify,
    /// File or directory was deleted
    Delete,
    /// File or directory was renamed (old path, new path tracked separately)
    Rename,
    /// File metadata changed (permissions, timestamps)
    Metadata,
    /// Unknown or other event type
    Other,
}

/// A file system change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEvent {
    /// The path that changed
    pub path: PathBuf,
    /// Type of change
    pub kind: FileEventKind,
    /// When the event was detected
    pub timestamp: DateTime<Utc>,
    /// For rename events, the new path (if known)
    pub new_path: Option<PathBuf>,
    /// Whether the path is a directory
    pub is_dir: bool,
}

impl FileEvent {
    pub fn new(path: PathBuf, kind: FileEventKind, is_dir: bool) -> Self {
        Self {
            path,
            kind,
            timestamp: Utc::now(),
            new_path: None,
            is_dir,
        }
    }

    pub fn with_new_path(mut self, new_path: PathBuf) -> Self {
        self.new_path = Some(new_path);
        self
    }
}

/// Batch of debounced events ready for processing
#[derive(Debug, Clone, Default)]
pub struct EventBatch {
    /// Events in this batch
    pub events: Vec<FileEvent>,
    /// Unique paths affected
    pub affected_paths: HashSet<PathBuf>,
    /// When the batch was created
    pub created_at: DateTime<Utc>,
    /// When the batch was finalized
    pub finalized_at: DateTime<Utc>,
}

impl EventBatch {
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            events: Vec::new(),
            affected_paths: HashSet::new(),
            created_at: now,
            finalized_at: now,
        }
    }

    pub fn add_event(&mut self, event: FileEvent) {
        self.affected_paths.insert(event.path.clone());
        if let Some(ref new_path) = event.new_path {
            self.affected_paths.insert(new_path.clone());
        }
        self.events.push(event);
    }

    pub fn finalize(&mut self) {
        self.finalized_at = Utc::now();
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }
}

/// Configuration for the file watcher
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherConfig {
    /// Paths to watch
    pub watch_paths: Vec<PathBuf>,
    /// Debounce delay in milliseconds
    pub debounce_ms: u64,
    /// Whether to watch recursively
    pub recursive: bool,
    /// File patterns to ignore (glob patterns)
    pub ignore_patterns: Vec<String>,
    /// Maximum events to buffer before forcing a flush
    pub max_buffer_size: usize,
    /// Whether root access is available
    pub has_root_access: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            watch_paths: Vec::new(),
            debounce_ms: DEFAULT_DEBOUNCE_MS,
            recursive: true,
            ignore_patterns: vec![
                "*.swp".to_string(),
                "*.tmp".to_string(),
                "*~".to_string(),
                ".git/*".to_string(),
                ".svn/*".to_string(),
                "node_modules/*".to_string(),
                "__pycache__/*".to_string(),
                "*.pyc".to_string(),
                ".DS_Store".to_string(),
                "Thumbs.db".to_string(),
            ],
            max_buffer_size: MAX_PENDING_EVENTS,
            has_root_access: false,
        }
    }
}

impl WatcherConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.watch_paths = paths;
        self
    }

    pub fn with_debounce(mut self, ms: u64) -> Self {
        self.debounce_ms = ms;
        self
    }

    pub fn with_recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    pub fn with_ignore_patterns(mut self, patterns: Vec<String>) -> Self {
        self.ignore_patterns = patterns;
        self
    }

    /// Check if root/elevated access is available
    pub fn check_root_access(&mut self) {
        #[cfg(unix)]
        {
            self.has_root_access = unsafe { libc::geteuid() } == 0;
        }
        #[cfg(windows)]
        {
            // On Windows, check if running as admin
            self.has_root_access = is_elevated_windows();
        }
    }
}

#[cfg(windows)]
fn is_elevated_windows() -> bool {
    // Simplified check - in practice, use Windows API
    std::env::var("USERNAME").map(|u| u == "Administrator").unwrap_or(false)
}

/// The file watcher daemon
pub struct FileWatcher {
    config: WatcherConfig,
    /// Pending events being debounced
    pending_events: Arc<RwLock<HashMap<PathBuf, FileEvent>>>,
    /// Last activity time for debouncing
    last_activity: Arc<RwLock<Instant>>,
    /// Channel for sending batched events
    event_tx: mpsc::Sender<EventBatch>,
    /// Shutdown signal
    shutdown_tx: broadcast::Sender<()>,
    /// Whether the watcher is running
    is_running: Arc<RwLock<bool>>,
}

impl FileWatcher {
    /// Create a new file watcher with the given configuration
    pub fn new(config: WatcherConfig) -> (Self, mpsc::Receiver<EventBatch>) {
        let (event_tx, event_rx) = mpsc::channel(100);
        let (shutdown_tx, _) = broadcast::channel(1);

        let watcher = Self {
            config,
            pending_events: Arc::new(RwLock::new(HashMap::new())),
            last_activity: Arc::new(RwLock::new(Instant::now())),
            event_tx,
            shutdown_tx,
            is_running: Arc::new(RwLock::new(false)),
        };

        (watcher, event_rx)
    }

    /// Check if the watcher is currently running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Get a shutdown signal receiver
    pub fn subscribe_shutdown(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Start watching files
    /// 
    /// This spawns background tasks for:
    /// 1. The actual file system watcher
    /// 2. A debounce timer that flushes events
    pub async fn start(&self) -> Result<(), WatcherError> {
        // Check root access warning
        if !self.config.has_root_access {
            warn!("File watcher running without root access.");
            warn!("Some system directories may not be watchable.");
            warn!("Run with sudo for full access: sudo skylock watch");
        }

        // Validate watch paths
        for path in &self.config.watch_paths {
            if !path.exists() {
                return Err(WatcherError::PathNotFound(path.clone()));
            }
            if !path.is_dir() {
                return Err(WatcherError::NotADirectory(path.clone()));
            }
        }

        *self.is_running.write().await = true;
        info!("File watcher started. Watching {} paths with {}ms debounce",
              self.config.watch_paths.len(), self.config.debounce_ms);

        // Start the debounce timer task
        let pending = self.pending_events.clone();
        let last_activity = self.last_activity.clone();
        let event_tx = self.event_tx.clone();
        let debounce_ms = self.config.debounce_ms;
        let max_buffer = self.config.max_buffer_size;
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        let is_running = self.is_running.clone();

        tokio::spawn(async move {
            let debounce_duration = Duration::from_millis(debounce_ms);
            
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("Debounce timer received shutdown signal");
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        let last = *last_activity.read().await;
                        let pending_count = pending.read().await.len();
                        
                        // Flush if debounce time has passed OR buffer is full
                        let should_flush = (last.elapsed() >= debounce_duration && pending_count > 0)
                            || pending_count >= max_buffer;
                        
                        if should_flush {
                            let mut batch = EventBatch::new();
                            {
                                let mut events = pending.write().await;
                                for (_, event) in events.drain() {
                                    batch.add_event(event);
                                }
                            }
                            
                            if !batch.is_empty() {
                                batch.finalize();
                                debug!("Flushing {} events", batch.len());
                                if event_tx.send(batch).await.is_err() {
                                    error!("Failed to send event batch - receiver dropped");
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            
            *is_running.write().await = false;
            info!("Debounce timer stopped");
        });

        Ok(())
    }

    /// Stop the file watcher
    pub async fn stop(&self) {
        info!("Stopping file watcher...");
        let _ = self.shutdown_tx.send(());
        *self.is_running.write().await = false;
    }

    /// Add a raw event to the pending queue
    /// Events for the same path will be merged/deduplicated
    pub async fn add_event(&self, event: FileEvent) {
        // Check if path matches ignore patterns
        if self.should_ignore(&event.path) {
            debug!("Ignoring event for path: {:?}", event.path);
            return;
        }

        let mut pending = self.pending_events.write().await;
        let path = event.path.clone();

        // Merge events for the same path
        if let Some(existing) = pending.get_mut(&path) {
            // Prioritize certain event types
            match (&existing.kind, &event.kind) {
                // Delete always wins
                (_, FileEventKind::Delete) => {
                    *existing = event;
                }
                // Create + Modify = Create
                (FileEventKind::Create, FileEventKind::Modify) => {
                    // Keep as Create
                }
                // Modify + Modify = Modify (update timestamp)
                (FileEventKind::Modify, FileEventKind::Modify) => {
                    existing.timestamp = event.timestamp;
                }
                // Other cases: use newer event
                _ => {
                    *existing = event;
                }
            }
        } else {
            pending.insert(path, event);
        }

        // Update last activity time
        *self.last_activity.write().await = Instant::now();
    }

    /// Check if a path should be ignored based on configured patterns
    fn should_ignore(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        
        for pattern in &self.config.ignore_patterns {
            if Self::matches_glob(pattern, &path_str) {
                return true;
            }
        }
        
        false
    }

    /// Simple glob pattern matching
    fn matches_glob(pattern: &str, path: &str) -> bool {
        // Handle simple patterns: *, ?, and **
        let pattern_parts: Vec<&str> = pattern.split('/').collect();
        let path_parts: Vec<&str> = path.split(std::path::MAIN_SEPARATOR).collect();
        
        // Check if any path component matches
        for part in &path_parts {
            if let Some(file_pattern) = pattern_parts.last() {
                if Self::matches_simple_glob(file_pattern, part) {
                    return true;
                }
            }
        }
        
        // Check full path for ** patterns
        if pattern.contains("**") {
            let simple_pattern = pattern.replace("**", "*");
            return Self::matches_simple_glob(&simple_pattern, path);
        }
        
        false
    }

    /// Match simple glob with * and ?
    fn matches_simple_glob(pattern: &str, text: &str) -> bool {
        let mut pattern_chars = pattern.chars().peekable();
        let mut text_chars = text.chars().peekable();
        
        loop {
            match (pattern_chars.peek(), text_chars.peek()) {
                (Some('*'), _) => {
                    pattern_chars.next();
                    if pattern_chars.peek().is_none() {
                        return true; // * at end matches everything
                    }
                    // Try matching * with zero or more characters
                    while text_chars.peek().is_some() {
                        let remaining_pattern: String = pattern_chars.clone().collect();
                        let remaining_text: String = text_chars.clone().collect();
                        if Self::matches_simple_glob(&remaining_pattern, &remaining_text) {
                            return true;
                        }
                        text_chars.next();
                    }
                    return false;
                }
                (Some('?'), Some(_)) => {
                    pattern_chars.next();
                    text_chars.next();
                }
                (Some(p), Some(t)) if *p == *t => {
                    pattern_chars.next();
                    text_chars.next();
                }
                (None, None) => return true,
                _ => return false,
            }
        }
    }
}

/// Errors that can occur during file watching
#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("Path not found: {0}")]
    PathNotFound(PathBuf),
    
    #[error("Not a directory: {0}")]
    NotADirectory(PathBuf),
    
    #[error("Permission denied: {0}")]
    PermissionDenied(PathBuf),
    
    #[error("Watcher error: {0}")]
    WatcherError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Statistics about the file watcher
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WatcherStats {
    pub events_received: u64,
    pub events_processed: u64,
    pub batches_sent: u64,
    pub paths_watched: usize,
    pub started_at: Option<DateTime<Utc>>,
    pub last_event_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_matching() {
        assert!(FileWatcher::matches_simple_glob("*.txt", "file.txt"));
        assert!(FileWatcher::matches_simple_glob("*.txt", "another.txt"));
        assert!(!FileWatcher::matches_simple_glob("*.txt", "file.rs"));
        assert!(FileWatcher::matches_simple_glob("test?", "test1"));
        assert!(FileWatcher::matches_simple_glob("test?", "testa"));
        assert!(!FileWatcher::matches_simple_glob("test?", "test12"));
        assert!(FileWatcher::matches_simple_glob("*", "anything"));
    }

    #[test]
    fn test_event_batch() {
        let mut batch = EventBatch::new();
        assert!(batch.is_empty());
        
        batch.add_event(FileEvent::new(
            PathBuf::from("/test/file.txt"),
            FileEventKind::Create,
            false,
        ));
        
        assert_eq!(batch.len(), 1);
        assert!(batch.affected_paths.contains(&PathBuf::from("/test/file.txt")));
    }

    #[test]
    fn test_watcher_config_defaults() {
        let config = WatcherConfig::default();
        assert_eq!(config.debounce_ms, DEFAULT_DEBOUNCE_MS);
        assert!(config.recursive);
        assert!(!config.ignore_patterns.is_empty());
    }

    #[tokio::test]
    async fn test_watcher_creation() {
        let config = WatcherConfig::default().with_debounce(100);
        let (watcher, _rx) = FileWatcher::new(config);
        assert!(!watcher.is_running().await);
    }

    #[tokio::test]
    async fn test_event_debouncing() {
        let config = WatcherConfig::default().with_debounce(50);
        let (watcher, _rx) = FileWatcher::new(config);
        
        // Add multiple events for the same path
        let path = PathBuf::from("/test/file.txt");
        
        watcher.add_event(FileEvent::new(path.clone(), FileEventKind::Create, false)).await;
        watcher.add_event(FileEvent::new(path.clone(), FileEventKind::Modify, false)).await;
        
        // Should be merged to single event
        let pending = watcher.pending_events.read().await;
        assert_eq!(pending.len(), 1);
        // Create + Modify = Create
        assert_eq!(pending.get(&path).unwrap().kind, FileEventKind::Create);
    }

    #[test]
    fn test_ignore_patterns() {
        let config = WatcherConfig::default();
        let (watcher, _rx) = FileWatcher::new(config);
        
        assert!(watcher.should_ignore(Path::new("/path/to/file.swp")));
        assert!(watcher.should_ignore(Path::new("/path/to/.git/objects/abc")));
        assert!(watcher.should_ignore(Path::new("/path/to/node_modules/package/file.js")));
        assert!(!watcher.should_ignore(Path::new("/path/to/important.txt")));
    }
}
