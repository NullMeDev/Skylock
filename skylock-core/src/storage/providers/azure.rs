use crate::{StorageConfig, StorageItem, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use crate::storage::UploadOptions;
use crate::storage::DownloadOptions;
use tokio::io::{AsyncRead, AsyncWrite};
use std::pin::Pin;
use crate::storage::StorageBackend;

use azure_storage::StorageCredentials;
use azure_storage_blobs::prelude::*;

pub struct AzureStorageProvider {
    config: StorageConfig,
    client: ContainerClient,
}

impl AzureStorageProvider {
    pub async fn new(config: &StorageConfig) -> Result<Self> {
        let connection_string = config.connection_string.as_ref()
            .ok_or_else(|| SkylockError::Storage("Azure connection string not provided".to_string()))?;
            
        let credentials = StorageCredentials::connection_string(connection_string);
        let client = BlobServiceClient::new(connection_string, credentials)
            .container("skylock-backup")
            .map_err(|e| SkylockError::Storage(format!("Failed to create Azure client: {}", e)))?;

        Ok(Self {
            config: config.clone(),
            client,
        })
    }
}

#[async_trait]
impl StorageBackend for AzureStorageProvider {
    async fn upload(
        &self,
        source: Pin<Box<dyn AsyncRead + Send>>,
        destination: &PathBuf,
        options: Option<UploadOptions>,
    ) -> Result<StorageItem> {
        let blob_client = self.client.blob(destination.to_string_lossy().as_ref());
        
        let mut buffer = Vec::new();
        source.into_async_read().read_to_end(&mut buffer).await?;

        let content_type = options.and_then(|o| o.content_type)
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let response = blob_client
            .put_block_blob(buffer)
            .content_type(&content_type)
            .await
            .map_err(|e| SkylockError::Storage(format!("Azure upload failed: {}", e)))?;

        Ok(StorageItem {
            path: destination.clone(),
            size: response.blob.properties.content_length.unwrap_or(0) as u64,
            last_modified: response.blob.properties.last_modified,
            metadata: None,
            etag: response.blob.properties.etag,
        })
    }

    async fn download(
        &self,
        source: &PathBuf,
        mut destination: Pin<Box<dyn AsyncWrite + Send>>,
        options: Option<DownloadOptions>,
    ) -> Result<()> {
        let blob_client = self.client.blob(source.to_string_lossy().as_ref());

        let response = if let Some(opts) = options {
            if let Some((start, end)) = opts.range {
                blob_client
                    .get()
                    .range(start..end)
                    .await
            } else {
                blob_client.get().await
            }
        } else {
            blob_client.get().await
        };

        let data = response
            .map_err(|e| SkylockError::Storage(format!("Azure download failed: {}", e)))?
            .data;

        destination.write_all(&data).await?;
        destination.flush().await?;

        Ok(())
    }

    async fn delete(&self, path: &PathBuf) -> Result<()> {
        let blob_client = self.client.blob(path.to_string_lossy().as_ref());

        blob_client
            .delete()
            .await
            .map_err(|e| SkylockError::Storage(format!("Azure delete failed: {}", e)))?;

        Ok(())
    }

    async fn list(
        &self,
        prefix: Option<&PathBuf>,
        recursive: bool,
    ) -> Result<Vec<StorageItem>> {
        let prefix_str = prefix.map(|p| p.to_string_lossy().to_string());
        let mut blobs = Vec::new();
        
        let mut stream = self.client.list_blobs()
            .prefix(prefix_str.as_deref().unwrap_or(""))
            .delimiter(if recursive { None } else { Some("/") });

        while let Some(page) = stream.next_page().await
            .map_err(|e| SkylockError::Storage(format!("Azure list failed: {}", e)))? {
            for blob in page.blobs.items {
                blobs.push(StorageItem {
                    path: PathBuf::from(blob.name),
                    size: blob.properties.content_length as u64,
                    last_modified: Some(blob.properties.last_modified),
                    metadata: blob.metadata,
                    etag: Some(blob.properties.etag),
                });
            }
        }

        Ok(blobs)
    }

    async fn get_metadata(&self, path: &PathBuf) -> Result<Option<StorageItem>> {
        let blob_client = self.client.blob(path.to_string_lossy().as_ref());

        match blob_client.get_properties().await {
            Ok(properties) => Ok(Some(StorageItem {
                path: path.clone(),
                size: properties.blob.properties.content_length as u64,
                last_modified: Some(properties.blob.properties.last_modified),
                metadata: properties.blob.metadata,
                etag: Some(properties.blob.properties.etag),
            })),
            Err(e) if e.is_not_found() => Ok(None),
            Err(e) => Err(SkylockError::Storage(format!("Azure metadata lookup failed: {}", e))),
        }
    }

    async fn copy(
        &self,
        source: &PathBuf,
        destination: &PathBuf,
    ) -> Result<StorageItem> {
        let source_blob = self.client.blob(source.to_string_lossy().as_ref());
        let dest_blob = self.client.blob(destination.to_string_lossy().as_ref());

        let source_url = source_blob.url();
        let copy_status = dest_blob
            .copy_from_url(source_url)
            .await
            .map_err(|e| SkylockError::Storage(format!("Azure copy failed: {}", e)))?;

        // Wait for copy to complete
        while let CopyStatus::Pending = copy_status.status {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let status = dest_blob
                .get_properties()
                .await
                .map_err(|e| SkylockError::Storage(format!("Azure copy status check failed: {}", e)))?;

            if let CopyStatus::Success = status.blob.properties.copy_status {
                break;
            }
        }

        // Get the copied blob's metadata
        let properties = dest_blob
            .get_properties()
            .await
            .map_err(|e| SkylockError::Storage(format!("Azure copy metadata lookup failed: {}", e)))?;

        Ok(StorageItem {
            path: destination.clone(),
            size: properties.blob.properties.content_length as u64,
            last_modified: Some(properties.blob.properties.last_modified),
            metadata: properties.blob.metadata,
            etag: Some(properties.blob.properties.etag),
        })
    }
}
