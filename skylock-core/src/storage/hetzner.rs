use std::path::{Path, PathBuf};
use async_trait::async_trait;
use tokio::io::AsyncRead;
use reqwest;
use crate::error::{Result, Error, ErrorCategory, StorageErrorType, ErrorSeverity};
use super::{StorageProvider, StorageConfig, StorageItem, UploadOptions, DownloadOptions};
use super::webdav::WebDavClient;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: u64,
    pub status: String,
    pub size: u64,
    pub created_at: String,
}

pub struct HetznerStorageProvider {
    webdav: WebDavClient,
    root_path: PathBuf,
    quota_bytes: u64,
}

#[async_trait]
impl StorageProvider for HetznerStorageProvider {
    async fn create_directory(&self, path: &Path) -> Result<()> {
        let full_path = self.resolve_path(path);
        self.webdav.create_directory(&full_path).await
    }

    async fn list_directory(&self, path: &Path) -> Result<Vec<StorageItem>> {
        let full_path = self.resolve_path(path);
        self.webdav.list_directory(&full_path).await
    }

    async fn upload<'a>(&self, path: &Path, content: Box<dyn AsyncRead + Send + Unpin + 'a>, options: UploadOptions) -> Result<()> {
        let full_path = self.resolve_path(path);
        self.webdav.upload_file(&full_path, content, options).await
    }

    async fn download<'a>(&self, path: &Path, options: DownloadOptions) -> Result<Box<dyn AsyncRead + Send + Unpin + 'a>> {
        let full_path = self.resolve_path(path);
        self.webdav.download_file(&full_path, options).await
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        let full_path = self.resolve_path(path);
        self.webdav.delete(&full_path).await
    }

    async fn get_quota(&self) -> Result<(u64, u64)> {
        // TODO: Implement actual quota checking via Hetzner API
        Ok((0, self.quota_bytes))
    }
}

impl HetznerStorageProvider {
    pub fn new(config: &StorageConfig) -> Result<Self> {
        let base_url = format!(
            "https://u{}-sub{}.your-storagebox.de/",
            config.box_id.ok_or_else(|| Error::new(
                ErrorCategory::Storage(Some(StorageErrorType::InvalidSubaccount)),
                ErrorSeverity::High,
                "Missing Hetzner box ID".to_string(),
                "hetzner_storage".to_string(),
            ))?,
            config.subaccount_id.ok_or_else(|| Error::new(
                ErrorCategory::Storage(Some(StorageErrorType::InvalidSubaccount)),
                ErrorSeverity::High,
                "Missing Hetzner subaccount ID".to_string(),
                "hetzner_storage".to_string(),
            ))?,
        );

        let username = format!("u{}-sub{}",
            config.box_id.unwrap(),
            config.subaccount_id.unwrap()
        );

        let password = config.api_token.clone().ok_or_else(|| Error::new(
            ErrorCategory::Storage(Some(StorageErrorType::AuthenticationFailed)),
            ErrorSeverity::High,
            "Missing Hetzner API token".to_string(),
            "hetzner_storage".to_string(),
        ))?;

        let webdav = WebDavClient::new(&base_url, username, password)?;

        Ok(Self {
            webdav,
            root_path: PathBuf::from("backup"),
            quota_bytes: 1024 * 1024 * 1024 * 1024, // 1TB default
        })
    }

    fn resolve_path(&self, path: &Path) -> PathBuf {
        self.root_path.join(path)
    }

#[tracing::instrument(level = "debug", skip(self))]
pub async fn get_snapshot(&self) -> Result<Snapshot> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://robot-ws.your-server.de/storagebox")
        .header("Authorization", format!("Bearer {}", self.webdav.get_token()))
        .send()
        .await?;

    let snapshot: Snapshot = response.error_for_status()?.json().await?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_hetzner_path_resolution() {
        let config = StorageConfig {
            box_id: Some(123),
            subaccount_id: Some(1),
            api_token: Some("token".to_string()),
            ..Default::default()
        };

        let storage = HetznerStorageProvider::new(&config).unwrap();
        let path = storage.resolve_path(Path::new("test/file.txt"));
        assert_eq!(path, PathBuf::from("backup/test/file.txt"));
    }

    // TODO: Add more tests with mocked WebDAV server
}

