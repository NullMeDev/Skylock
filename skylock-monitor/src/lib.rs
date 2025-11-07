pub mod error;

use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event};
use error::{Result, Error};
use skylock_hetzner::HetznerClient;
use skylock_sync::SyncthingClient;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tracing::{info, error, debug};

pub struct FileMonitor {
    watcher: RecommendedWatcher,
    hetzner: HetznerClient,
    syncthing: SyncthingClient,
    watched_paths: Vec<PathBuf>,
    event_tx: mpsc::Sender<notify::Event>,
}

#[derive(Debug)]
enum FileAction {
    Upload,
    Delete,
    Modify,
}

impl FileMonitor {
    pub fn new(
        hetzner: HetznerClient,
        syncthing: SyncthingClient,
        watched_paths: Vec<PathBuf>,
    ) -> Result<(Self, mpsc::Receiver<notify::Event>)> {
        let (event_tx, event_rx) = mpsc::channel(100);
        let event_tx_clone = event_tx.clone();

        let watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            match res {
                Ok(event) => {
                    let _ = event_tx_clone.blocking_send(event);
                }
                Err(e) => error!("Watch error: {:?}", e),
            }
        })
        .map_err(|e| Error::Other(format!("Monitor error: {}", e)))?;

        Ok((
            Self {
                watcher,
                hetzner,
                syncthing,
                watched_paths,
                event_tx,
            },
            event_rx,
        ))
    }

    pub async fn start(&mut self) -> Result<()> {
        for path in &self.watched_paths {
            self.watcher.watch(path, RecursiveMode::Recursive)?;
            info!("Watching path: {}", path.display());
        }

        Ok(())
    }

    pub async fn process_event(&self, event: notify::Event) -> Result<()> {
        match event.kind {
            notify::EventKind::Create(_) => {
                for path in event.paths {
                    self.handle_file_action(path, FileAction::Upload).await?;
                }
            }
            notify::EventKind::Modify(_) => {
                for path in event.paths {
                    self.handle_file_action(path, FileAction::Modify).await?;
                }
            }
            notify::EventKind::Remove(_) => {
                for path in event.paths {
                    self.handle_file_action(path, FileAction::Delete).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_file_action(&self, path: PathBuf, action: FileAction) -> Result<()> {
        // Convert local path to remote path
        let remote_path = self.local_to_remote_path(&path)?;

        match action {
            FileAction::Upload | FileAction::Modify => {
                if path.is_file() {
                    debug!("Uploading file: {} -> {}", path.display(), remote_path.display());
                    self.hetzner.upload_file(&path, &remote_path).await?;
                }
            }
            FileAction::Delete => {
                debug!("Deleting file: {}", remote_path.display());
                self.hetzner.delete_file(&remote_path).await?;
            }
        }

        // Trigger Syncthing scan for the affected folder
        if let Some(folder_id) = self.get_syncthing_folder_id(&path) {
            self.syncthing.scan_folder(&folder_id).await?;
        }

        Ok(())
    }

    fn local_to_remote_path(&self, local_path: &Path) -> Result<PathBuf> {
        for watched_path in &self.watched_paths {
            if let Ok(relative) = local_path.strip_prefix(watched_path) {
                return Ok(PathBuf::from("backup").join(relative));
            }
        }
        Err(Error::Other(format!(
            "Path '{}' is not within any watched directories: {:?}",
            local_path.display(),
            self.watched_paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        )))
    }

    fn get_syncthing_folder_id(&self, path: &Path) -> Option<String> {
        for watched_path in &self.watched_paths {
            if path.starts_with(watched_path) {
                return Some(watched_path.to_string_lossy().replace(':', "").replace('\\', "-"));
            }
        }
        None
    }
}
