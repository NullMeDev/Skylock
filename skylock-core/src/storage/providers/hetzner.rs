use std::path::PathBuf;
use std::pin::Pin;
use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};
use crate::{Result, SkylockError};
use super::super::{StorageBackend, StorageItem, StorageConfig, UploadOptions, DownloadOptions};

#[cfg(feature = "hetzner-storage")]
#[derive(Debug)]
pub struct HetznerStorageProvider {
    api_token: String,
    box_id: u64,
}

impl HetznerStorageProvider {
    pub async fn new(config: &StorageConfig) -> Result<Self> {
        let api_token = config.api_token.as_ref()
            .ok_or_else(|| SkylockError::Generic("Hetzner API token required".into()))?
            .clone();
        let box_id = config.box_id
            .ok_or_else(|| SkylockError::Generic("Hetzner box ID required".into()))?;

        Ok(Self {
            api_token,
            box_id,
        })
    }
}

#[async_trait]
impl StorageBackend for HetznerStorageProvider {
    async fn upload(
        &self,
        _source: Pin<Box<dyn AsyncRead + Send>>,
        _destination: &PathBuf,
        _options: Option<UploadOptions>,
    ) -> Result<StorageItem> {
        // TODO: Implement upload
        unimplemented!("Hetzner storage provider not yet implemented")
    }

    async fn download(
        &self,
        _source: &PathBuf,
        _destination: Pin<Box<dyn AsyncWrite + Send>>,
        _options: Option<DownloadOptions>,
    ) -> Result<()> {
        // TODO: Implement download
        unimplemented!("Hetzner storage provider not yet implemented")
    }

    async fn delete(&self, _path: &PathBuf) -> Result<()> {
        // TODO: Implement delete
        unimplemented!("Hetzner storage provider not yet implemented")
    }

    async fn list(
        &self,
        _prefix: Option<&PathBuf>,
        _recursive: bool,
    ) -> Result<Vec<StorageItem>> {
        // TODO: Implement list
        unimplemented!("Hetzner storage provider not yet implemented")
    }

    async fn get_metadata(&self, _path: &PathBuf) -> Result<Option<StorageItem>> {
        // TODO: Implement get_metadata
        unimplemented!("Hetzner storage provider not yet implemented")
    }

    async fn copy(
        &self,
        _source: &PathBuf,
        _destination: &PathBuf,
    ) -> Result<StorageItem> {
        // TODO: Implement copy
        unimplemented!("Hetzner storage provider not yet implemented")
    }
}