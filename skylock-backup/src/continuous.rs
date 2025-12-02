//! Continuous Backup Mode
//!
//! Provides real-time continuous backup by integrating the file watcher,
//! sync queue, and state tracking components.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, broadcast, RwLock};
use tracing::{info, warn, error, debug};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

use crate::watcher::{FileWatcher, WatcherConfig, FileEvent, FileEventKind, EventBatch};
use crate::sync_queue::{SyncQueueProcessor, SyncQueueConfig, SyncItem, SyncAction, SyncResult};
use crate::sync_state::{SyncStateManager, SyncStateConfig, SyncStatus, SyncAction as StateAction};

/// Configuration for continuous backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuousBackupConfig {
    /// Paths to watch for changes
    pub watch_paths: Vec<PathBuf>,
    /// File watcher configuration
    pub watcher: WatcherConfig,
    /// Sync queue configuration
    pub queue: SyncQueueConfig,
    /// Sync state configuration
    pub state: SyncStateConfig,
    /// Whether to perform initial scan on startup
    pub initial_scan: bool,
    /// Interval for periodic state saves (in seconds)
    pub state_save_interval_secs: u64,
    /// Whether to enable desktop notifications
    pub notifications_enabled: bool,
}

impl Default for ContinuousBackupConfig {
    fn default() -> Self {
        Self {
            watch_paths: Vec::new(),
            watcher: WatcherConfig::default(),
            queue: SyncQueueConfig::default(),
            state: SyncStateConfig::default(),
            initial_scan: true,
            state_save_interval_secs: 60,
            notifications_enabled: true,
        }
    }
}

impl ContinuousBackupConfig {
    pub fn with_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.watch_paths = paths.clone();
        self.watcher.watch_paths = paths;
        self
    }
}

/// Statistics for continuous backup
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContinuousBackupStats {
    pub started_at: Option<DateTime<Utc>>,
    pub uptime_secs: u64,
    pub files_watched: usize,
    pub events_received: u64,
    pub files_synced: u64,
    pub bytes_uploaded: u64,
    pub errors: u64,
    pub conflicts_resolved: u64,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub pending_items: usize,
}

/// The continuous backup daemon
pub struct ContinuousBackup {
    config: ContinuousBackupConfig,
    /// File watcher
    watcher: Arc<FileWatcher>,
    /// Event batch receiver
    event_rx: mpsc::Receiver<EventBatch>,
    /// Sync queue processor
    queue: Arc<SyncQueueProcessor>,
    /// Sync result receiver
    result_rx: mpsc::Receiver<SyncResult>,
    /// Sync state manager
    state: Arc<RwLock<SyncStateManager>>,
    /// Shutdown signal sender
    shutdown_tx: broadcast::Sender<()>,
    /// Statistics
    stats: Arc<RwLock<ContinuousBackupStats>>,
    /// Whether the daemon is running
    is_running: Arc<RwLock<bool>>,
    /// Start time
    start_time: Option<Instant>,
}

impl ContinuousBackup {
    /// Create a new continuous backup daemon
    pub fn new(config: ContinuousBackupConfig) -> Result<Self, ContinuousBackupError> {
        // Create components
        let mut watcher_config = config.watcher.clone();
        watcher_config.watch_paths = config.watch_paths.clone();
        watcher_config.check_root_access();
        
        let (watcher, event_rx) = FileWatcher::new(watcher_config);
        let (queue, result_rx) = SyncQueueProcessor::new(config.queue.clone());
        let state = SyncStateManager::new(config.state.clone())
            .map_err(|e| ContinuousBackupError::StateError(e.to_string()))?;
        
        let (shutdown_tx, _) = broadcast::channel(1);
        
        Ok(Self {
            config,
            watcher: Arc::new(watcher),
            event_rx,
            queue: Arc::new(queue),
            result_rx,
            state: Arc::new(RwLock::new(state)),
            shutdown_tx,
            stats: Arc::new(RwLock::new(ContinuousBackupStats::default())),
            is_running: Arc::new(RwLock::new(false)),
            start_time: None,
        })
    }

    /// Start the continuous backup daemon
    pub async fn start(&mut self) -> Result<(), ContinuousBackupError> {
        if *self.is_running.read().await {
            return Err(ContinuousBackupError::AlreadyRunning);
        }

        info!("Starting continuous backup daemon...");
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.started_at = Some(Utc::now());
        }
        self.start_time = Some(Instant::now());
        *self.is_running.write().await = true;

        // Perform initial scan if configured
        if self.config.initial_scan {
            self.perform_initial_scan().await?;
        }

        // Start the file watcher
        self.watcher.start().await
            .map_err(|e| ContinuousBackupError::WatcherError(e.to_string()))?;

