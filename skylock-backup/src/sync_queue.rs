//! Sync Queue Processor
//!
//! Processes file change events from the watcher, handles conflicts,
//! and queues files for backup. Uses "newest version wins" conflict resolution.

use std::collections::{HashMap, VecDeque, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Semaphore};
use tracing::{info, warn, error, debug};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

use crate::watcher::{EventBatch, FileEvent, FileEventKind};

/// Default maximum queue size
pub const DEFAULT_MAX_QUEUE_SIZE: usize = 10000;

/// Default number of concurrent uploads
pub const DEFAULT_CONCURRENT_UPLOADS: usize = 4;

/// A queued item ready for sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncItem {
    /// Path to the file
    pub path: PathBuf,
    /// Action to take
    pub action: SyncAction,
    /// When the item was queued
    pub queued_at: DateTime<Utc>,
    /// File modification time (for conflict resolution)
    pub mtime: Option<DateTime<Utc>>,
    /// Number of retry attempts
    pub retry_count: u32,
    /// Priority (lower = higher priority)
    pub priority: u32,
}

impl SyncItem {
    pub fn new(path: PathBuf, action: SyncAction) -> Self {
        Self {
            path,
            action,
            queued_at: Utc::now(),
            mtime: None,
            retry_count: 0,
            priority: 100,
        }
    }

    pub fn with_mtime(mut self, mtime: DateTime<Utc>) -> Self {
        self.mtime = Some(mtime);
        self
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }
}

/// Action to take for a sync item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncAction {
    /// Upload new or modified file
    Upload,
    /// Delete file from remote
    Delete,
    /// Rename/move file on remote
    Rename,
    /// Skip this file (e.g., conflict resolved to remote version)
    Skip,
}

/// Result of a conflict resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictResolution {
    /// The path that had a conflict
    pub path: PathBuf,
    /// How it was resolved
    pub resolution: ConflictResolutionType,
    /// Local modification time
    pub local_mtime: Option<DateTime<Utc>>,
    /// Remote modification time
    pub remote_mtime: Option<DateTime<Utc>>,
    /// When the conflict was resolved
    pub resolved_at: DateTime<Utc>,
}

/// Types of conflict resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictResolutionType {
    /// Local version is newer, upload it
    LocalWins,
    /// Remote version is newer, skip local
    RemoteWins,
    /// Both versions are kept (renamed)
    BothKept,
    /// User chose local version
    UserChoseLocal,
    /// User chose remote version
    UserChoseRemote,
}

/// Configuration for the sync queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncQueueConfig {
    /// Maximum items in the queue
    pub max_queue_size: usize,
    /// Number of concurrent uploads
    pub concurrent_uploads: usize,
    /// Maximum retry attempts before giving up
    pub max_retries: u32,
    /// Delay between retries (exponential backoff base in ms)
    pub retry_delay_ms: u64,
    /// Whether to warn on conflicts
    pub warn_on_conflicts: bool,
    /// Whether to log conflict resolutions
    pub log_conflicts: bool,
}

impl Default for SyncQueueConfig {
    fn default() -> Self {
        Self {
            max_queue_size: DEFAULT_MAX_QUEUE_SIZE,
            concurrent_uploads: DEFAULT_CONCURRENT_UPLOADS,
            max_retries: 3,
            retry_delay_ms: 1000,
            warn_on_conflicts: true,
            log_conflicts: true,
        }
    }
}

/// Statistics about the sync queue
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncQueueStats {
    pub items_queued: u64,
    pub items_processed: u64,
    pub items_failed: u64,
    pub conflicts_resolved: u64,
    pub bytes_uploaded: u64,
    pub bytes_downloaded: u64,
    pub current_queue_size: usize,
    pub last_sync_at: Option<DateTime<Utc>>,
}

