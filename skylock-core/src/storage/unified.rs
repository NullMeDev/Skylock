//! Unified Storage Abstraction
//!
//! Provides a unified interface for all storage providers, enabling
//! transparent switching between different cloud storage backends.

use crate::storage::{
    StorageBackend, StorageConfig, StorageItem, UploadOptions, DownloadOptions, 
    StorageProviderType, LocalStorageProvider, HetznerStorageProvider,
};
use crate::{Result, SkylockError};
use crate::error_types::StorageErrorType;
use async_trait::async_trait;
use std::path::PathBuf;
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{info, debug, error};
use std::sync::Arc;

#[cfg(feature = "aws-storage")]
use crate::storage::AWSStorageProvider;

#[cfg(feature = "backblaze-storage")]
use crate::storage::BackblazeStorageProvider;

/// Builder for creating a unified storage client
#[derive(Debug, Default)]
pub struct UnifiedStorageBuilder {
    config: Option<StorageConfig>,
    fallback_providers: Vec<StorageConfig>,
    retry_attempts: u32,
    retry_delay_ms: u64,
}

impl UnifiedStorageBuilder {
    pub fn new() -> Self {
        Self {
            config: None,
            fallback_providers: Vec::new(),
            retry_attempts: 3,
            retry_delay_ms: 1000,
        }
    }

    /// Set the primary storage configuration
    pub fn with_config(mut self, config: StorageConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Add a fallback provider (used if primary fails)
    pub fn with_fallback(mut self, config: StorageConfig) -> Self {
        self.fallback_providers.push(config);
        self
    }

    /// Set the number of retry attempts for failed operations
    pub fn with_retry_attempts(mut self, attempts: u32) -> Self {
        self.retry_attempts = attempts;
        self
    }

    /// Set the delay between retry attempts in milliseconds
    pub fn with_retry_delay(mut self, delay_ms: u64) -> Self {
        self.retry_delay_ms = delay_ms;
        self
    }

    /// Build the unified storage client
    pub async fn build(self) -> Result<UnifiedStorage> {
        let config = self.config.ok_or_else(|| {
            SkylockError::Storage(StorageErrorType::ConfigError)
        })?;

        let primary = create_provider(&config).await?;
        
        let mut fallbacks = Vec::new();
        for fb_config in self.fallback_providers {
            match create_provider(&fb_config).await {
                Ok(provider) => fallbacks.push(provider),
                Err(e) => {
                    error!("Failed to create fallback provider: {}", e);
                }
            }
        }

        Ok(UnifiedStorage {
            primary,
            fallbacks,
            retry_attempts: self.retry_attempts,
            retry_delay_ms: self.retry_delay_ms,
        })
    }
}

/// Create a storage provider from configuration
async fn create_provider(config: &StorageConfig) -> Result<Arc<dyn StorageBackend + Send + Sync>> {
    match config.provider {
        StorageProviderType::Local => {
            let provider = LocalStorageProvider::new(config)?;
            Ok(Arc::new(provider) as Arc<dyn StorageBackend + Send + Sync>)
        }
        
        StorageProviderType::Hetzner => {
            let provider = HetznerStorageProvider::new(config).await?;
            Ok(Arc::new(provider) as Arc<dyn StorageBackend + Send + Sync>)
        }
        
        #[cfg(feature = "aws-storage")]
        StorageProviderType::AWS => {
            let provider = AWSStorageProvider::new(config).await?;
            Ok(Arc::new(provider) as Arc<dyn StorageBackend + Send + Sync>)
        }
        
        #[cfg(feature = "aws-storage")]
        StorageProviderType::S3Compatible => {
            // S3-compatible providers use the same AWS SDK
            let provider = AWSStorageProvider::new(config).await?;
            Ok(Arc::new(provider) as Arc<dyn StorageBackend + Send + Sync>)
        }
        
        #[cfg(feature = "backblaze-storage")]
        StorageProviderType::Backblaze => {
            let provider = BackblazeStorageProvider::new(config).await?;
            Ok(Arc::new(provider) as Arc<dyn StorageBackend + Send + Sync>)
        }
        
        // Placeholder for future providers - not yet implemented
        #[cfg(feature = "azure-storage")]
        StorageProviderType::Azure => {
            Err(SkylockError::Storage(StorageErrorType::ConfigError))
        }
        #[cfg(feature = "gcp-storage")]
        StorageProviderType::Gcp => {
            Err(SkylockError::Storage(StorageErrorType::ConfigError))
        }
        
        #[allow(unreachable_patterns)]
        _ => Err(SkylockError::Storage(StorageErrorType::ConfigError)),
    }
}

/// Unified storage client that abstracts over different storage providers
pub struct UnifiedStorage {
    primary: Arc<dyn StorageBackend + Send + Sync>,
    fallbacks: Vec<Arc<dyn StorageBackend + Send + Sync>>,
    retry_attempts: u32,
    retry_delay_ms: u64,
}

impl std::fmt::Debug for UnifiedStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnifiedStorage")
            .field("fallbacks_count", &self.fallbacks.len())
            .field("retry_attempts", &self.retry_attempts)
            .field("retry_delay_ms", &self.retry_delay_ms)
            .finish()
    }
}

