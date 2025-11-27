//! Audit Storage Backends
//!
//! Provides different storage options for audit logs.

use super::events::AuditEvent;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tracing::error;

/// Trait for audit log storage backends
#[async_trait]
pub trait AuditStorage {
    /// Write an audit event to storage
    async fn write(&self, event: &AuditEvent) -> Result<(), AuditStorageError>;
    
    /// Query events (optional, not all backends support this)
    async fn query(&self, _filter: &AuditFilter) -> Result<Vec<AuditEvent>, AuditStorageError> {
        Err(AuditStorageError::NotSupported)
    }
}

/// Error type for audit storage operations
#[derive(Debug, thiserror::Error)]
pub enum AuditStorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Operation not supported by this storage backend")]
    NotSupported,
    
    #[error("Storage error: {0}")]
    Other(String),
}

/// Filter for querying audit events
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub categories: Vec<String>,
    pub severities: Vec<super::events::EventSeverity>,
    pub actor_id: Option<String>,
    pub limit: Option<usize>,
}

/// File-based audit storage with rotation support
pub struct FileAuditStorage {
    /// Directory for audit logs
    log_dir: PathBuf,
    /// Current log file
    current_file: Arc<RwLock<Option<File>>>,
    /// Current file name
    current_filename: Arc<RwLock<String>>,
    /// Maximum file size before rotation (bytes)
    max_file_size: u64,
    /// Current file size
    current_size: Arc<RwLock<u64>>,
}

impl FileAuditStorage {
    /// Create a new file-based audit storage
    pub async fn new(log_dir: PathBuf) -> Result<Self, AuditStorageError> {
        // Create directory if it doesn't exist
        tokio::fs::create_dir_all(&log_dir).await?;
        
        let storage = Self {
            log_dir,
            current_file: Arc::new(RwLock::new(None)),
            current_filename: Arc::new(RwLock::new(String::new())),
            max_file_size: 10 * 1024 * 1024, // 10 MB default
            current_size: Arc::new(RwLock::new(0)),
        };
        
        // Open initial file
        storage.rotate_if_needed().await?;
        
        Ok(storage)
    }
    
    /// Set maximum file size before rotation
    pub fn with_max_size(mut self, max_size: u64) -> Self {
        self.max_file_size = max_size;
        self
    }
    
    /// Generate filename for current timestamp
    fn generate_filename(&self) -> String {
        let now = chrono::Utc::now();
        format!("audit_{}.jsonl", now.format("%Y%m%d_%H%M%S"))
    }
    
    /// Rotate log file if needed
    async fn rotate_if_needed(&self) -> Result<(), AuditStorageError> {
        let size = *self.current_size.read().await;
        let needs_rotation = {
            let file = self.current_file.read().await;
            file.is_none() || size >= self.max_file_size
        };
        
        if needs_rotation {
            self.rotate().await?;
        }
        
        Ok(())
    }
    
    /// Force rotation to a new file
    async fn rotate(&self) -> Result<(), AuditStorageError> {
        let new_filename = self.generate_filename();
        let new_path = self.log_dir.join(&new_filename);
        
        let new_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&new_path)
            .await?;
        
        let mut file_lock = self.current_file.write().await;
        let mut filename_lock = self.current_filename.write().await;
        let mut size_lock = self.current_size.write().await;
        
        *file_lock = Some(new_file);
        *filename_lock = new_filename;
        *size_lock = 0;
        
        Ok(())
    }
}

#[async_trait]
impl AuditStorage for FileAuditStorage {
    async fn write(&self, event: &AuditEvent) -> Result<(), AuditStorageError> {
        // Serialize event to JSON line
        let mut json = serde_json::to_string(event)?;
        json.push('\n');
        let bytes = json.as_bytes();
        
        // Check if rotation is needed
        self.rotate_if_needed().await?;
        
        // Write to file
        let mut file_lock = self.current_file.write().await;
        if let Some(ref mut file) = *file_lock {
            file.write_all(bytes).await?;
            file.flush().await?;
            
            // Update size
            let mut size_lock = self.current_size.write().await;
            *size_lock += bytes.len() as u64;
        }
        
        Ok(())
    }
}

/// In-memory audit storage (for testing)
#[derive(Clone)]
pub struct MemoryAuditStorage {
    events: Arc<RwLock<Vec<AuditEvent>>>,
    max_events: usize,
}

impl MemoryAuditStorage {
    /// Create a new in-memory storage
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            max_events: 10000,
        }
    }
    
    /// Create with custom max events
    pub fn with_max_events(max_events: usize) -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
            max_events,
        }
    }
    
    /// Get all stored events
    pub async fn get_events(&self) -> Vec<AuditEvent> {
        self.events.read().await.clone()
    }
    
    /// Clear all events
    pub async fn clear(&self) {
        self.events.write().await.clear();
    }
}

