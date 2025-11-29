use std::path::PathBuf;
use std::pin::Pin;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use tokio::io::{AsyncRead, AsyncWrite};
use chrono::{DateTime, Utc};
use crate::{
    Result, SkylockError,
    error_types::{Error, ErrorCategory, ErrorSeverity, StorageErrorType},
};

pub mod providers;
pub mod unified;

pub use providers::{
    LocalStorageProvider,
    HetznerStorageProvider,
};
pub use unified::{UnifiedStorage, UnifiedStorageBuilder, helpers};

#[cfg(feature = "aws-storage")]
pub use providers::AWSStorageProvider;
#[cfg(feature = "azure-storage")]
pub use providers::AzureStorageProvider;
#[cfg(feature = "gcp-storage")]
pub use providers::GCPStorageProvider;
#[cfg(feature = "backblaze-storage")]
pub use providers::BackblazeStorageProvider;
#[cfg(feature = "aws-storage")]
pub use providers::{S3CompatibleConfig, S3CompatibleProvider, ProviderInfo, list_providers};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum StorageProviderType {
    #[default]
    Local,
    Hetzner,
    #[cfg(feature = "aws-storage")]
    AWS,
    #[cfg(feature = "azure-storage")]
    Azure,
    #[cfg(feature = "gcp-storage")]
    GCP,
    #[cfg(feature = "backblaze-storage")]
    Backblaze,
    /// Generic S3-compatible storage (MinIO, Wasabi, DigitalOcean Spaces, etc.)
    #[cfg(feature = "aws-storage")]
    S3Compatible,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub provider: StorageProviderType,
    pub api_token: Option<String>,
    pub connection_string: Option<String>,
    pub box_id: Option<u64>,
    pub subaccount_id: Option<u64>,
    pub max_concurrent_uploads: usize,
    pub max_concurrent_downloads: usize,
    pub chunk_size: usize,
    pub retry_count: usize,
    pub retry_delay_ms: u64,
    
    // Cloud provider fields (AWS S3, Backblaze B2, etc.)
    /// S3 bucket name or B2 bucket name
    pub bucket_name: Option<String>,
    /// AWS region or B2 region (e.g., "us-east-1", "us-west-002")
    pub region: Option<String>,
    /// Custom endpoint URL for S3-compatible services
    pub endpoint: Option<String>,
    /// Access key ID (AWS) or Application Key ID (B2)
    pub access_key_id: Option<String>,
    /// Secret access key (AWS) or Application Key (B2)
    pub secret_access_key: Option<String>,
    /// B2 account ID (Backblaze specific)
    pub account_id: Option<String>,
    /// Server-side encryption type (e.g., "AES256", "aws:kms")
    pub server_side_encryption: Option<String>,
    /// KMS key ID for SSE-KMS encryption
    pub kms_key_id: Option<String>,
    /// Multipart upload threshold in bytes (default: 100MB)
    pub multipart_threshold: Option<u64>,
    /// Multipart part size in bytes (default: 10MB, min: 5MB)
    pub multipart_part_size: Option<u64>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            provider: StorageProviderType::default(),
            api_token: None,
            connection_string: None,
            box_id: None,
            subaccount_id: None,
            max_concurrent_uploads: 4,
            max_concurrent_downloads: 4,
            chunk_size: 10 * 1024 * 1024, // 10MB
            retry_count: 3,
            retry_delay_ms: 1000,
            bucket_name: None,
            region: None,
            endpoint: None,
            access_key_id: None,
            secret_access_key: None,
            account_id: None,
            server_side_encryption: None,
            kms_key_id: None,
            multipart_threshold: Some(100 * 1024 * 1024), // 100MB
            multipart_part_size: Some(10 * 1024 * 1024),  // 10MB
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageItem {
    pub path: PathBuf,
    pub size: u64,
    pub last_modified: Option<DateTime<Utc>>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
    pub etag: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UploadOptions {
    pub chunk_size: Option<usize>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DownloadOptions {
    pub chunk_size: Option<usize>,
    pub range: Option<(u64, u64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: u64,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub size: u64,
    pub size_filesystem: u64,
    pub is_automatic: bool,
}

#[async_trait]
pub trait StorageBackend: Send + Sync + std::fmt::Debug {
    async fn upload(
        &self,
        source: Pin<Box<dyn AsyncRead + Send>>,
        destination: &PathBuf,
        options: Option<UploadOptions>,
    ) -> Result<StorageItem>;

    async fn download(
        &self,
        source: &PathBuf,
        destination: Pin<Box<dyn AsyncWrite + Send>>,
        options: Option<DownloadOptions>,
    ) -> Result<()>;

    async fn delete(&self, path: &PathBuf) -> Result<()>;

    async fn list(
        &self,
        prefix: Option<&PathBuf>,
        recursive: bool,
    ) -> Result<Vec<StorageItem>>;

    async fn get_metadata(&self, path: &PathBuf) -> Result<Option<StorageItem>>;

    async fn copy(
        &self,
        source: &PathBuf,
        destination: &PathBuf,
    ) -> Result<StorageItem>;

    // Snapshot management
    async fn create_snapshot(&self, description: Option<String>) -> Result<Snapshot> {
        Err(Error::new(
            ErrorCategory::Storage(StorageErrorType::WriteError),
            ErrorSeverity::Medium,
            "Snapshots not supported by this provider".to_string(),
            "storage_backend".to_string(),
        ).into())
    }

    async fn list_snapshots(&self) -> Result<Vec<Snapshot>> {
        Err(Error::new(
            ErrorCategory::Storage(StorageErrorType::ReadError),
            ErrorSeverity::Medium,
            "Snapshots not supported by this provider".to_string(),
            "storage_backend".to_string(),
        ).into())
    }

    async fn get_snapshot(&self, snapshot_id: u64) -> Result<Option<Snapshot>> {
        Err(Error::new(
            ErrorCategory::Storage(StorageErrorType::PathNotFound),
            ErrorSeverity::Medium,
            "Snapshots not supported by this provider".to_string(),
            "storage_backend".to_string(),
        ).into())
    }

    async fn delete_snapshot(&self, snapshot_id: u64) -> Result<()> {
        Err(Error::new(
            ErrorCategory::Storage(StorageErrorType::DeleteError),
            ErrorSeverity::Medium,
            "Snapshots not supported by this provider".to_string(),
            "storage_backend".to_string(),
        ).into())
    }

    async fn rollback_snapshot(&self, snapshot_id: u64) -> Result<()> {
        Err(Error::new(
            ErrorCategory::Storage(StorageErrorType::WriteError),
            ErrorSeverity::Medium,
            "Snapshots not supported by this provider".to_string(),
            "storage_backend".to_string(),
        ).into())
    }
}

#[derive(Debug)]
pub struct StorageManager {
    config: StorageConfig,
    provider: Box<dyn StorageBackend>,
}

impl StorageManager {
    pub async fn new(config: StorageConfig) -> Result<Self> {
        let provider: Box<dyn StorageBackend> = match config.provider {
            StorageProviderType::Local => Box::new(LocalStorageProvider::new(&config)?),
            StorageProviderType::Hetzner => Box::new(HetznerStorageProvider::new(&config).await?),
            #[cfg(feature = "aws-storage")]
            StorageProviderType::AWS => Box::new(AWSStorageProvider::new(&config).await?),
            #[cfg(feature = "azure-storage")]
            StorageProviderType::Azure => Box::new(AzureStorageProvider::new(&config).await?),
            #[cfg(feature = "gcp-storage")]
            StorageProviderType::GCP => Box::new(GCPStorageProvider::new(&config).await?),
            #[cfg(feature = "backblaze-storage")]
            StorageProviderType::Backblaze => Box::new(BackblazeStorageProvider::new(&config).await?),
            #[cfg(feature = "aws-storage")]
            StorageProviderType::S3Compatible => {
                // S3-compatible providers use AWS SDK with custom endpoints
                Box::new(AWSStorageProvider::new(&config).await?)
            }
            _ => return Err(Error::new(
                ErrorCategory::Storage(StorageErrorType::PathNotFound),
                ErrorSeverity::High,
                "Storage provider not available".to_string(),
                "storage_manager".to_string(),
            ).into()),
        };

        Ok(Self { config, provider })
    }

    pub async fn upload_file(&self, local_path: &PathBuf, remote_path: &PathBuf) -> Result<StorageItem> {
        let file = tokio::fs::File::open(local_path).await?;
        let reader = Box::pin(file);

        self.provider.upload(reader, remote_path, None).await
    }

    pub async fn download_file(&self, remote_path: &PathBuf, local_path: &PathBuf) -> Result<()> {
        let file = tokio::fs::File::create(local_path).await?;
        let writer = Box::pin(file);

        self.provider.download(remote_path, writer, None).await
    }

    pub async fn delete_file(&self, path: &PathBuf) -> Result<()> {
        self.provider.delete(path).await
    }

    pub async fn list_files(&self, prefix: Option<&PathBuf>) -> Result<Vec<StorageItem>> {
        self.provider.list(prefix, true).await
    }

    pub async fn get_file_metadata(&self, path: &PathBuf) -> Result<Option<StorageItem>> {
        self.provider.get_metadata(path).await
    }

    pub async fn copy_file(&self, source: &PathBuf, destination: &PathBuf) -> Result<StorageItem> {
        self.provider.copy(source, destination).await
    }

    // Snapshot management methods
    pub async fn create_snapshot(&self, description: Option<String>) -> Result<Snapshot> {
        self.provider.create_snapshot(description).await
    }

    pub async fn list_snapshots(&self) -> Result<Vec<Snapshot>> {
        self.provider.list_snapshots().await
    }

    pub async fn get_snapshot(&self, snapshot_id: u64) -> Result<Option<Snapshot>> {
        self.provider.get_snapshot(snapshot_id).await
    }

    pub async fn delete_snapshot(&self, snapshot_id: u64) -> Result<()> {
        self.provider.delete_snapshot(snapshot_id).await
    }

    pub async fn rollback_snapshot(&self, snapshot_id: u64) -> Result<()> {
        self.provider.rollback_snapshot(snapshot_id).await
    }
}