impl UnifiedStorage {
    /// Create a builder for UnifiedStorage
    pub fn builder() -> UnifiedStorageBuilder {
        UnifiedStorageBuilder::new()
    }

    /// Create a simple unified storage with just one provider
    pub async fn new(config: StorageConfig) -> Result<Self> {
        Self::builder().with_config(config).build().await
    }

    /// Execute an operation with retries and fallback support
    async fn with_retry_and_fallback<F, Fut, T>(&self, operation: F) -> Result<T>
    where
        F: Fn(Arc<dyn StorageBackend + Send + Sync>) -> Fut + Clone,
        Fut: std::future::Future<Output = Result<T>>,
    {
        // Try primary provider with retries
        for attempt in 0..self.retry_attempts {
            match operation(self.primary.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if attempt < self.retry_attempts - 1 {
                        debug!("Primary provider failed (attempt {}), retrying: {}", attempt + 1, e);
                        tokio::time::sleep(tokio::time::Duration::from_millis(self.retry_delay_ms)).await;
                    } else {
                        error!("Primary provider failed after {} attempts: {}", self.retry_attempts, e);
                    }
                }
            }
        }

        // Try fallback providers
        for (idx, fallback) in self.fallbacks.iter().enumerate() {
            for attempt in 0..self.retry_attempts {
                match operation(fallback.clone()).await {
                    Ok(result) => {
                        info!("Fallback provider {} succeeded", idx + 1);
                        return Ok(result);
                    }
                    Err(e) => {
                        if attempt < self.retry_attempts - 1 {
                            debug!("Fallback {} failed (attempt {}), retrying: {}", idx + 1, attempt + 1, e);
                            tokio::time::sleep(tokio::time::Duration::from_millis(self.retry_delay_ms)).await;
                        }
                    }
                }
            }
        }

        Err(SkylockError::Storage(StorageErrorType::ConnectionFailed(
            "All storage providers failed".to_string()
        )))
    }

    /// Get the number of available providers (primary + fallbacks)
    pub fn provider_count(&self) -> usize {
        1 + self.fallbacks.len()
    }
}

#[async_trait]
impl StorageBackend for UnifiedStorage {
    async fn upload(
        &self,
        source: Pin<Box<dyn AsyncRead + Send>>,
        destination: &PathBuf,
        options: Option<UploadOptions>,
    ) -> Result<StorageItem> {
        // For upload, we need to buffer the data since we might retry
        use tokio::io::AsyncReadExt;
        let mut data = Vec::new();
        let mut source = source;
        source.read_to_end(&mut data).await?;

        let dest = destination.clone();
        let opts = options.clone();
        
        self.with_retry_and_fallback(move |provider| {
            let data_clone = data.clone();
            let dest_clone = dest.clone();
            let opts_clone = opts.clone();
            async move {
                let cursor = std::io::Cursor::new(data_clone);
                let reader = Box::pin(tokio::io::BufReader::new(cursor));
                provider.upload(reader, &dest_clone, opts_clone).await
            }
        }).await
    }

