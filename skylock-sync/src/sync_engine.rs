use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use std::collections::{HashMap, HashSet};
use chrono::{DateTime, Utc};
use crate::watcher::{SyncEvent, ChangeType};
use skylock_core::Result;
use tokio::fs;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct SyncState {
    pub path: PathBuf,
    pub last_modified: DateTime<Utc>,
    pub checksum: String,
    pub size: u64,
    pub synced: bool,
}

pub struct SyncEngine {
    base_path: PathBuf,
    sync_states: Arc<RwLock<HashMap<PathBuf, SyncState>>>,
    event_rx: mpsc::Receiver<SyncEvent>,
    progress_tx: mpsc::Sender<SyncProgress>,
    excluded_paths: HashSet<PathBuf>,
    conflict_strategy: ConflictStrategy,
}

#[derive(Debug, Clone)]
pub enum SyncProgress {
    Started {
        path: PathBuf,
        operation: SyncOperation,
    },
    Progress {
        path: PathBuf,
        bytes_processed: u64,
        total_bytes: u64,
    },
    Completed {
        path: PathBuf,
        operation: SyncOperation,
    },
    Error {
        path: PathBuf,
        error: String,
    },
}

#[derive(Debug, Clone)]
pub enum SyncOperation {
    Upload,
    Download,
    Delete,
    Rename(PathBuf),
    Resolve(ConflictResolution),
}

#[derive(Debug, Clone)]
pub enum ConflictStrategy {
    KeepLocal,
    KeepRemote,
    KeepNewer,
    KeepOlder,
    Rename,
    Ask,
}

#[derive(Debug, Clone)]
pub enum ConflictResolution {
    KeepLocal(PathBuf),
    KeepRemote(PathBuf),
    Rename(PathBuf, PathBuf),
}

impl SyncEngine {
    pub fn new(
        base_path: PathBuf,
        event_rx: mpsc::Receiver<SyncEvent>,
        progress_tx: mpsc::Sender<SyncProgress>,
        conflict_strategy: ConflictStrategy,
    ) -> Self {
        Self {
            base_path,
            sync_states: Arc::new(RwLock::new(HashMap::new())),
            event_rx,
            progress_tx,
            excluded_paths: HashSet::new(),
            conflict_strategy,
        }
    }

    pub fn exclude_path<P: AsRef<Path>>(&mut self, path: P) {
        self.excluded_paths.insert(path.as_ref().to_path_buf());
    }

    pub async fn run(&mut self) -> Result<()> {
        // Initial scan
        self.scan_workspace().await?;

        // Process sync events
        while let Some(event) = self.event_rx.recv().await {
            match event {
                SyncEvent::FileChanged { path, change_type, timestamp } => {
                    if self.is_excluded(&path) {
                        continue;
                    }

                    match change_type {
                        ChangeType::Created | ChangeType::Modified => {
                            self.handle_file_change(&path, timestamp).await?;
                        },
                        ChangeType::Deleted => {
                            self.handle_file_deletion(&path).await?;
                        },
                        ChangeType::Renamed(old_path) => {
                            self.handle_file_rename(&old_path, &path).await?;
                        },
                    }
                },
                SyncEvent::Error(err) => {
                    eprintln!("Sync error: {}", err);
                },
            }
        }

        Ok(())
    }

    async fn scan_workspace(&mut self) -> Result<()> {
        let mut stack = vec![self.base_path.clone()];

        while let Some(dir) = stack.pop() {
            let mut entries = fs::read_dir(&dir).await?;

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();

                if self.is_excluded(&path) {
                    continue;
                }

                if path.is_dir() {
                    stack.push(path);
                } else {
                    let metadata = fs::metadata(&path).await?;
                    let state = SyncState {
                        path: path.clone(),
                        last_modified: DateTime::from(metadata.modified()?),
                        checksum: self.calculate_checksum(&path).await?,
                        size: metadata.len(),
                        synced: true,
                    };

                    self.sync_states.write().await.insert(path, state);
                }
            }
        }

