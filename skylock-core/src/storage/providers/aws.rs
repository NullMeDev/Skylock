use crate::{StorageConfig, StorageItem, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use crate::storage::UploadOptions;
use crate::storage::DownloadOptions;
use tokio::io::{AsyncRead, AsyncWrite};
use std::pin::Pin;
use crate::storage::StorageBackend;

pub struct AWSStorageProvider {
    config: StorageConfig,
}

impl AWSStorageProvider {
    pub async fn new(config: &StorageConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
        })
    }
}

#[async_trait]
impl StorageBackend for AWSStorageProvider {
    async fn upload(
        &self,
        _source: Pin<Box<dyn AsyncRead + Send>>,
        _destination: &PathBuf,
        _options: Option<UploadOptions>,
    ) -> Result<StorageItem> {
        todo!("Implement AWS upload")
    }

    async fn download(
        &self,
        _source: &PathBuf,
        _destination: Pin<Box<dyn AsyncWrite + Send>>,
        _options: Option<DownloadOptions>,
    ) -> Result<()> {
        todo!("Implement AWS download")
    }

    async fn delete(&self, _path: &PathBuf) -> Result<()> {
        todo!("Implement AWS delete")
    }

    async fn list(
        &self,
        _prefix: Option<&PathBuf>,
        _recursive: bool,
    ) -> Result<Vec<StorageItem>> {
        todo!("Implement AWS list")
    }

    async fn get_metadata(&self, _path: &PathBuf) -> Result<Option<StorageItem>> {
        todo!("Implement AWS metadata")
    }

    async fn copy(
        &self,
        _source: &PathBuf,
        _destination: &PathBuf,
    ) -> Result<StorageItem> {
        todo!("Implement AWS copy")
    }
}