//! AWS S3 Storage Provider
//!
//! Provides integration with AWS S3 and S3-compatible storage services.
//! Features:
//! - Standard single-part uploads for small files
//! - Multipart uploads for large files (>100MB by default)
//! - Server-side encryption (SSE-S3, SSE-KMS)
//! - Streaming downloads with range support
//! - S3-compatible services (MinIO, Wasabi, DigitalOcean Spaces, etc.)

use crate::storage::{StorageConfig, StorageItem, UploadOptions, DownloadOptions, StorageBackend};
use crate::{Result, SkylockError};
use async_trait::async_trait;
use std::path::PathBuf;
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart, ServerSideEncryption};
use chrono::{DateTime, Utc};
use tracing::{info, debug, warn, error};

/// Default multipart upload threshold (100 MB)
const DEFAULT_MULTIPART_THRESHOLD: u64 = 100 * 1024 * 1024;

/// Default multipart part size (10 MB)
const DEFAULT_PART_SIZE: u64 = 10 * 1024 * 1024;

/// Minimum part size allowed by S3 (5 MB)
const MIN_PART_SIZE: u64 = 5 * 1024 * 1024;

/// Maximum part size allowed by S3 (5 GB)  
const MAX_PART_SIZE: u64 = 5 * 1024 * 1024 * 1024;

/// Maximum number of parts in a multipart upload
const MAX_PARTS: usize = 10000;

/// AWS S3 storage provider
#[derive(Debug)]
pub struct AWSStorageProvider {
    #[allow(dead_code)]
    config: StorageConfig,
    client: Client,
    bucket_name: String,
    multipart_threshold: u64,
    part_size: u64,
}

impl AWSStorageProvider {
    /// Create a new AWS S3 storage provider
    pub async fn new(config: &StorageConfig) -> Result<Self> {
        let bucket_name = config.bucket_name.as_ref()
            .ok_or_else(|| SkylockError::Storage(
                crate::error_types::StorageErrorType::ConfigError
            ))?;

        // Load AWS configuration from environment
        let mut aws_config_loader = aws_config::from_env();

        // Set region if provided in config
        if let Some(region) = &config.region {
            aws_config_loader = aws_config_loader.region(
                aws_sdk_s3::config::Region::new(region.clone())
            );
        }

        let aws_config = aws_config_loader.load().await;

        // Build S3 client configuration  
        let mut s3_config_builder = aws_sdk_s3::config::Builder::from(&aws_config);

        // Set region again for S3 config if provided
        if let Some(region) = &config.region {
            s3_config_builder = s3_config_builder.region(
                aws_sdk_s3::config::Region::new(region.clone())
            );
        }

        // Set custom endpoint for S3-compatible services
        if let Some(endpoint) = &config.endpoint {
            s3_config_builder = s3_config_builder.endpoint_url(endpoint);
            s3_config_builder = s3_config_builder.force_path_style(true);
        }

        let s3_config = s3_config_builder.build();
        let client = Client::from_conf(s3_config);

        // Validate multipart settings
        let multipart_threshold = config.multipart_threshold
            .unwrap_or(DEFAULT_MULTIPART_THRESHOLD);
        
        let mut part_size = config.multipart_part_size
            .unwrap_or(DEFAULT_PART_SIZE);
        
        if part_size < MIN_PART_SIZE {
            warn!("Part size {} is below minimum {}, using minimum", part_size, MIN_PART_SIZE);
            part_size = MIN_PART_SIZE;
        }
        if part_size > MAX_PART_SIZE {
            warn!("Part size {} exceeds maximum {}, using maximum", part_size, MAX_PART_SIZE);
            part_size = MAX_PART_SIZE;
        }

        info!(
            "AWS S3 provider initialized: bucket={}, region={:?}, endpoint={:?}",
            bucket_name, config.region, config.endpoint
        );

        Ok(Self {
            config: config.clone(),
            client,
            bucket_name: bucket_name.clone(),
            multipart_threshold,
            part_size,
        })
    }

    fn path_to_key(&self, path: &PathBuf) -> String {
        path.to_string_lossy().trim_start_matches('/').to_string()
    }

    fn get_sse_config(&self) -> Option<ServerSideEncryption> {
        match self.config.server_side_encryption.as_deref() {
            Some("AES256") => Some(ServerSideEncryption::Aes256),
            Some("aws:kms") => Some(ServerSideEncryption::AwsKms),
            _ => None,
        }
    }