#[async_trait]
impl StorageProvider for HetznerStorageProvider {
    async fn list_directory(&self, path: &Path) -> Result<Vec<StorageItem>> {
        let full_path = self.resolve_path(path);
        let entries = self.webdav.list_directory(&full_path).await?;

        Ok(entries.into_iter().map(|entry| StorageItem {
            path: entry.path,
            size: entry.size,
            modified: Some(entry.modified),
            is_directory: entry.is_dir,
        }).collect())
    }

    async fn upload_file<R: AsyncRead + Send + Unpin + 'static>(
        &self,
        path: &Path,
        content: R,
        size: u64,
        _options: Option<UploadOptions>,
    ) -> Result<()> {
        let full_path = self.resolve_path(path);
        self.webdav.upload_file(&full_path, content, size).await
    }

    async fn download_file(
        &self,
        path: &Path,
        _options: Option<DownloadOptions>,
    ) -> Result<Pin<Box<dyn AsyncRead + Send>>> {
        let full_path = self.resolve_path(path);
        let response = self.webdav.download_file(&full_path).await?;
        Ok(Box::pin(response.bytes_stream().map(|result| {
            result.map_err(|e| std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read response: {}", e),
            ))
        })))
    }

    async fn delete(&self, path: &Path) -> Result<()> {
        let full_path = self.resolve_path(path);
        self.webdav.delete(&full_path).await
    }

    async fn create_directory(&self, path: &Path) -> Result<()> {
        let full_path = self.resolve_path(path);
        self.webdav.create_directory(&full_path).await
    }

    async fn get_quota(&self) -> Result<(u64, u64)> {
        // TODO: Implement actual quota checking via Hetzner API
        Ok((0, self.quota_bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn test_hetzner_path_resolution() {
        let config = StorageConfig {
            box_id: Some(123),
            subaccount_id: Some(1),
            api_token: Some("token".to_string()),
            ..Default::default()
        };

        let storage = HetznerStorageProvider::new(&config).unwrap();
        let path = storage.resolve_path(Path::new("test/file.txt"));
        assert_eq!(path, PathBuf::from("backup/test/file.txt"));
    }

    // TODO: Add more tests with mocked WebDAV server
}

    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn get_snapshot(&self) -> Result<Snapshot> {
        let client = reqwest::Client::new();
        let response = client
            .get("https://robot-ws.your-server.de/storagebox")
            .header("Authorization", format!("Bearer {}", self.webdav.get_token()))
            .send()
            .await?;

        let snapshot: Snapshot = response.error_for_status()?.json().await?;
        Ok(snapshot)
    }

    async fn list_snapshots(&self) -> Result<Vec<Snapshot>> {
        let url = format!("{}/snapshots", self.base_url);

        let response = self.client
            .get(&url)
            .send()
            .await?;

        let snapshots: Vec<Snapshot> = response.error_for_status()?.json().await?;
        Ok(snapshots)
    }

    async fn get_snapshot(&self, snapshot_id: u64) -> Result<Option<Snapshot>> {
        let url = format!("{}/snapshots/{}", self.base_url, snapshot_id);

        let response = self.client
            .get(&url)
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let snapshot: Snapshot = response.error_for_status()?.json().await?;
        Ok(Some(snapshot))
    }

    async fn delete_snapshot(&self, snapshot_id: u64) -> Result<()> {
        let url = format!("{}/snapshots/{}", self.base_url, snapshot_id);

        self.client
            .delete(&url)
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }

    async fn rollback_snapshot(&self, snapshot_id: u64) -> Result<()> {
        let url = format!("{}/snapshots/{}/rollback", self.base_url, snapshot_id);

        self.client
            .post(&url)
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }
}

impl StorageManager {
    pub async fn new(config: StorageConfig) -> Result<Self> {
        let mut headers = header::HeaderMap::new();
        let mut username = String::new();
        let mut password = String::new();
        let mut base_url = String::new();

        // Prefer API token auth if available
        if let Some(api_token) = &config.api_token {
            headers.insert(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(&format!("Bearer {}", api_token))?,
            );

            base_url = format!(
                "https://robot-storage.hetzner.com/storage/{}/files",
                config.box_id.ok_or_else(|| crate::error::SkylockError::Storage(
                    "box_id is required when using API token authentication".to_string()
                ))?
            );
        } else if let Some(connection_string) = &config.connection_string {
            // Parse connection string for basic auth
            let parts: Vec<&str> = connection_string.split(';').collect();

            for part in parts {
                let kv: Vec<&str> = part.split('=').collect();
                if kv.len() == 2 {
                    match kv[0] {
                        "username" => username = kv[1].to_string(),
                        "password" => password = kv[1].to_string(),
                        "url" => base_url = kv[1].to_string(),
                        _ => (),
                    }
                }
            }

            // Add basic auth header
            let auth = format!("{}:{}", username, password);
            let auth_header = format!("Basic {}", encode(auth));
            headers.insert(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(&auth_header)?,
            );
        } else {
            return Err(crate::error::SkylockError::Storage(
                "Either api_token or connection_string must be provided".to_string()
            ));
        }

        // Add common headers
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );

        // Create HTTP client with retry logic
        let client = Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            client,
            base_url,
            username,
            password,
        })
    }

    fn get_url(&self, path: &PathBuf) -> String {
        format!("{}/{}", self.base_url, path.to_string_lossy())
    }
}

