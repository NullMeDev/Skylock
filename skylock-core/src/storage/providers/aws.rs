use crate::{StorageConfig, StorageItem, Result};
use crate::error::SkylockError;
use crate::storage::{UploadOptions, DownloadOptions, StorageBackend};
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use std::pin::Pin;
use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;

/// AWS S3 storage provider
///
/// Supports AWS S3 and S3-compatible providers (Backblaze B2, Wasabi, MinIO, etc.)
/// 
/// Configuration:
/// - `bucket_name`: S3 bucket name (required)
/// - `region`: AWS region (e.g., "us-east-1", default: "us-east-1")
/// - `endpoint`: Custom endpoint for S3-compatible services (optional)
/// - `access_key_id`: AWS access key (from env or config)
/// - `secret_access_key`: AWS secret key (from env or config)
pub struct AWSStorageProvider {
    config: StorageConfig,
    client: Client,
    bucket_name: String,
}

impl AWSStorageProvider {
    /// Create a new AWS S3 storage provider
    ///
    /// # Example
    /// ```
    /// let mut config = StorageConfig::default();
    /// config.bucket_name = Some("my-backup-bucket".to_string());
    /// config.region = Some("us-east-1".to_string());
    /// let provider = AWSStorageProvider::new(&config).await?;
    /// ```
    pub async fn new(config: &StorageConfig) -> Result<Self> {
        let bucket_name = config.bucket_name.as_ref()
            .ok_or_else(|| SkylockError::Storage(
                "S3 bucket name not provided".to_string()
            ))?;

        // Load AWS configuration
        let aws_config = aws_config::load_from_env().await;
        
        // Create S3 client
        let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&aws_config);

        // Set region if provided
        if let Some(region) = &config.region {
            s3_config_builder = s3_config_builder.region(
                aws_sdk_s3::config::Region::new(region.clone())
            );
        }

        // Set custom endpoint for S3-compatible services
        if let Some(endpoint) = &config.endpoint {
            s3_config_builder = s3_config_builder.endpoint_url(endpoint);
        }

        let s3_config = s3_config_builder.build();
        let client = Client::from_conf(s3_config);

        Ok(Self {
            config: config.clone(),
            client,
            bucket_name: bucket_name.clone(),
        })
    }

    /// Convert PathBuf to S3 key (string)
    fn path_to_key(&self, path: &PathBuf) -> String {
        // Remove leading slash if present
        path.to_string_lossy()
            .trim_start_matches('/')
            .to_string()
    }
}

#[async_trait]
impl StorageBackend for AWSStorageProvider {
    async fn upload(
        &self,
        mut source: Pin<Box<dyn AsyncRead + Send>>,
        destination: &PathBuf,
        options: Option<UploadOptions>,
    ) -> Result<StorageItem> {
        let key = self.path_to_key(destination);

        // Read source into memory (for now - could be optimized with multipart upload)
        let mut buffer = Vec::new();
        source.read_to_end(&mut buffer).await?;

        // Prepare put object request
        let mut request = self.client
            .put_object()
            .bucket(&self.bucket_name)
            .key(&key)
            .body(ByteStream::from(buffer.clone()));

        // Set content type if provided
        if let Some(opts) = options {
            if let Some(content_type) = opts.content_type {
                request = request.content_type(content_type);
            }
        }

        // Execute upload
        let response = request.send().await
            .map_err(|e| SkylockError::Storage(format!("S3 upload failed: {}", e)))?;

        Ok(StorageItem {
            path: destination.clone(),
            size: buffer.len() as u64,
            last_modified: response.e_tag().map(|_| std::time::SystemTime::now()),
            metadata: None,
            etag: response.e_tag().map(|s| s.to_string()),
        })
    }

