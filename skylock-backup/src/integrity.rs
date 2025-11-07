use std::path::{Path, PathBuf};
use tokio::fs;
use futures::StreamExt;
use sha2::{Sha256, Digest};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use skylock_core::{Result, SkylockError};
use tracing::{info, warn};

#[derive(Debug, Serialize, Deserialize)]
pub struct FileIntegrity {
    path: PathBuf,
    size: u64,
    modified: DateTime<Utc>,
    hash: String,
}

pub struct IntegrityManager {
    base_path: PathBuf,
    integrity_db: PathBuf,
}

impl IntegrityManager {
    pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
        let mut integrity_db = base_path.as_ref().to_path_buf();
        integrity_db.push(".skylock");
        integrity_db.push("integrity.db");

        Self {
            base_path: base_path.as_ref().to_path_buf(),
            integrity_db,
        }
    }

    pub async fn verify_integrity(&self, path: &Path) -> Result<bool> {
        let stored = self.load_integrity(path).await?;
        let current = self.compute_integrity(path).await?;

        Ok(stored.hash == current.hash)
    }

    pub async fn update_integrity(&self, path: &Path) -> Result<()> {
        let integrity = self.compute_integrity(path).await?;
        self.store_integrity(&integrity).await
    }

    async fn compute_integrity(&self, path: &Path) -> Result<FileIntegrity> {
        let metadata = fs::metadata(path).await?;
        let mut file = fs::File::open(path).await?;
        let mut hasher = Sha256::new();

        let mut buffer = vec![0; 1024 * 1024]; // 1MB buffer
        while let Ok(n) = file.read(&mut buffer).await {
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        Ok(FileIntegrity {
            path: path.strip_prefix(&self.base_path)?.to_path_buf(),
            size: metadata.len(),
            modified: metadata.modified()?.into(),
            hash: format!("{:x}", hasher.finalize()),
        })
    }

    async fn load_integrity(&self, path: &Path) -> Result<FileIntegrity> {
        let relative_path = path.strip_prefix(&self.base_path)?;
        let mut db_path = self.integrity_db.clone();
        db_path.push(relative_path);
        db_path.set_extension("integrity");

        let data = fs::read_to_string(&db_path).await?;
        Ok(serde_json::from_str(&data)?)
    }

    async fn store_integrity(&self, integrity: &FileIntegrity) -> Result<()> {
        let mut db_path = self.integrity_db.clone();
        db_path.push(&integrity.path);
        db_path.set_extension("integrity");

        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let data = serde_json::to_string_pretty(integrity)?;
        fs::write(&db_path, data).await?;
        Ok(())
    }

    pub async fn verify_all(&self) -> Result<Vec<PathBuf>> {
        let mut failed = Vec::new();
        let mut entries = fs::read_dir(&self.base_path).await?;

        while let Some(entry) = entries.next().await {
            let entry = entry?;
            let path = entry.path();

            if !self.verify_integrity(&path).await? {
                failed.push(path);
            }
        }

        Ok(failed)
    }

    pub async fn rebuild_database(&self) -> Result<()> {
        // Clean existing database
        if self.integrity_db.exists() {
            fs::remove_dir_all(&self.integrity_db).await?;
        }
        fs::create_dir_all(&self.integrity_db).await?;

        // Rebuild for all files
        let mut entries = fs::read_dir(&self.base_path).await?;
        while let Some(entry) = entries.next().await {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                self.update_integrity(&path).await?;
            }
        }

        Ok(())
    }
}