#[async_trait]
impl StorageProvider for HetznerStorageProvider {
    async fn upload(
        &self,
        mut source: Pin<Box<dyn AsyncRead + Send>>,
        destination: &PathBuf,
        options: Option<UploadOptions>,
    ) -> Result<StorageItem> {
        let url = self.get_url(destination);
        let chunk_size = options
            .and_then(|opt| opt.chunk_size)
            .unwrap_or(8192);

        // Create body stream
        let mut buffer = vec![0u8; chunk_size];
        let mut body = Vec::new();
        loop {
            let n = source.as_mut().read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&buffer[..n]);
        }

        // Upload file
        let response = self.client
            .put(&url)
            .body(body)
            .send()
            .await?;

        response.error_for_status()?;

        // Get file metadata
        self.get_metadata(destination).await?
            .ok_or_else(|| crate::error::SkylockError::Storage(
                "Failed to get uploaded file metadata".to_string()
            ))
    }

    async fn download(
        &self,
        source: &PathBuf,
        mut destination: Pin<Box<dyn AsyncWrite + Send>>,
        options: Option<DownloadOptions>,
    ) -> Result<()> {
        let url = self.get_url(source);

        let mut response = self.client
            .get(&url)
            .send()
            .await?;

        response.error_for_status()?;

        while let Some(chunk) = response.chunk().await? {
            destination.as_mut().write_all(&chunk).await?;
        }

        Ok(())
    }

    async fn delete(&self, path: &PathBuf) -> Result<()> {
        let url = self.get_url(path);

        let response = self.client
            .delete(&url)
            .send()
            .await?;

        response.error_for_status()?;

        Ok(())
    }

    async fn list(
        &self,
        prefix: Option<&PathBuf>,
        recursive: bool,
    ) -> Result<Vec<StorageItem>> {
        let mut url = self.base_url.clone();

        if let Some(prefix) = prefix {
            url = format!("{}/{}", url, prefix.to_string_lossy());
        }

        if recursive {
            url = format!("{}?recursive=true", url);
        }

        let response = self.client
            .get(&url)
            .send()
            .await?;

        response.error_for_status()?;

        let list: HetznerListResponse = response.json().await?;

        let items = list.entries
            .into_iter()
            .filter(|entry| entry.entry_type == "file")
            .map(|entry| StorageItem {
                path: PathBuf::from(entry.name),
                size: entry.size,
                modified: chrono::DateTime::parse_from_rfc3339(&entry.mtime)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                etag: None,
                metadata: None,
            })
            .collect();

        Ok(items)
    }

    async fn get_metadata(&self, path: &PathBuf) -> Result<Option<StorageItem>> {
        let url = self.get_url(path);

        let response = self.client
            .head(&url)
            .send()
            .await?;

        if response.status().is_success() {
            let size = response
                .headers()
                .get(header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);

            let modified = response
                .headers()
                .get(header::LAST_MODIFIED)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| chrono::DateTime::parse_from_rfc2822(v).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|| chrono::Utc::now());

            Ok(Some(StorageItem {
                path: path.clone(),
                size,
                modified,
                etag: None,
                metadata: None,
            }))
        } else {
            Ok(None)
        }
    }

    async fn copy(
        &self,
        source: &PathBuf,
        destination: &PathBuf,
    ) -> Result<StorageItem> {
        let source_url = self.get_url(source);
        let dest_url = self.get_url(destination);

        let response = self.client
            .request(reqwest::Method::from_bytes(b"COPY")?, &source_url)
            .header("Destination", dest_url)
            .send()
            .await?;

        response.error_for_status()?;

        self.get_metadata(destination).await?
            .ok_or_else(|| crate::error::SkylockError::Storage(
                "Failed to get copied file metadata".to_string()
            ))
    }
}