    async fn download(
        &self,
        source: &PathBuf,
        mut destination: Pin<Box<dyn AsyncWrite + Send>>,
        options: Option<DownloadOptions>,
    ) -> Result<()> {
        let key = self.path_to_key(source);

        // Prepare get object request
        let mut request = self.client
            .get_object()
            .bucket(&self.bucket_name)
            .key(&key);

        // Handle range download if requested
        if let Some(opts) = options {
            if let Some((start, end)) = opts.range {
                let range_str = format!("bytes={}-{}", start, end - 1);
                request = request.range(range_str);
            }
        }

        // Execute download
        let response = request.send().await
            .map_err(|e| SkylockError::Storage(format!("S3 download failed: {}", e)))?;

        // Stream body to destination
        let mut body = response.body.into_async_read();
        tokio::io::copy(&mut body, &mut destination).await?;
        destination.flush().await?;

        Ok(())
    }

    async fn delete(&self, path: &PathBuf) -> Result<()> {
        let key = self.path_to_key(path);

        self.client
            .delete_object()
            .bucket(&self.bucket_name)
            .key(&key)
            .send()
            .await
            .map_err(|e| SkylockError::Storage(format!("S3 delete failed: {}", e)))?;

        Ok(())
    }

    async fn list(
        &self,
        prefix: Option<&PathBuf>,
        recursive: bool,
    ) -> Result<Vec<StorageItem>> {
        let prefix_str = prefix
            .map(|p| self.path_to_key(p))
            .unwrap_or_default();

        let mut request = self.client
            .list_objects_v2()
            .bucket(&self.bucket_name)
            .prefix(&prefix_str);

        // Set delimiter for non-recursive listing
        if !recursive {
            request = request.delimiter("/");
        }

        let response = request.send().await
            .map_err(|e| SkylockError::Storage(format!("S3 list failed: {}", e)))?;

        let mut items = Vec::new();

        if let Some(contents) = response.contents() {
            for object in contents {
                if let (Some(key), Some(size)) = (object.key(), object.size()) {
                    items.push(StorageItem {
                        path: PathBuf::from("/").join(key),
                        size: size as u64,
                        last_modified: object.last_modified()
                            .and_then(|dt| dt.to_system_time().ok()),
                        metadata: None,
                        etag: object.e_tag().map(|s| s.to_string()),
                    });
                }
            }
        }

        Ok(items)
    }

    async fn get_metadata(&self, path: &PathBuf) -> Result<Option<StorageItem>> {
        let key = self.path_to_key(path);

        let response = self.client
            .head_object()
            .bucket(&self.bucket_name)
            .key(&key)
            .send()
            .await;

        match response {
            Ok(head) => Ok(Some(StorageItem {
                path: path.clone(),
                size: head.content_length().unwrap_or(0) as u64,
                last_modified: head.last_modified()
                    .and_then(|dt| dt.to_system_time().ok()),
                metadata: None,
                etag: head.e_tag().map(|s| s.to_string()),
            })),
            Err(e) => {
                // Check if it's a 404 (not found)
                if e.to_string().contains("404") || e.to_string().contains("NotFound") {
                    Ok(None)
                } else {
                    Err(SkylockError::Storage(format!("S3 head object failed: {}", e)))
                }
            }
        }
    }

    async fn copy(
        &self,
        source: &PathBuf,
        destination: &PathBuf,
    ) -> Result<StorageItem> {
        let source_key = self.path_to_key(source);
        let dest_key = self.path_to_key(destination);

        // S3 copy source format: bucket/key
        let copy_source = format!("{}/{}", self.bucket_name, source_key);

        self.client
            .copy_object()
            .bucket(&self.bucket_name)
            .copy_source(&copy_source)
            .key(&dest_key)
            .send()
            .await
            .map_err(|e| SkylockError::Storage(format!("S3 copy failed: {}", e)))?;

        // Get metadata of destination
        self.get_metadata(destination).await?
            .ok_or_else(|| SkylockError::Storage(
                "Copied object not found after copy".to_string()
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_key() {
        let config = StorageConfig {
            bucket_name: Some("test-bucket".to_string()),
            ..Default::default()
        };

        // Note: Can't easily test AWS client creation without credentials
        // These would be integration tests run in CI with real AWS credentials
    }
}