        Ok(())
    }

    async fn handle_file_change(&mut self, path: &Path, timestamp: DateTime<Utc>) -> Result<()> {
        let metadata = fs::metadata(path).await?;
        let checksum = self.calculate_checksum(path).await?;

        let mut states = self.sync_states.write().await;

        if let Some(existing_state) = states.get(path) {
            if existing_state.checksum != checksum {
                // File content changed
                self.progress_tx.send(SyncProgress::Started {
                    path: path.to_path_buf(),
                    operation: SyncOperation::Upload,
                }).await?;

                self.sync_file(path).await?;

                states.insert(path.to_path_buf(), SyncState {
                    path: path.to_path_buf(),
                    last_modified: timestamp,
                    checksum,
                    size: metadata.len(),
                    synced: true,
                });

                self.progress_tx.send(SyncProgress::Completed {
                    path: path.to_path_buf(),
                    operation: SyncOperation::Upload,
                }).await?;
            }
        } else {
            // New file
            self.progress_tx.send(SyncProgress::Started {
                path: path.to_path_buf(),
                operation: SyncOperation::Upload,
            }).await?;

            self.sync_file(path).await?;

            states.insert(path.to_path_buf(), SyncState {
                path: path.to_path_buf(),
                last_modified: timestamp,
                checksum,
                size: metadata.len(),
                synced: true,
            });

            self.progress_tx.send(SyncProgress::Completed {
                path: path.to_path_buf(),
                operation: SyncOperation::Upload,
            }).await?;
        }

        Ok(())
    }

    async fn handle_file_deletion(&mut self, path: &Path) -> Result<()> {
        self.progress_tx.send(SyncProgress::Started {
            path: path.to_path_buf(),
            operation: SyncOperation::Delete,
        }).await?;

        // Handle remote deletion
        self.delete_remote_file(path).await?;

        self.sync_states.write().await.remove(path);

        self.progress_tx.send(SyncProgress::Completed {
            path: path.to_path_buf(),
            operation: SyncOperation::Delete,
        }).await?;

        Ok(())
    }

    async fn handle_file_rename(&mut self, old_path: &Path, new_path: &Path) -> Result<()> {
        self.progress_tx.send(SyncProgress::Started {
            path: new_path.to_path_buf(),
            operation: SyncOperation::Rename(old_path.to_path_buf()),
        }).await?;

        // Handle remote rename
        self.rename_remote_file(old_path, new_path).await?;

        let mut states = self.sync_states.write().await;
        if let Some(mut state) = states.remove(old_path) {
            state.path = new_path.to_path_buf();
            states.insert(new_path.to_path_buf(), state);
        }

        self.progress_tx.send(SyncProgress::Completed {
            path: new_path.to_path_buf(),
            operation: SyncOperation::Rename(old_path.to_path_buf()),
        }).await?;

        Ok(())
    }

    async fn sync_file(&self, path: &Path) -> Result<()> {
        let file_size = fs::metadata(path).await?.len();
        let mut bytes_processed = 0;

        // Read the file in chunks and upload
        let mut file = fs::File::open(path).await?;
        let mut buffer = vec![0; 1024 * 1024]; // 1MB chunks

        while let Ok(n) = file.read(&mut buffer).await {
            if n == 0 {
                break;
            }

            // Upload chunk
            self.upload_chunk(path, &buffer[..n]).await?;

            bytes_processed += n as u64;

            self.progress_tx.send(SyncProgress::Progress {
                path: path.to_path_buf(),
                bytes_processed,
                total_bytes: file_size,
            }).await?;
        }

        Ok(())
    }

    async fn calculate_checksum(&self, path: &Path) -> Result<String> {
        use sha2::{Sha256, Digest};
        use tokio::io::AsyncReadExt;

        let mut file = fs::File::open(path).await?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await?;

        let mut hasher = Sha256::new();
        hasher.update(&buffer);
        let result = hasher.finalize();

        Ok(format!("{:x}", result))
    }

    fn is_excluded(&self, path: &Path) -> bool {
        self.excluded_paths.iter().any(|excluded| {
            path.starts_with(excluded)
        })
    }

    async fn upload_chunk(&self, path: &Path, chunk: &[u8]) -> Result<()> {
        // Implementation will depend on the remote storage system
        // For now, just simulate the upload
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        Ok(())
    }

    async fn delete_remote_file(&self, path: &Path) -> Result<()> {
        // Implementation will depend on the remote storage system
        // For now, just simulate the deletion
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        Ok(())
    }

    async fn rename_remote_file(&self, old_path: &Path, new_path: &Path) -> Result<()> {
        // Implementation will depend on the remote storage system
        // For now, just simulate the rename
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        Ok(())
    }

    pub async fn resolve_conflict(&self, path: &Path) -> Result<ConflictResolution> {
        match self.conflict_strategy {
            ConflictStrategy::KeepLocal => {
                Ok(ConflictResolution::KeepLocal(path.to_path_buf()))
            },
            ConflictStrategy::KeepRemote => {
                Ok(ConflictResolution::KeepRemote(path.to_path_buf()))
            },
            ConflictStrategy::KeepNewer => {
                // Compare timestamps and keep the newer version
                let local_time = fs::metadata(path).await?.modified()?;
                let remote_time = self.get_remote_modified_time(path).await?;

                if local_time > remote_time {
                    Ok(ConflictResolution::KeepLocal(path.to_path_buf()))
                } else {
                    Ok(ConflictResolution::KeepRemote(path.to_path_buf()))
                }
            },
            ConflictStrategy::KeepOlder => {
                // Compare timestamps and keep the older version
                let local_time = fs::metadata(path).await?.modified()?;
                let remote_time = self.get_remote_modified_time(path).await?;

                if local_time < remote_time {
                    Ok(ConflictResolution::KeepLocal(path.to_path_buf()))
                } else {
                    Ok(ConflictResolution::KeepRemote(path.to_path_buf()))
                }
            },
            ConflictStrategy::Rename => {
                let new_path = self.generate_unique_path(path).await?;
                Ok(ConflictResolution::Rename(path.to_path_buf(), new_path))
            },
            ConflictStrategy::Ask => {
                // This would typically interact with the UI
                // For now, default to renaming
                let new_path = self.generate_unique_path(path).await?;
                Ok(ConflictResolution::Rename(path.to_path_buf(), new_path))
            },
        }
    }

    async fn get_remote_modified_time(&self, _path: &Path) -> Result<std::time::SystemTime> {
        // Implementation will depend on the remote storage system
        // For now, return current time
        Ok(std::time::SystemTime::now())
    }

    async fn generate_unique_path(&self, path: &Path) -> Result<PathBuf> {
        let file_name = path.file_name()
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid file name"
            ))?.to_string_lossy();

        let extension = path.extension()
            .map(|ext| ext.to_string_lossy().into_owned());

        let stem = path.file_stem()
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid file name"
            ))?.to_string_lossy();

        let mut counter = 1;
        let mut new_path = path.to_path_buf();

        while new_path.exists() {
            new_path = path.with_file_name(format!(
                "{} ({}){}",
                stem,
                counter,
                extension.as_ref().map_or_else(String::new, |ext| format!(".{}", ext))
            ));
            counter += 1;
        }

        Ok(new_path)
    }
}
