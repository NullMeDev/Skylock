mod sftp;
mod sftp_secure;
mod api;
mod webdav;

use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;
use sha2::{Sha256, Digest};
use skylock_core::{Result, StorageErrorType, SkylockError};
use tracing::{info, debug};
use base64::engine::general_purpose::STANDARD as base64_standard;
use base64::Engine;
use indicatif::ProgressBar;

pub use api::{StorageBox, CreateStorageBoxRequest, StorageBoxCredentials};
pub use sftp::SftpClient;
pub use sftp_secure::{SecureSftpClient, SecureSftpConfig, generate_ed25519_keypair};
pub use webdav::{HetznerWebDAVClient, WebDAVConfig};

#[allow(dead_code)]
const USER_AGENT: &str = "Skylock-Hybrid/1.0";

#[derive(Debug)]
pub enum StorageProtocol {
    WebDav,
    Sftp,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub api_token: String,
    pub storage_box_id: Option<u64>,
    pub location: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FileMetadata {
    pub path: PathBuf,
    pub size: u64,
    pub hash: String,
    pub last_modified: chrono::DateTime<chrono::Utc>,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct HetznerConfig {
    pub endpoint: String,
    pub username: String,
    pub password: String,
    pub api_token: String,
    pub encryption_key: String,
}

#[allow(dead_code)]
pub struct HetznerClient {
    webdav: HetznerWebDAVClient,
}

impl HetznerClient {
    pub fn new(config: HetznerConfig) -> Result<Self> {
        debug!("Creating HetznerClient with endpoint: {}, username: {}", 
            config.endpoint, config.username);
        debug!("Password length: {} chars", config.password.len());
        
        let webdav_config = WebDAVConfig {
            base_url: config.endpoint.clone(),
            username: config.username.clone(),
            password: config.password.clone(),
            base_path: "/".to_string(),
        };
        
        let webdav = HetznerWebDAVClient::new(webdav_config)
            .map_err(|e| {
                debug!("Failed to create WebDAV client: {}", e);
                SkylockError::Storage(StorageErrorType::StorageBoxUnavailable)
            })?;

        debug!("HetznerClient created successfully");
        Ok(Self {
            webdav,
        })
    }


    pub async fn upload_file(&self, local_path: &Path, remote_path: &Path) -> Result<FileMetadata> {
        self.upload_file_with_progress(local_path, remote_path, None).await
    }

    pub async fn upload_file_with_progress(&self, local_path: &Path, remote_path: &Path, progress: Option<ProgressBar>) -> Result<FileMetadata> {
        let file = tokio::fs::File::open(local_path).await?;
        let metadata = file.metadata().await?;
        let file_size = metadata.len();

        // Calculate file hash
        let mut hasher = Sha256::new();
        let mut buffer = Vec::new();
        tokio::fs::File::open(local_path).await?.read_to_end(&mut buffer).await?;
        hasher.update(&buffer);
        let hash = base64_standard.encode(hasher.finalize());

        let remote_path_str = remote_path.to_string_lossy().into_owned();
        debug!("Uploading file to {}", remote_path_str);

        // Use WebDAV client for upload with progress
        self.webdav.upload_file_with_progress(local_path, &remote_path_str, progress)
            .await
            .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;

        Ok(FileMetadata {
            path: remote_path.to_path_buf(),
            size: file_size,
            hash,
            last_modified: chrono::Utc::now(),
        })
    }

    pub async fn download_file(&self, remote_path: &Path, local_path: &Path) -> Result<FileMetadata> {
        let remote_path_str = remote_path.to_string_lossy().into_owned();
        info!("Downloading file from {}", remote_path_str);

        // Use WebDAV client for download
        self.webdav.download_file(&remote_path_str, local_path)
            .await
            .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;

        // Get file size and calculate hash
        let file = tokio::fs::File::open(local_path).await?;
        let metadata = file.metadata().await?;
        let file_size = metadata.len();

        let mut hasher = Sha256::new();
        let mut buffer = Vec::new();
        tokio::fs::File::open(local_path).await?.read_to_end(&mut buffer).await?;
        hasher.update(&buffer);
        let hash = base64_standard.encode(hasher.finalize());

        Ok(FileMetadata {
            path: remote_path.to_path_buf(),
            size: file_size,
            hash,
            last_modified: chrono::Utc::now(),
        })
    }

    pub async fn delete_file(&self, remote_path: &Path) -> Result<()> {
        let remote_path_str = remote_path.to_string_lossy().into_owned();
        info!("Deleting file {}", remote_path_str);

        self.webdav.delete_file(&remote_path_str)
            .await
            .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;
        Ok(())
    }

    pub async fn list_files(&self, prefix: &str) -> Result<Vec<FileMetadata>> {
        let file_names = self.webdav.list_files(prefix)
            .await
            .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;

        // Convert file names to FileMetadata
        let mut files = Vec::new();
        for name in file_names {
            files.push(FileMetadata {
                path: PathBuf::from(&name),
                size: 0, // WebDAV list doesn't return sizes easily
                hash: String::new(),
                last_modified: chrono::Utc::now(),
            });
        }

        Ok(files)
    }

    pub async fn create_directory(&self, path: &str) -> Result<()> {
        debug!("Creating directory: {}", path);
        self.webdav.create_directory(path)
            .await
            .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;
        Ok(())
    }

    pub async fn list_directories(&self, path: &str) -> Result<Vec<String>> {
        debug!("Listing directories in: {}", path);
        self.webdav.list_directories(path)
            .await
            .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))
    }

}
