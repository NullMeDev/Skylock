use std::path::PathBuf;
use std::pin::Pin;
use async_trait::async_trait;
use chrono::Utc;
use tokio::{io::{AsyncRead, AsyncWrite}, fs};
use crate::{
    Result, SkylockError,
    error_types::{Error, ErrorCategory, ErrorSeverity, StorageErrorType}
};
use super::super::{StorageBackend, StorageItem, StorageConfig, UploadOptions, DownloadOptions};

#[derive(Debug)]
pub struct LocalStorageProvider {
    root_path: PathBuf,
}

impl LocalStorageProvider {
    pub fn new(config: &StorageConfig) -> Result<Self> {
        let root_path = config.connection_string.as_ref()
            .ok_or_else(|| SkylockError::Generic("Local storage requires root path".into()))?
            .into();
        Ok(Self { root_path })
    }

    fn resolve_path(&self, path: &PathBuf) -> PathBuf {
        self.root_path.join(path)
    }
}

#[async_trait]
impl StorageBackend for LocalStorageProvider {
    async fn upload(
        &self,
        mut source: Pin<Box<dyn AsyncRead + Send>>,
        destination: &PathBuf,
        _options: Option<UploadOptions>,
    ) -> Result<StorageItem> {
        let full_path = self.resolve_path(destination);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = fs::File::create(&full_path).await?;
        tokio::io::copy(&mut source, &mut file).await?;
        
        let metadata = fs::metadata(&full_path).await?;
        Ok(StorageItem {
            path: destination.clone(),
            size: metadata.len(),
            last_modified: Some(metadata.modified()?.into()),
            etag: None,
            metadata: None,
        })
    }

    async fn download(
        &self,
        source: &PathBuf,
        mut destination: Pin<Box<dyn AsyncWrite + Send>>,
        _options: Option<DownloadOptions>,
    ) -> Result<()> {
        let full_path = self.resolve_path(source);
        let mut file = fs::File::open(&full_path).await?;
        tokio::io::copy(&mut file, &mut destination).await?;
        Ok(())
    }

    async fn delete(&self, path: &PathBuf) -> Result<()> {
        let full_path = self.resolve_path(path);
        fs::remove_file(&full_path).await?;
        Ok(())
    }

    async fn list(
        &self,
        prefix: Option<&PathBuf>,
        recursive: bool,
    ) -> Result<Vec<StorageItem>> {
        let mut items = Vec::new();
        let search_path = match prefix {
            Some(p) => self.resolve_path(p),
            None => self.root_path.clone(),
        };

        let mut entries = if recursive {
            walkdir::WalkDir::new(&search_path).into_iter()
        } else {
            walkdir::WalkDir::new(&search_path).max_depth(1).into_iter()
        };

        while let Some(entry) = entries.next() {
            let entry = entry.map_err(|e| Error::new(
                ErrorCategory::Storage(StorageErrorType::ReadError),
                ErrorSeverity::Medium,
                format!("Directory walking error: {}", e),
                "local_provider".to_string(),
            ))?;
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let metadata = fs::metadata(path).await?;
            let relative_path = path.strip_prefix(&self.root_path)
                .map_err(|e| Error::new(
                    ErrorCategory::Storage(StorageErrorType::PathNotFound),
                    ErrorSeverity::Medium,
                    format!("Path prefix error: {}", e),
                    "local_provider".to_string(),
                ))?
                .to_path_buf();

            items.push(StorageItem {
                path: relative_path,
                size: metadata.len(),
                last_modified: Some(metadata.modified()?.into()),
                etag: None,
                metadata: None,
            });
        }

        Ok(items)
    }

    async fn get_metadata(&self, path: &PathBuf) -> Result<Option<StorageItem>> {
        let full_path = self.resolve_path(path);
        if !full_path.exists() {
            return Ok(None);
        }

        let metadata = fs::metadata(&full_path).await?;
        Ok(Some(StorageItem {
            path: path.clone(),
            size: metadata.len(),
            last_modified: Some(metadata.modified()?.into()),
            etag: None,
            metadata: None,
        }))
    }

    async fn copy(
        &self,
        source: &PathBuf,
        destination: &PathBuf,
    ) -> Result<StorageItem> {
        let src_path = self.resolve_path(source);
        let dest_path = self.resolve_path(destination);

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::copy(&src_path, &dest_path).await?;
        let metadata = fs::metadata(&dest_path).await?;
        
        Ok(StorageItem {
            path: destination.clone(),
            size: metadata.len(),
            last_modified: Some(metadata.modified()?.into()),
            etag: None,
            metadata: None,
        })
    }
}