/// The sync queue processor
pub struct SyncQueueProcessor {
    config: SyncQueueConfig,
    /// Pending items to sync (priority queue)
    queue: Arc<RwLock<VecDeque<SyncItem>>>,
    /// Items currently being processed
    in_progress: Arc<RwLock<HashSet<PathBuf>>>,
    /// Recent conflict resolutions
    conflicts: Arc<RwLock<Vec<ConflictResolution>>>,
    /// Statistics
    stats: Arc<RwLock<SyncQueueStats>>,
    /// Semaphore for concurrent uploads
    upload_semaphore: Arc<Semaphore>,
    /// Channel for completed items
    completed_tx: mpsc::Sender<SyncResult>,
    /// Whether the processor is running
    is_running: Arc<RwLock<bool>>,
}

/// Result of processing a sync item
#[derive(Debug, Clone)]
pub struct SyncResult {
    pub item: SyncItem,
    pub success: bool,
    pub error: Option<String>,
    pub bytes_transferred: u64,
    pub duration_ms: u64,
}

impl SyncQueueProcessor {
    /// Create a new sync queue processor
    pub fn new(config: SyncQueueConfig) -> (Self, mpsc::Receiver<SyncResult>) {
        let (completed_tx, completed_rx) = mpsc::channel(100);
        let upload_semaphore = Arc::new(Semaphore::new(config.concurrent_uploads));

        let processor = Self {
            config,
            queue: Arc::new(RwLock::new(VecDeque::new())),
            in_progress: Arc::new(RwLock::new(HashSet::new())),
            conflicts: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(SyncQueueStats::default())),
            upload_semaphore,
            completed_tx,
            is_running: Arc::new(RwLock::new(false)),
        };

