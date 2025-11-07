mod batch;
mod checksum;
mod conflict;
mod remote;

pub use self::batch::BatchProcessor;
pub use self::checksum::Checksummer;
pub use self::conflict::{Conflict, ConflictResolver};
pub use self::remote::RemoteStateManager;

use std::collections::HashMap;
use tokio::sync::mpsc;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use crate::{Result, SystemError};
use std::sync::Arc;
use tokio::sync::RwLock;
use notify::{Watcher, RecursiveMode, Event};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub directories: Vec<PathBuf>,
    pub ignore_patterns: Vec<String>,
    pub sync_interval: chrono::Duration,
    pub batch_size: usize,
    pub conflict_resolution: ConflictResolution,
    pub checksum_algorithm: ChecksumAlgorithm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictResolution {
    KeepNewest,
    KeepOldest,
    KeepBoth,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChecksumAlgorithm {
    MD5,
    SHA256,
    XXHash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileState {
    pub path: PathBuf,
    pub modified: DateTime<Utc>,
    pub size: u64,
    pub checksum: String,
    pub sync_status: SyncStatus,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncStatus {
    Synced,
    Modified,
    Conflicted,
    Pending,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct SyncEvent {
    pub path: PathBuf,
    pub event_type: SyncEventType,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum SyncEventType {
    Create,
    Modify,
    Delete,
    Rename(PathBuf), // Contains the new path
}

pub struct FileSync {
    config: SyncConfig,
    state: Arc<RwLock<HashMap<PathBuf, FileState>>>,
    watcher: Option<Box<dyn Watcher>>,
    event_tx: mpsc::Sender<SyncEvent>,
    error_tx: mpsc::Sender<SystemError>,
}

impl FileSync {
    pub fn new(config: SyncConfig, error_tx: mpsc::Sender<SystemError>) -> Result<(Self, mpsc::Receiver<SyncEvent>)> {
        let (event_tx, event_rx) = mpsc::channel(1000);

        Ok((Self {
            config,
            state: Arc::new(RwLock::new(HashMap::new())),
            watcher: None,
            event_tx,
            error_tx,
        }, event_rx))
    }

    pub async fn start(&mut self) -> Result<()> {
        // Initialize the file watcher
        let event_tx = self.event_tx.clone();
        let mut watcher = notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
            let event_tx = event_tx.clone();
            tokio::spawn(async move {
                match res {
                    Ok(event) => {
                        let sync_event = match event.kind {
                            notify::EventKind::Create(_) => Some(SyncEventType::Create),
                            notify::EventKind::Modify(_) => Some(SyncEventType::Modify),
                            notify::EventKind::Remove(_) => Some(SyncEventType::Delete),
                            notify::EventKind::Access(notify::event::AccessKind::Close(_)) => {
                                // Handle rename events separately as they need both paths
                                None
                            },
                            _ => None,
                        };

                        if let Some(event_type) = sync_event {
                            for path in event.paths {
                                let _ = event_tx.send(SyncEvent {
                                    path,
                                    event_type: event_type.clone(),
                                    timestamp: Utc::now(),
                                }).await;
                            }
                        }
                    },
                    Err(e) => {
                        // Handle watcher errors
                        eprintln!("Watch error: {:?}", e);
                    }
                }
            });
        })?;

        // Start watching configured directories
        for dir in &self.config.directories {
            watcher.watch(dir, RecursiveMode::Recursive)?;
        }

        self.watcher = Some(Box::new(watcher));
        Ok(())
    }

    pub async fn process_event(&self, event: SyncEvent) -> Result<()> {
        let mut state = self.state.write().await;

        match event.event_type {
            SyncEventType::Create | SyncEventType::Modify => {
                let file_state = self.create_file_state(&event.path).await?;
                state.insert(event.path, file_state);
            },
            SyncEventType::Delete => {
                state.remove(&event.path);
            },
            SyncEventType::Rename(new_path) => {
                if let Some(file_state) = state.remove(&event.path) {
                    let mut updated_state = file_state;
                    updated_state.path = new_path.clone();
                    state.insert(new_path, updated_state);
                }
            },
        }

        Ok(())
    }

    async fn create_file_state(&self, path: &PathBuf) -> Result<FileState> {
        // Get file metadata
        let metadata = tokio::fs::metadata(path).await?;

        // Calculate checksum
        let checksum = self.calculate_checksum(path).await?;

        Ok(FileState {
            path: path.clone(),
            modified: metadata.modified()?.into(),
            size: metadata.len(),
            checksum,
            sync_status: SyncStatus::Pending,
            version: 1,
        })
    }

    async fn calculate_checksum(&self, path: &PathBuf) -> Result<String> {
        // Implement checksum calculation based on configured algorithm
        match self.config.checksum_algorithm {
            ChecksumAlgorithm::MD5 => {
                // Implement MD5 checksum
                Ok("TODO: implement MD5".to_string())
            },
            ChecksumAlgorithm::SHA256 => {
                // Implement SHA256 checksum
                Ok("TODO: implement SHA256".to_string())
            },
            ChecksumAlgorithm::XXHash => {
                // Implement XXHash checksum
                Ok("TODO: implement XXHash".to_string())
            },
        }
    }

    pub async fn sync_all(&self) -> Result<()> {
        let state = self.state.read().await;

        // Group files into batches
        let mut batch = Vec::with_capacity(self.config.batch_size);

        for (path, file_state) in state.iter() {
            match file_state.sync_status {
                SyncStatus::Modified | SyncStatus::Pending => {
                    batch.push((path.clone(), file_state.clone()));

                    if batch.len() >= self.config.batch_size {
                        self.sync_batch(&batch).await?;
                        batch.clear();
                    }
                },
                _ => continue,
            }
        }

        // Sync remaining files
        if !batch.is_empty() {
            self.sync_batch(&batch).await?;
        }

        Ok(())
    }

    async fn sync_batch(&self, batch: &[(PathBuf, FileState)]) -> Result<()> {
        let mut processor = BatchProcessor::new(
            self.config.conflict_resolution.clone(),
            self.error_tx.clone(),
        );

        let remote_manager = RemoteStateManager::new(
            PathBuf::from("/remote/sync"), // TODO: Make configurable
            self.error_tx.clone(),
        );

        // Get current remote states for files in batch
        for (path, _) in batch {
            if let Some(remote_state) = remote_manager.get_remote_state(path).await? {
                processor.update_remote_state(path.clone(), remote_state).await;
            }
        }

        // Process the batch and handle conflicts
        let mut batch_vec = batch.to_vec();
        let processed = processor.process_batch(&mut batch_vec).await?;

        // Sync processed files
        for (path, state) in processed {
            // Upload local changes
            remote_manager.upload_file(&path, &state).await?;

            // Update local state
            let mut local_state = self.state.write().await;
            if let Some(file_state) = local_state.get_mut(&path) {
                file_state.sync_status = SyncStatus::Synced;
                file_state.version += 1;
            }
        }

        // Handle deletions
        let local_state = self.state.read().await;
        let deletions = processor.process_deletions(&local_state).await?;

        for path in deletions {
            remote_manager.delete_file(&path).await?;
        }

        Ok(())
    }
}
