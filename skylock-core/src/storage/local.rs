use std::path::PathBuf;
use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite};
use std::pin::Pin;
use tokio::fs::{self, File};
use crate::Result;
use super::{StorageProvider, StorageConfig, StorageItem, UploadOptions, DownloadOptions};
use tokio::io::AsyncWriteExt;
use tokio_util::io::StreamReader;
use futures::StreamExt;
use walkdir::WalkDir;

pub struct LocalStorageProvider {
    root_path: PathBuf,
}

impl LocalStorageProvider {
    pub fn new(config: &StorageConfig) -> Result<Self> {
        let root_path = PathBuf::from(&config.connection_string);
        std::fs::create_dir_all(&root_path)?;

        Ok(Self { root_path })
    }

    fn get_absolute_path(&self, path: &PathBuf) -> PathBuf {
        self.root_path.join(path)
    }
}

#[async_trait]
impl StorageProvider for LocalStorageProvider {
    async fn upload(
        &self,
        mut source: Pin<Box<dyn AsyncRead + Send>>,
        destination: &PathBuf,
        options: Option<UploadOptions>,
    ) -> Result<StorageItem> {
        let abs_path = self.get_absolute_path(destination);

        // Create parent directories if they don't exist
        if let Some(parent) = abs_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = File::create(&abs_path).await?;
        let chunk_size = options
            .and_then(|opt| opt.chunk_size)
            .unwrap_or(8192);

        let mut buffer = vec![0u8; chunk_size];
        loop {
            let n = source.as_mut().read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            file.write_all(&buffer[..n]).await?;
        }

        // Get file metadata
        let metadata = fs::metadata(&abs_path).await?;

        Ok(StorageItem {
            path: destination.clone(),
            size: metadata.len(),
            modified: metadata.modified()?.into(),
            etag: None,
            metadata: options.and_then(|opt| opt.metadata),
        })
    }

    async fn download(
        &self,
        source: &PathBuf,
        mut destination: Pin<Box<dyn AsyncWrite + Send>>,
        options: Option<DownloadOptions>,
    ) -> Result<()> {
        let abs_path = self.get_absolute_path(source);
        let mut file = File::open(&abs_path).await?;

        let chunk_size = options
            .and_then(|opt| opt.chunk_size)
            .unwrap_or(8192);

        let mut buffer = vec![0u8; chunk_size];
        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            destination.as_mut().write_all(&buffer[..n]).await?;
        }

        Ok(())
    }

    async fn delete(&self, path: &PathBuf) -> Result<()> {
        let abs_path = self.get_absolute_path(path);
        fs::remove_file(abs_path).await?;
        Ok(())
    }

    async fn list(
        &self,
        prefix: Option<&PathBuf>,
        recursive: bool,
    ) -> Result<Vec<StorageItem>> {
        let mut items = Vec::new();
        let base_path = match prefix {
            Some(p) => self.get_absolute_path(p),
            None => self.root_path.clone(),
        };

        let walker = WalkDir::new(&base_path)
            .min_depth(0)
            .max_depth(if recursive { std::usize::MAX } else { 1 })
            .into_iter();

        for entry in walker.filter_entry(|e| e.file_type().is_file()) {
            let entry = entry?;
            let path = entry.path();
            let metadata = entry.metadata()?;

            if metadata.is_file() {
                let relative_path = path.strip_prefix(&self.root_path)?;
                items.push(StorageItem {
                    path: relative_path.to_path_buf(),
                    size: metadata.len(),
                    modified: metadata.modified()?.into(),
                    etag: None,
                    metadata: None,
                });
            }
        }

        Ok(items)
    }

    async fn get_metadata(&self, path: &PathBuf) -> Result<Option<StorageItem>> {
        let abs_path = self.get_absolute_path(path);

        if !abs_path.exists() {
            return Ok(None);
        }

        let metadata = fs::metadata(&abs_path).await?;

        Ok(Some(StorageItem {
            path: path.clone(),
            size: metadata.len(),
            modified: metadata.modified()?.into(),
            etag: None,
            metadata: None,
        }))
    }

    async fn copy(
        &self,
        source: &PathBuf,
        destination: &PathBuf,
    ) -> Result<StorageItem> {
        let source_path = self.get_absolute_path(source);
        let dest_path = self.get_absolute_path(destination);

        // Create parent directories if they don't exist
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::copy(&source_path, &dest_path).await?;

        let metadata = fs::metadata(&dest_path).await?;

        Ok(StorageItem {
            path: destination.clone(),
            size: metadata.len(),
            modified: metadata.modified()?.into(),
            etag: None,
            metadata: None,
        })
    }
}