    async fn upload_single(&self, data: Vec<u8>, key: &str, content_type: Option<String>) -> Result<StorageItem> {
        let size = data.len() as u64;
        let mut request = self.client
            .put_object()
            .bucket(&self.bucket_name)
            .key(key)
            .body(ByteStream::from(data));

        if let Some(ct) = content_type {
            request = request.content_type(ct);
        }

        if let Some(sse) = self.get_sse_config() {
            request = request.server_side_encryption(sse);
        }

        let response = request.send().await
            .map_err(|e| {
                error!("S3 upload failed: {}", e);
                SkylockError::Storage(crate::error_types::StorageErrorType::WriteError)
            })?;

        Ok(StorageItem {
            path: PathBuf::from("/").join(key),
            size,
            last_modified: Some(Utc::now()),
            metadata: None,
            etag: response.e_tag().map(|s| s.to_string()),
        })
    }

    async fn upload_multipart(&self, data: Vec<u8>, key: &str, content_type: Option<String>) -> Result<StorageItem> {
        // Start multipart upload
        let mut create_req = self.client
            .create_multipart_upload()
            .bucket(&self.bucket_name)
            .key(key);

        if let Some(ct) = &content_type {
            create_req = create_req.content_type(ct);
        }

        if let Some(sse) = self.get_sse_config() {
            create_req = create_req.server_side_encryption(sse);
        }

        let create_resp = create_req.send().await
            .map_err(|e| {
                error!("Failed to start multipart upload: {}", e);
                SkylockError::Storage(crate::error_types::StorageErrorType::WriteError)
            })?;

        let upload_id = create_resp.upload_id()
            .ok_or_else(|| SkylockError::Storage(crate::error_types::StorageErrorType::WriteError))?
            .to_string();

        let mut completed_parts: Vec<CompletedPart> = Vec::new();
        let mut offset = 0usize;
        let mut part_number = 1i32;
        let total_size = data.len();

        // Upload parts
        while offset < total_size {
            let end = std::cmp::min(offset + self.part_size as usize, total_size);
            let part_data = data[offset..end].to_vec();

            let upload_resp = self.client
                .upload_part()
                .bucket(&self.bucket_name)
                .key(key)
                .upload_id(&upload_id)
                .part_number(part_number)
                .body(ByteStream::from(part_data))
                .send()
                .await
                .map_err(|e| {
                    error!("Failed to upload part {}: {}", part_number, e);
                    SkylockError::Storage(crate::error_types::StorageErrorType::WriteError)
                })?;

            let etag = upload_resp.e_tag()
                .ok_or_else(|| SkylockError::Storage(crate::error_types::StorageErrorType::WriteError))?
                .to_string();

            completed_parts.push(
                CompletedPart::builder()
                    .part_number(part_number)
                    .e_tag(etag)
                    .build()
            );

            offset = end;
            part_number += 1;

            if part_number > MAX_PARTS as i32 {
                let _ = self.client
                    .abort_multipart_upload()
                    .bucket(&self.bucket_name)
                    .key(key)
                    .upload_id(&upload_id)
                    .send()
                    .await;
                return Err(SkylockError::Storage(crate::error_types::StorageErrorType::WriteError));
            }
        }

        // Complete multipart upload
        let completed = CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();

        let complete_resp = self.client
            .complete_multipart_upload()
            .bucket(&self.bucket_name)
            .key(key)
            .upload_id(&upload_id)
            .multipart_upload(completed)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to complete multipart upload: {}", e);
                SkylockError::Storage(crate::error_types::StorageErrorType::WriteError)
            })?;

        info!("Multipart upload complete: key={}, parts={}", key, part_number - 1);

        Ok(StorageItem {
            path: PathBuf::from("/").join(key),
            size: total_size as u64,
            last_modified: Some(Utc::now()),
            metadata: None,
            etag: complete_resp.e_tag().map(|s| s.to_string()),
        })
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
        let content_type = options.as_ref().and_then(|o| o.content_type.clone());

        // Read all data into memory
        let mut data = Vec::new();
        source.read_to_end(&mut data).await?;

        if data.len() as u64 <= self.multipart_threshold {
            debug!("Using single upload for key={}, size={}", key, data.len());
            self.upload_single(data, &key, content_type).await
        } else {
            debug!("Using multipart upload for key={}, size={}", key, data.len());
            self.upload_multipart(data, &key, content_type).await
        }
    }

    async fn download(
        &self,
        source: &PathBuf,
        mut destination: Pin<Box<dyn AsyncWrite + Send>>,
        options: Option<DownloadOptions>,
    ) -> Result<()> {
        let key = self.path_to_key(source);

        let mut request = self.client
            .get_object()
            .bucket(&self.bucket_name)
            .key(&key);

        if let Some(opts) = options {
            if let Some((start, end)) = opts.range {
                request = request.range(format!("bytes={}-{}", start, end - 1));
            }
        }

        let response = request.send().await
            .map_err(|e| {
                error!("S3 download failed for key={}: {}", key, e);
                SkylockError::Storage(crate::error_types::StorageErrorType::ReadError)
            })?;

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
            .map_err(|e| {
                error!("S3 delete failed: {}", e);
                SkylockError::Storage(crate::error_types::StorageErrorType::DeleteError)
            })?;

        Ok(())
    }

    async fn list(
        &self,
        prefix: Option<&PathBuf>,
        recursive: bool,
    ) -> Result<Vec<StorageItem>> {
        let prefix_str = prefix.map(|p| self.path_to_key(p)).unwrap_or_default();
        let mut items = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut request = self.client
                .list_objects_v2()
                .bucket(&self.bucket_name)
                .prefix(&prefix_str);

            if !recursive {
                request = request.delimiter("/");
            }

            if let Some(token) = &continuation_token {
                request = request.continuation_token(token);
            }

            let response = request.send().await
                .map_err(|e| {
                    error!("S3 list failed: {}", e);
                    SkylockError::Storage(crate::error_types::StorageErrorType::ReadError)
                })?;

            if let Some(contents) = response.contents() {
                for object in contents {
                    if let Some(key) = object.key() {
                        items.push(StorageItem {
                            path: PathBuf::from("/").join(key),
                            size: object.size() as u64,
                            last_modified: object.last_modified()
                                .map(|dt| {
                                    DateTime::<Utc>::from_timestamp(dt.secs(), dt.subsec_nanos())
                                        .unwrap_or_else(Utc::now)
                                }),
                            metadata: None,
                            etag: object.e_tag().map(|s| s.to_string()),
                        });
                    }
                }
            }

            if response.is_truncated() {
                continuation_token = response.next_continuation_token().map(String::from);
            } else {
                break;
            }
        }

        Ok(items)
    }

    async fn get_metadata(&self, path: &PathBuf) -> Result<Option<StorageItem>> {
        let key = self.path_to_key(path);

        match self.client.head_object().bucket(&self.bucket_name).key(&key).send().await {
            Ok(head) => Ok(Some(StorageItem {
                path: path.clone(),
                size: head.content_length() as u64,
                last_modified: head.last_modified()
                    .map(|dt| {
                        DateTime::<Utc>::from_timestamp(dt.secs(), dt.subsec_nanos())
                            .unwrap_or_else(Utc::now)
                    }),
                metadata: None,
                etag: head.e_tag().map(|s| s.to_string()),
            })),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("404") || err_str.contains("NotFound") {
                    Ok(None)
                } else {
                    Err(SkylockError::Storage(crate::error_types::StorageErrorType::ReadError))
                }
            }
        }
    }

    async fn copy(&self, source: &PathBuf, destination: &PathBuf) -> Result<StorageItem> {
        let source_key = self.path_to_key(source);
        let dest_key = self.path_to_key(destination);
        let copy_source = format!("{}/{}", self.bucket_name, source_key);

        let mut request = self.client
            .copy_object()
            .bucket(&self.bucket_name)
            .copy_source(&copy_source)
            .key(&dest_key);

        if let Some(sse) = self.get_sse_config() {
            request = request.server_side_encryption(sse);
        }

        request.send().await
            .map_err(|e| {
                error!("S3 copy failed: {}", e);
                SkylockError::Storage(crate::error_types::StorageErrorType::WriteError)
            })?;

        self.get_metadata(destination).await?
            .ok_or_else(|| SkylockError::Storage(crate::error_types::StorageErrorType::PathNotFound))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_key() {
        let path = PathBuf::from("/backups/2024/file.enc");
        assert_eq!(path.to_string_lossy().trim_start_matches('/'), "backups/2024/file.enc");
    }

    #[test]
    fn test_default_config() {
        let config = StorageConfig::default();
        assert_eq!(config.multipart_threshold, Some(100 * 1024 * 1024));
    }
}