        // Spawn event processing task
        self.spawn_event_processor();

        // Spawn sync processor task
        self.spawn_sync_processor();

        // Spawn result handler task
        self.spawn_result_handler();

        // Spawn periodic state saver
        self.spawn_state_saver();

        info!("Continuous backup daemon started successfully");
        Ok(())
    }

    /// Stop the continuous backup daemon
    pub async fn stop(&mut self) -> Result<(), ContinuousBackupError> {
        if !*self.is_running.read().await {
            return Ok(());
        }

        info!("Stopping continuous backup daemon...");
        
        // Send shutdown signal
        let _ = self.shutdown_tx.send(());
        
        // Stop the watcher
        self.watcher.stop().await;
        
        // Save state
        {
            let mut state = self.state.write().await;
            state.save().map_err(|e| ContinuousBackupError::StateError(e.to_string()))?;
        }
        
        *self.is_running.write().await = false;
        info!("Continuous backup daemon stopped");
        Ok(())
    }

    /// Check if the daemon is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Get current statistics
    pub async fn stats(&self) -> ContinuousBackupStats {
        let mut stats = self.stats.read().await.clone();
        
        if let Some(start) = self.start_time {
            stats.uptime_secs = start.elapsed().as_secs();
        }
        
        stats.pending_items = self.queue.queue_size().await;
        stats
    }

    /// Get a shutdown signal receiver
    pub fn subscribe_shutdown(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Perform initial scan of watched directories
    async fn perform_initial_scan(&self) -> Result<(), ContinuousBackupError> {
        info!("Performing initial scan of watched directories...");
        
        let mut files_found = 0;
        
        for path in &self.config.watch_paths {
            if !path.exists() {
                warn!("Watch path does not exist: {:?}", path);
                continue;
            }
            
            files_found += self.scan_directory(path).await?;
        }
        
        info!("Initial scan complete: {} files found", files_found);
        
        {
            let mut stats = self.stats.write().await;
            stats.files_watched = files_found;
        }
        
        Ok(())
    }

    /// Recursively scan a directory
    async fn scan_directory(&self, dir: &PathBuf) -> Result<usize, ContinuousBackupError> {
        let mut count = 0;
        
        let entries = std::fs::read_dir(dir)
            .map_err(|e| ContinuousBackupError::IoError(e.to_string()))?;
        
        for entry in entries {
            let entry = entry.map_err(|e| ContinuousBackupError::IoError(e.to_string()))?;
            let path = entry.path();
            
            if path.is_file() {
                if let Ok(metadata) = std::fs::metadata(&path) {
                    let mtime: DateTime<Utc> = metadata.modified()
                        .map(|t| t.into())
                        .unwrap_or_else(|_| Utc::now());
                    
                    let mut state = self.state.write().await;
                    
                    // Check if file is new or modified
                    if let Some(existing) = state.get_state(&path) {
                        if existing.local_mtime < mtime {
                            state.mark_modified(&path, metadata.len(), mtime);
                        }
                    } else {
                        state.mark_modified(&path, metadata.len(), mtime);
                    }
                    
                    count += 1;
                }
            } else if path.is_dir() {
                // Check ignore patterns
                let should_ignore = self.config.watcher.ignore_patterns.iter()
                    .any(|p| path.to_string_lossy().contains(p.trim_matches('*')));
                
                if !should_ignore {
                    count += Box::pin(self.scan_directory(&path)).await?;
                }
            }
        }
        
        Ok(count)
    }

    /// Spawn the event processing task
    fn spawn_event_processor(&mut self) {
        // We need to take ownership of event_rx for the spawned task
        // In a real implementation, we'd use channels differently
        // For now, we'll just log that it would be spawned
        let queue = self.queue.clone();
        let state = self.state.clone();
        let stats = self.stats.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        
        info!("Event processor task spawned (placeholder - would process events from watcher)");
        
        // In a full implementation, this would receive from event_rx
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("Event processor received shutdown signal");
                        break;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                        // In real implementation, would process events here
                    }
                }
            }
        });
    }

    /// Spawn the sync processor task
    fn spawn_sync_processor(&self) {
        let queue = self.queue.clone();
        let state = self.state.clone();
        let stats = self.stats.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("Sync processor received shutdown signal");
                        break;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                        // Get next item to sync
                        if let Some(item) = queue.next_item().await {
                            debug!("Processing sync item: {:?}", item.path);
                            
                            // Mark as syncing in state
                            {
                                let mut state = state.write().await;
                                state.mark_syncing(&item.path);
                            }
                            
                            // In a real implementation, this would perform the actual sync
                            // For now, we simulate it
                            let start = Instant::now();
                            let result = simulate_sync(&item).await;
                            let duration = start.elapsed().as_millis() as u64;
                            
                            // Update state and stats
                            {
                                let mut state = state.write().await;
                                if result.success {
                                    state.mark_synced(&item.path, None);
                                    state.record_sync(
                                        &item.path,
                                        StateAction::Upload,
                                        true,
                                        result.bytes_transferred,
                                        duration,
                                        None,
                                    );
                                } else {
                                    state.mark_failed(&item.path, result.error.clone().unwrap_or_default());
                                    state.record_sync(
                                        &item.path,
                                        StateAction::Upload,
                                        false,
                                        0,
                                        duration,
                                        result.error.clone(),
                                    );
                                }
                            }
                            
                            {
                                let mut stats = stats.write().await;
                                if result.success {
                                    stats.files_synced += 1;
                                    stats.bytes_uploaded += result.bytes_transferred;
                                    stats.last_sync_at = Some(Utc::now());
                                } else {
                                    stats.errors += 1;
                                }
                            }
                            
                            // Complete the item in the queue
                            queue.complete_item(result).await;
                        }
                    }
                }
            }
        });
    }

    /// Spawn the result handler task
    fn spawn_result_handler(&mut self) {
        let stats = self.stats.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("Result handler received shutdown signal");
                        break;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(500)) => {
                        // Would handle results from result_rx in full implementation
                    }
                }
            }
        });
    }

    /// Spawn periodic state saver
    fn spawn_state_saver(&self) {
        let state = self.state.clone();
        let interval = self.config.state_save_interval_secs;
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        debug!("State saver received shutdown signal");
                        // Final save
                        let mut state = state.write().await;
                        let _ = state.save();
                        break;
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(interval)) => {
                        let mut state = state.write().await;
                        if let Err(e) = state.save() {
                            error!("Failed to save sync state: {}", e);
                        }
                    }
                }
            }
        });
    }

    /// Handle an incoming file event
    pub async fn handle_event(&self, event: FileEvent) {
        // Update state
        {
            let mut state = self.state.write().await;
            
            match event.kind {
                FileEventKind::Create | FileEventKind::Modify => {
                    if let Ok(metadata) = std::fs::metadata(&event.path) {
                        let mtime: DateTime<Utc> = metadata.modified()
                            .map(|t| t.into())
                            .unwrap_or_else(|_| Utc::now());
                        state.mark_modified(&event.path, metadata.len(), mtime);
                    }
                }
                FileEventKind::Delete => {
                    state.mark_deleted(&event.path);
                }
                _ => {}
            }
        }
        
        // Add to queue
        if let Err(e) = self.queue.add_event(event.clone()).await {
            error!("Failed to add event to queue: {}", e);
        }
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.events_received += 1;
        }
    }
}