    async fn download(
        &self,
        source: &PathBuf,
        destination: Pin<Box<dyn AsyncWrite + Send>>,
        options: Option<DownloadOptions>,
    ) -> Result<()> {
        use tokio::io::AsyncWriteExt;
        
        // For download, we buffer to memory first to support retries
        let src = source.clone();
        let opts = options.clone();
        
        let data = self.with_retry_and_fallback(move |provider| {
            let src_clone = src.clone();
            let opts_clone = opts.clone();
            async move {
                let buffer = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
                let buffer_clone = buffer.clone();
                
                // Create a custom async writer that writes to our buffer
                struct BufferWriter(std::sync::Arc<tokio::sync::Mutex<Vec<u8>>>);
                
                impl tokio::io::AsyncWrite for BufferWriter {
                    fn poll_write(
                        self: Pin<&mut Self>,
                        _cx: &mut std::task::Context<'_>,
                        buf: &[u8],
                    ) -> std::task::Poll<std::io::Result<usize>> {
                        // Use try_lock since we're in a sync context within poll
                        if let Ok(mut guard) = self.0.try_lock() {
                            guard.extend_from_slice(buf);
                            std::task::Poll::Ready(Ok(buf.len()))
                        } else {
                            std::task::Poll::Pending
                        }
                    }
                    
                    fn poll_flush(
                        self: Pin<&mut Self>,
                        _cx: &mut std::task::Context<'_>,
                    ) -> std::task::Poll<std::io::Result<()>> {
                        std::task::Poll::Ready(Ok(()))
                    }
                    
                    fn poll_shutdown(
                        self: Pin<&mut Self>,
                        _cx: &mut std::task::Context<'_>,
                    ) -> std::task::Poll<std::io::Result<()>> {
                        std::task::Poll::Ready(Ok(()))
                    }
                }
                
                let writer: Pin<Box<dyn AsyncWrite + Send>> = Box::pin(BufferWriter(buffer_clone));
                provider.download(&src_clone, writer, opts_clone).await?;
                
                let result = buffer.lock().await.clone();
                Ok(result)
            }
        }).await?;

        // Write to actual destination
        let mut dest = destination;
        dest.write_all(&data).await?;
        dest.flush().await?;
        
        Ok(())
    }

    async fn delete(&self, path: &PathBuf) -> Result<()> {
        let p = path.clone();
        self.with_retry_and_fallback(move |provider| {
            let path_clone = p.clone();
            async move { provider.delete(&path_clone).await }
        }).await
    }

    async fn list(&self, prefix: Option<&PathBuf>, recursive: bool) -> Result<Vec<StorageItem>> {
        let p = prefix.cloned();
        self.with_retry_and_fallback(move |provider| {
            let prefix_clone = p.clone();
            async move { provider.list(prefix_clone.as_ref(), recursive).await }
        }).await
    }

    async fn get_metadata(&self, path: &PathBuf) -> Result<Option<StorageItem>> {
        let p = path.clone();
        self.with_retry_and_fallback(move |provider| {
            let path_clone = p.clone();
            async move { provider.get_metadata(&path_clone).await }
        }).await
    }

    async fn copy(&self, source: &PathBuf, destination: &PathBuf) -> Result<StorageItem> {
        let src = source.clone();
        let dest = destination.clone();
        self.with_retry_and_fallback(move |provider| {
            let src_clone = src.clone();
            let dest_clone = dest.clone();
            async move { provider.copy(&src_clone, &dest_clone).await }
        }).await
    }
}

