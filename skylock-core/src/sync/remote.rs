use std::path::PathBuf;
use tokio::sync::mpsc;
use std::collections::{HashMap, VecDeque};

use crate::{
    Result,
};
use super::{FileState, SyncStatus};
use super::checksum::Checksummer;

pub struct RemoteStateManager {
    checksummer: Checksummer,
    error_tx: mpsc::Sender<crate::error_types::SystemError>,
    remote_base: PathBuf,
}

impl RemoteStateManager {
    pub fn new(
        remote_base: PathBuf,
        error_tx: mpsc::Sender<crate::error_types::SystemError>,
    ) -> Self {
        Self {
            checksummer: Checksummer,
            error_tx,
            remote_base,
        }
    }

    pub async fn upload_file(&self, local_path: &PathBuf, _state: &FileState) -> Result<()> {
        let remote_path = self.get_remote_path(local_path);

        // Ensure remote directory exists
        if let Some(parent) = remote_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Copy file to remote location
        tokio::fs::copy(local_path, &remote_path).await?;

        Ok(())
    }

    pub async fn download_file(&self, remote_path: &PathBuf, local_path: &PathBuf) -> Result<()> {
        // Ensure local directory exists
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Copy file from remote location
        tokio::fs::copy(self.get_remote_path(remote_path), local_path).await?;

        Ok(())
    }

    pub async fn delete_file(&self, path: &PathBuf) -> Result<()> {
        let remote_path = self.get_remote_path(path);
        tokio::fs::remove_file(remote_path).await?;
        Ok(())
    }

    pub async fn get_remote_state(&self, path: &PathBuf) -> Result<Option<FileState>> {
        let remote_path = self.get_remote_path(path);

        if !remote_path.exists() {
            return Ok(None);
        }

        let metadata = tokio::fs::metadata(&remote_path).await?;
        if !metadata.is_file() {
            return Ok(None);
        }

        let checksum = self.checksummer.calculate(&remote_path, &super::ChecksumAlgorithm::XXHash).await?;

        Ok(Some(FileState {
            path: path.clone(),
            modified: metadata.modified()?.into(),
            size: metadata.len(),
            checksum,
            sync_status: SyncStatus::Synced,
            version: 1,
        }))
    }

    pub async fn list_remote_files(&self) -> Result<HashMap<PathBuf, FileState>> {
        let mut states = HashMap::new();
        let mut to_visit = VecDeque::new();
        to_visit.push_back(self.remote_base.clone());

        while let Some(dir) = to_visit.pop_front() {
            let mut entries = tokio::fs::read_dir(&dir).await?;

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();

                if path.is_dir() {
                    to_visit.push_back(path);
                } else {
                    let relative_path = path.strip_prefix(&self.remote_base)
                        .unwrap_or(&path)
                        .to_path_buf();

                    if let Some(state) = self.get_remote_state(&relative_path).await? {
                        states.insert(relative_path, state);
                    }
                }
            }
        }

        Ok(states)
    }

    fn get_remote_path(&self, local_path: &PathBuf) -> PathBuf {
        self.remote_base.join(local_path)
    }
}