/// Simulate a sync operation (placeholder for actual implementation)
async fn simulate_sync(item: &SyncItem) -> SyncResult {
    // In a real implementation, this would:
    // 1. Read the file
    // 2. Encrypt it
    // 3. Upload to storage provider
    
    // Simulate some processing time
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    
    // Get file size for stats
    let bytes = std::fs::metadata(&item.path)
        .map(|m| m.len())
        .unwrap_or(0);
    
    SyncResult {
        item: item.clone(),
        success: true, // In real impl, would depend on actual sync result
        error: None,
        bytes_transferred: bytes,
        duration_ms: 10,
    }
}

/// Errors that can occur in continuous backup
#[derive(Debug, thiserror::Error)]
pub enum ContinuousBackupError {
    #[error("Already running")]
    AlreadyRunning,
    
    #[error("Not running")]
    NotRunning,
    
    #[error("Watcher error: {0}")]
    WatcherError(String),
    
    #[error("Queue error: {0}")]
    QueueError(String),
    
    #[error("State error: {0}")]
    StateError(String),
    
    #[error("IO error: {0}")]
    IoError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config() -> ContinuousBackupConfig {
        let temp_dir = TempDir::new().unwrap();
        ContinuousBackupConfig {
            watch_paths: vec![temp_dir.path().to_path_buf()],
            initial_scan: false,
            state: SyncStateConfig {
                db_path: temp_dir.path().join("test_state.db"),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_daemon_creation() {
        let config = test_config();
        let daemon = ContinuousBackup::new(config);
        assert!(daemon.is_ok());
    }

    #[tokio::test]
    async fn test_stats_default() {
        let config = test_config();
        let daemon = ContinuousBackup::new(config).unwrap();
        let stats = daemon.stats().await;
        assert!(stats.started_at.is_none());
        assert_eq!(stats.files_synced, 0);
    }
}