        (processor, completed_rx)
    }

    /// Get current statistics
    pub async fn stats(&self) -> SyncQueueStats {
        let mut stats = self.stats.read().await.clone();
        stats.current_queue_size = self.queue.read().await.len();
        stats
    }

    /// Get recent conflicts
    pub async fn recent_conflicts(&self, limit: usize) -> Vec<ConflictResolution> {
        let conflicts = self.conflicts.read().await;
        conflicts.iter().rev().take(limit).cloned().collect()
    }

    /// Add an event batch to the queue
    pub async fn add_batch(&self, batch: EventBatch) -> Result<usize, SyncQueueError> {
        let mut added = 0;
        
        for event in batch.events {
            if self.add_event(event).await? {
                added += 1;
            }
        }
        
        debug!("Added {} items from batch to sync queue", added);
        Ok(added)
    }

    /// Add a single event to the queue
    pub async fn add_event(&self, event: FileEvent) -> Result<bool, SyncQueueError> {
        let action = match event.kind {
            FileEventKind::Create | FileEventKind::Modify => SyncAction::Upload,
            FileEventKind::Delete => SyncAction::Delete,
            FileEventKind::Rename => SyncAction::Rename,
            FileEventKind::Metadata => return Ok(false), // Skip metadata-only changes
            FileEventKind::Other => return Ok(false),
        };

        let mut item = SyncItem::new(event.path.clone(), action);
        
        // Try to get file mtime for conflict resolution
        if let Ok(metadata) = tokio::fs::metadata(&event.path).await {
            if let Ok(modified) = metadata.modified() {
                item.mtime = Some(modified.into());
            }
        }

        self.add_item(item).await
    }

    /// Add a sync item to the queue
    pub async fn add_item(&self, item: SyncItem) -> Result<bool, SyncQueueError> {
        let mut queue = self.queue.write().await;
        
        // Check queue size limit
        if queue.len() >= self.config.max_queue_size {
            return Err(SyncQueueError::QueueFull);
        }

        // Check if already in queue or in progress
        let in_progress = self.in_progress.read().await;
        if in_progress.contains(&item.path) {
            debug!("Skipping item already in progress: {:?}", item.path);
            return Ok(false);
        }

        // Check for duplicates and merge if found
        if let Some(existing) = queue.iter_mut().find(|i| i.path == item.path) {
            // Newer action takes precedence
            if item.queued_at > existing.queued_at {
                *existing = item;
            }
            return Ok(false);
        }

        // Insert by priority (lower priority value = earlier in queue)
        let pos = queue.iter().position(|i| i.priority > item.priority)
            .unwrap_or(queue.len());
        queue.insert(pos, item);

        // Update stats
        let mut stats = self.stats.write().await;
        stats.items_queued += 1;

        Ok(true)
    }

    /// Get the next item to process
    pub async fn next_item(&self) -> Option<SyncItem> {
        let mut queue = self.queue.write().await;
        let mut in_progress = self.in_progress.write().await;

        // Find first item not already in progress
        if let Some(pos) = queue.iter().position(|i| !in_progress.contains(&i.path)) {
            let item = queue.remove(pos)?;
            in_progress.insert(item.path.clone());
            Some(item)
        } else {
            None
        }
    }

    /// Mark an item as completed
    pub async fn complete_item(&self, result: SyncResult) {
        let mut in_progress = self.in_progress.write().await;
        in_progress.remove(&result.item.path);

        let mut stats = self.stats.write().await;
        stats.items_processed += 1;
        if result.success {
            stats.bytes_uploaded += result.bytes_transferred;
            stats.last_sync_at = Some(Utc::now());
        } else {
            stats.items_failed += 1;
        }

        // Send result to completion channel
        let _ = self.completed_tx.send(result).await;
    }

    /// Resolve a conflict between local and remote versions
    /// Uses "newest version wins" strategy
    pub async fn resolve_conflict(
        &self,
        path: &PathBuf,
        local_mtime: Option<DateTime<Utc>>,
        remote_mtime: Option<DateTime<Utc>>,
    ) -> ConflictResolutionType {
        let resolution = match (local_mtime, remote_mtime) {
            (Some(local), Some(remote)) => {
                if local > remote {
                    ConflictResolutionType::LocalWins
                } else if remote > local {
                    ConflictResolutionType::RemoteWins
                } else {
                    // Same time, default to local
                    ConflictResolutionType::LocalWins
                }
            }
            (Some(_), None) => ConflictResolutionType::LocalWins,
            (None, Some(_)) => ConflictResolutionType::RemoteWins,
            (None, None) => ConflictResolutionType::LocalWins, // Default to local if no info
        };

        // Log the conflict
        if self.config.warn_on_conflicts {
            warn!(
                "Conflict detected for {:?}: {:?} (local: {:?}, remote: {:?})",
                path, resolution, local_mtime, remote_mtime
            );
        }

        // Record the conflict
        if self.config.log_conflicts {
            let record = ConflictResolution {
                path: path.clone(),
                resolution,
                local_mtime,
                remote_mtime,
                resolved_at: Utc::now(),
            };

            let mut conflicts = self.conflicts.write().await;
            conflicts.push(record);

            // Keep only recent conflicts (last 1000)
            if conflicts.len() > 1000 {
                conflicts.remove(0);
            }

            let mut stats = self.stats.write().await;
            stats.conflicts_resolved += 1;
        }

        resolution
    }

    /// Retry a failed item
    pub async fn retry_item(&self, mut item: SyncItem) -> Result<bool, SyncQueueError> {
        item.increment_retry();
        
        if item.retry_count > self.config.max_retries {
            warn!("Item exceeded max retries, dropping: {:?}", item.path);
            return Ok(false);
        }

        // Calculate exponential backoff delay
        let delay_ms = self.config.retry_delay_ms * (2_u64.pow(item.retry_count - 1));
        debug!("Scheduling retry {} for {:?} in {}ms", item.retry_count, item.path, delay_ms);

        // Schedule retry after delay
        let queue = self.queue.clone();
        let path = item.path.clone();
        
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            let mut queue = queue.write().await;
            
            // Insert at appropriate position by priority
            let pos = queue.iter().position(|i| i.priority > item.priority)
                .unwrap_or(queue.len());
            queue.insert(pos, item);
            
            debug!("Retry queued for {:?}", path);
        });

        Ok(true)
    }

    /// Clear the queue
    pub async fn clear(&self) {
        let mut queue = self.queue.write().await;
        queue.clear();
        info!("Sync queue cleared");
    }

    /// Get current queue size
    pub async fn queue_size(&self) -> usize {
        self.queue.read().await.len()
    }

    /// Check if queue is empty
    pub async fn is_empty(&self) -> bool {
        self.queue.read().await.is_empty() && self.in_progress.read().await.is_empty()
    }
}