/// Helper functions for common storage operations
pub mod helpers {
    use super::*;
    use std::path::Path;
    use tokio::fs::File;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};

    /// Upload a file from the local filesystem
    pub async fn upload_file(
        storage: &UnifiedStorage,
        local_path: &Path,
        remote_path: &PathBuf,
    ) -> Result<StorageItem> {
        let file = File::open(local_path).await
            .map_err(|e| SkylockError::Storage(StorageErrorType::ReadError))?;
        let reader: Pin<Box<dyn AsyncRead + Send>> = Box::pin(BufReader::new(file));
        storage.upload(reader, remote_path, None).await
    }

    /// Download a file to the local filesystem
    pub async fn download_file(
        storage: &UnifiedStorage,
        remote_path: &PathBuf,
        local_path: &Path,
    ) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))?;
        }

        let file = File::create(local_path).await
            .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))?;
        let writer: Pin<Box<dyn AsyncWrite + Send>> = Box::pin(BufWriter::new(file));
        storage.download(remote_path, writer, None).await
    }

    /// Upload data from memory
    pub async fn upload_bytes(
        storage: &UnifiedStorage,
        data: Vec<u8>,
        remote_path: &PathBuf,
        content_type: Option<String>,
    ) -> Result<StorageItem> {
        let cursor = std::io::Cursor::new(data);
        let reader: Pin<Box<dyn AsyncRead + Send>> = Box::pin(cursor);
        let options = content_type.map(|ct| UploadOptions {
            content_type: Some(ct),
            ..Default::default()
        });
        storage.upload(reader, remote_path, options).await
    }

    /// Download data to memory
    pub async fn download_bytes(
        storage: &UnifiedStorage,
        remote_path: &PathBuf,
    ) -> Result<Vec<u8>> {
        use tokio::io::AsyncWriteExt;
        
        let buffer = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let buffer_clone = buffer.clone();
        
        // Create a custom async writer that writes to our buffer
        struct BufferWriter(std::sync::Arc<tokio::sync::Mutex<Vec<u8>>>);
        
        impl tokio::io::AsyncWrite for BufferWriter {
            fn poll_write(
                self: Pin<&mut Self>,
                _cx: &mut std::task::Context<'_>,
                buf: &[u8],
            ) -> std::task::Poll<std::io::Result<usize>> {
                if let Ok(mut guard) = self.0.try_lock() {
                    guard.extend_from_slice(buf);
                    std::task::Poll::Ready(Ok(buf.len()))
                } else {
                    std::task::Poll::Pending
                }
            }
            
            fn poll_flush(
                self: Pin<&mut Self>,
                _cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                std::task::Poll::Ready(Ok(()))
            }
            
            fn poll_shutdown(
                self: Pin<&mut Self>,
                _cx: &mut std::task::Context<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                std::task::Poll::Ready(Ok(()))
            }
        }
        
        let writer: Pin<Box<dyn AsyncWrite + Send>> = Box::pin(BufferWriter(buffer_clone));
        storage.download(remote_path, writer, None).await?;
        
        let result = buffer.lock().await.clone();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_builder_requires_config() {
        let result = UnifiedStorageBuilder::new().build().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_local_provider_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            provider: StorageProviderType::Local,
            connection_string: Some(temp_dir.path().to_string_lossy().to_string()),
            ..Default::default()
        };

        let storage = UnifiedStorage::new(config).await;
        assert!(storage.is_ok());
        assert_eq!(storage.unwrap().provider_count(), 1);
    }

    #[tokio::test]
    async fn test_builder_with_retry_settings() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            provider: StorageProviderType::Local,
            connection_string: Some(temp_dir.path().to_string_lossy().to_string()),
            ..Default::default()
        };

        let storage = UnifiedStorage::builder()
            .with_config(config)
            .with_retry_attempts(5)
            .with_retry_delay(2000)
            .build()
            .await
            .unwrap();

        assert_eq!(storage.retry_attempts, 5);
        assert_eq!(storage.retry_delay_ms, 2000);
    }
}
