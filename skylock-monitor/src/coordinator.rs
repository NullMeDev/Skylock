use std::sync::Arc;
use tokio::sync::RwLock;
use skylock_core::{Result, SkylockError, notifications::NotificationManager};
use crate::{FileMonitor, SyncthingClient, HetznerClient};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, error};

pub struct SyncCoordinator {
    file_monitor: Arc<FileMonitor>,
    syncthing: Arc<SyncthingClient>,
    hetzner: Arc<HetznerClient>,
    notifications: NotificationManager,
    sync_state: Arc<RwLock<HashMap<PathBuf, SyncState>>>,
}

#[derive(Debug, Clone)]
enum SyncState {
    Syncing { bytes_total: u64, bytes_done: u64 },
    Conflicted { local_path: PathBuf, remote_path: PathBuf },
    Completed,
    Failed(String),
}

impl SyncCoordinator {
    pub fn new(
        file_monitor: FileMonitor,
        syncthing: SyncthingClient,
        hetzner: HetznerClient,
        notifications: NotificationManager,
    ) -> Self {
        Self {
            file_monitor: Arc::new(file_monitor),
            syncthing: Arc::new(syncthing),
            hetzner: Arc::new(hetzner),
            notifications,
            sync_state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start(&self) -> Result<()> {
        // Start monitoring Syncthing events
        self.monitor_syncthing_events().await?;

        // Start monitoring local file changes
        self.monitor_local_changes().await?;

        Ok(())
    }

    async fn monitor_syncthing_events(&self) -> Result<()> {
        let syncthing = self.syncthing.clone();
        let state = self.sync_state.clone();
        let notifications = self.notifications.clone();

        tokio::spawn(async move {
            let mut last_event_id = None;
            loop {
                if let Ok(events) = syncthing.get_events(last_event_id).await {
                    for event in events {
                        if let Some(id) = event.id {
                            last_event_id = Some(id);
                        }

                        match event.type_.as_str() {
                            "ItemStarted" => {
                                if let Some(path) = event.data.get("path").and_then(|p| p.as_str()) {
                                    let path = PathBuf::from(path);
                                    state.write().await.insert(
                                        path.clone(),
                                        SyncState::Syncing {
                                            bytes_total: 0,
                                            bytes_done: 0,
                                        }
                                    );
                                }
                            }
                            "ItemFinished" => {
                                if let Some(path) = event.data.get("path").and_then(|p| p.as_str()) {
                                    let path = PathBuf::from(path);
                                    state.write().await.insert(path.clone(), SyncState::Completed);
                                }
                            }
                            "RemoteIndexUpdated" => {
                                // Trigger sync with Hetzner for updated files
                                if let Some(folder) = event.data.get("folder").and_then(|f| f.as_str()) {
                                    notifications.notify_sync_progress(0, 100)?;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        });

        Ok(())
    }

    async fn monitor_local_changes(&self) -> Result<()> {
        let hetzner = self.hetzner.clone();
        let state = self.sync_state.clone();
        let notifications = self.notifications.clone();

        tokio::spawn(async move {
            loop {
                let changes = state.read().await.clone();
                for (path, sync_state) in changes {
                    match sync_state {
                        SyncState::Completed => {
                            // Sync completed file to Hetzner
                            if let Err(e) = hetzner.upload_file(&path, &path).await {
                                error!("Failed to sync to Hetzner: {}", e);
                                notifications.notify_backup_failed(e.to_string())?;
                            }
                        }
                        SyncState::Failed(error) => {
                            notifications.notify_backup_failed(error)?;
                        }
                        _ => {}
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });

        Ok(())
    }

    pub async fn handle_deletion(&self, path: &Path) -> Result<()> {
        // Show deletion prompt
        use skylock_ui::show_deletion_prompt;
        match show_deletion_prompt(path)? {
            skylock_ui::DeletionChoice::DeleteEverywhere => {
                // Delete from Syncthing
                self.syncthing.delete_file(path).await?;
                // Delete from Hetzner
                self.hetzner.delete_file(path).await?;
                self.notifications.notify_file_deleted(path.to_string_lossy().into_owned())?;
            }
            skylock_ui::DeletionChoice::DeleteLocalOnly => {
                // Only delete from Syncthing
                self.syncthing.delete_file(path).await?;
            }
            skylock_ui::DeletionChoice::Cancel => {
                // Do nothing
            }
        }
        Ok(())
    }
}