/// Errors that can occur in the sync queue
#[derive(Debug, thiserror::Error)]
pub enum SyncQueueError {
    #[error("Queue is full")]
    QueueFull,
    
    #[error("Item already in queue")]
    DuplicateItem,
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Sync error: {0}")]
    SyncError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sync_queue_creation() {
        let config = SyncQueueConfig::default();
        let (processor, _rx) = SyncQueueProcessor::new(config);
        
        assert!(processor.is_empty().await);
        assert_eq!(processor.queue_size().await, 0);
    }

    #[tokio::test]
    async fn test_add_item() {
        let config = SyncQueueConfig::default();
        let (processor, _rx) = SyncQueueProcessor::new(config);
        
        let item = SyncItem::new(PathBuf::from("/test/file.txt"), SyncAction::Upload);
        let result = processor.add_item(item).await;
        
        assert!(result.is_ok());
        assert!(result.unwrap());
        assert_eq!(processor.queue_size().await, 1);
    }

    #[tokio::test]
    async fn test_duplicate_prevention() {
        let config = SyncQueueConfig::default();
        let (processor, _rx) = SyncQueueProcessor::new(config);
        
        let item1 = SyncItem::new(PathBuf::from("/test/file.txt"), SyncAction::Upload);
        let item2 = SyncItem::new(PathBuf::from("/test/file.txt"), SyncAction::Upload);
        
        processor.add_item(item1).await.unwrap();
        let result = processor.add_item(item2).await.unwrap();
        
        // Second add should return false (merged)
        assert!(!result);
        assert_eq!(processor.queue_size().await, 1);
    }

    #[tokio::test]
    async fn test_conflict_resolution() {
        let config = SyncQueueConfig {
            warn_on_conflicts: false,
            ..Default::default()
        };
        let (processor, _rx) = SyncQueueProcessor::new(config);
        
        let path = PathBuf::from("/test/file.txt");
        let local_time = Utc::now();
        let remote_time = local_time - chrono::Duration::hours(1);
        
        let resolution = processor.resolve_conflict(
            &path,
            Some(local_time),
            Some(remote_time),
        ).await;
        
        assert_eq!(resolution, ConflictResolutionType::LocalWins);
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        let config = SyncQueueConfig::default();
        let (processor, _rx) = SyncQueueProcessor::new(config);
        
        // Add items with different priorities
        let low_priority = SyncItem::new(PathBuf::from("/low.txt"), SyncAction::Upload)
            .with_priority(200);
        let high_priority = SyncItem::new(PathBuf::from("/high.txt"), SyncAction::Upload)
            .with_priority(50);
        let medium_priority = SyncItem::new(PathBuf::from("/medium.txt"), SyncAction::Upload)
            .with_priority(100);
        
        processor.add_item(low_priority).await.unwrap();
        processor.add_item(high_priority).await.unwrap();
        processor.add_item(medium_priority).await.unwrap();
        
        // Should get highest priority first
        let first = processor.next_item().await.unwrap();
        assert_eq!(first.path, PathBuf::from("/high.txt"));
        
        let second = processor.next_item().await.unwrap();
        assert_eq!(second.path, PathBuf::from("/medium.txt"));
    }

    #[test]
    fn test_sync_item_creation() {
        let item = SyncItem::new(PathBuf::from("/test.txt"), SyncAction::Upload)
            .with_priority(50);
        
        assert_eq!(item.path, PathBuf::from("/test.txt"));
        assert_eq!(item.action, SyncAction::Upload);
        assert_eq!(item.priority, 50);
        assert_eq!(item.retry_count, 0);
    }
}
