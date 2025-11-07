use async_trait::async_trait;
use crate::{
    Result, 
    error_types::{Error, ErrorCategory, ErrorSeverity, SecurityErrorType, NetworkErrorType, StorageErrorType},
};

#[cfg(feature = "aws-storage")]
use aws_sdk_s3::{Client as S3Client, Region};
#[cfg(feature = "aws-storage")]
use aws_config;

use std::path::Path;
use tokio::io::AsyncReadExt;

#[async_trait]
pub trait CloudStorageProvider: Send + Sync {
    async fn upload_backup(&self, backup_path: &Path, remote_path: &str) -> Result<()>;
    async fn download_backup(&self, remote_path: &str, local_path: &Path) -> Result<()>;
    async fn list_backups(&self, prefix: &str) -> Result<Vec<String>>;
    async fn delete_backup(&self, remote_path: &str) -> Result<()>;
}

#[cfg(feature = "aws-storage")]
pub struct S3StorageProvider {
    client: S3Client,
    bucket: String,
}

#[cfg(feature = "aws-storage")]
impl S3StorageProvider {
    pub async fn new(region: &str, bucket: &str) -> Result<Self> {
        let config = aws_config::from_env()
            .region(Region::new(region.to_string()))
            .load()
            .await;
        
        let client = S3Client::new(&config);
        
        Ok(Self {
            client,
            bucket: bucket.to_string(),
        })
    }
}

#[cfg(feature = "aws-storage")]
#[async_trait]
impl CloudStorageProvider for S3StorageProvider {
    async fn upload_backup(&self, backup_path: &Path, remote_path: &str) -> Result<()> {
        let mut file = tokio::fs::File::open(backup_path).await
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::AccessDenied),
                ErrorSeverity::High,
                "Failed to open backup file".to_string(),
                "s3_provider".to_string(),
            ))?;

        let mut contents = Vec::new();
        file.read_to_end(&mut contents).await
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::StorageError(e.to_string())),
                ErrorSeverity::High,
                "Failed to read backup file".to_string(),
                "s3_provider".to_string(),
            ))?;

        self.client.put_object()
            .bucket(&self.bucket)
            .key(remote_path)
            .body(contents.into())
            .send()
            .await
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::StorageError(e.to_string())),
                ErrorSeverity::High,
                "Failed to upload to S3".to_string(),
                "s3_provider".to_string(),
            ))?;

        Ok(())
    }

    async fn download_backup(&self, remote_path: &str, local_path: &Path) -> Result<()> {
        let output = self.client.get_object()
            .bucket(&self.bucket)
            .key(remote_path)
            .send()
            .await
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::StorageError(e.to_string())),
                ErrorSeverity::High,
                "Failed to download from S3".to_string(),
                "s3_provider".to_string(),
            ))?;

        let data = output.body.collect().await
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::StorageError(e.to_string())),
                ErrorSeverity::High,
                "Failed to collect S3 data".to_string(),
                "s3_provider".to_string(),
            ))?;

        tokio::fs::write(local_path, data.into_bytes())
            .await
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::StorageError(e.to_string())),
                ErrorSeverity::High,
                "Failed to write backup file".to_string(),
                "s3_provider".to_string(),
            ))?;

        Ok(())
    }

    async fn list_backups(&self, prefix: &str) -> Result<Vec<String>> {
        let output = self.client.list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix)
            .send()
            .await
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::StorageError(e.to_string())),
                ErrorSeverity::High,
                "Failed to list S3 objects".to_string(),
                "s3_provider".to_string(),
            ))?;

        let keys = output.contents()
            .unwrap_or_default()
            .iter()
            .filter_map(|obj| obj.key().map(String::from))
            .collect();

        Ok(keys)
    }

    async fn delete_backup(&self, remote_path: &str) -> Result<()> {
        self.client.delete_object()
            .bucket(&self.bucket)
            .key(remote_path)
            .send()
            .await
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::StorageError(e.to_string())),
                ErrorSeverity::High,
                "Failed to delete S3 object".to_string(),
                "s3_provider".to_string(),
            ))?;

        Ok(())
    }
}

// WebDAV provider can be implemented here when needed