impl Default for MemoryAuditStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuditStorage for MemoryAuditStorage {
    async fn write(&self, event: &AuditEvent) -> Result<(), AuditStorageError> {
        let mut events = self.events.write().await;
        
        // Remove oldest events if at capacity
        if events.len() >= self.max_events {
            events.remove(0);
        }
        
        events.push(event.clone());
        Ok(())
    }
    
    async fn query(&self, filter: &AuditFilter) -> Result<Vec<AuditEvent>, AuditStorageError> {
        let events = self.events.read().await;
        
        let filtered: Vec<AuditEvent> = events
            .iter()
            .filter(|e| {
                // Filter by time range
                if let Some(start) = filter.start_time {
                    if e.timestamp < start {
                        return false;
                    }
                }
                if let Some(end) = filter.end_time {
                    if e.timestamp > end {
                        return false;
                    }
                }
                
                // Filter by category
                if !filter.categories.is_empty() && !filter.categories.contains(&e.category().to_string()) {
                    return false;
                }
                
                // Filter by severity
                if !filter.severities.is_empty() && !filter.severities.contains(&e.severity) {
                    return false;
                }
                
                // Filter by actor
                if let Some(ref actor_id) = filter.actor_id {
                    if &e.actor.id != actor_id {
                        return false;
                    }
                }
                
                true
            })
            .cloned()
            .collect();
        
        // Apply limit
        if let Some(limit) = filter.limit {
            Ok(filtered.into_iter().take(limit).collect())
        } else {
            Ok(filtered)
        }
    }
}

/// Multi-backend storage that writes to multiple backends
pub struct MultiAuditStorage {
    backends: Vec<Arc<dyn AuditStorage + Send + Sync>>,
}

impl MultiAuditStorage {
    /// Create a new multi-backend storage
    pub fn new(backends: Vec<Arc<dyn AuditStorage + Send + Sync>>) -> Self {
        Self { backends }
    }
}

#[async_trait]
impl AuditStorage for MultiAuditStorage {
    async fn write(&self, event: &AuditEvent) -> Result<(), AuditStorageError> {
        let mut last_error = None;
        
        for backend in &self.backends {
            if let Err(e) = backend.write(event).await {
                error!("Failed to write to audit backend: {}", e);
                last_error = Some(e);
            }
        }
        
        // Return last error if any backend failed
        if let Some(e) = last_error {
            Err(e)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::events::{AuditEventType, AuditActor, EventOutcome, EventSeverity};
    
    #[tokio::test]
    async fn test_memory_storage() {
        let storage = MemoryAuditStorage::new();
        
        let event = AuditEvent::new(
            AuditEventType::LoginAttempt {
                username: "test".to_string(),
                method: "password".to_string(),
            },
            AuditActor::user("test"),
            EventOutcome::Success,
        );
        
        storage.write(&event).await.unwrap();
        
        let events = storage.get_events().await;
        assert_eq!(events.len(), 1);
    }
    
    #[tokio::test]
    async fn test_memory_storage_query() {
        let storage = MemoryAuditStorage::new();
        
        // Add multiple events
        for i in 0..5 {
            let event = AuditEvent::new(
                AuditEventType::LoginAttempt {
                    username: format!("user{}", i),
                    method: "password".to_string(),
                },
                AuditActor::user(format!("user{}", i)),
                EventOutcome::Success,
            );
            storage.write(&event).await.unwrap();
        }
        
        // Query with filter
        let filter = AuditFilter {
            actor_id: Some("user2".to_string()),
            ..Default::default()
        };
        
        let results = storage.query(&filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].actor.id, "user2");
    }
    
    #[tokio::test]
    async fn test_memory_storage_max_events() {
        let storage = MemoryAuditStorage::with_max_events(3);
        
        for i in 0..5 {
            let event = AuditEvent::new(
                AuditEventType::LoginAttempt {
                    username: format!("user{}", i),
                    method: "password".to_string(),
                },
                AuditActor::user(format!("user{}", i)),
                EventOutcome::Success,
            );
            storage.write(&event).await.unwrap();
        }
        
        let events = storage.get_events().await;
        assert_eq!(events.len(), 3);
        // Oldest events should be removed
        assert_eq!(events[0].actor.id, "user2");
    }
    
    #[tokio::test]
    async fn test_file_storage() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = FileAuditStorage::new(temp_dir.path().to_path_buf())
            .await
            .unwrap();
        
        let event = AuditEvent::new(
            AuditEventType::ServiceStart {
                service: "test".to_string(),
                version: "1.0".to_string(),
            },
            AuditActor::system(),
            EventOutcome::Success,
        );
        
        storage.write(&event).await.unwrap();
        
        // Verify file was created
        let entries: Vec<_> = std::fs::read_dir(temp_dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }
}
