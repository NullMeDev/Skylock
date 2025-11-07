use async_trait::async_trait;
use reqwest::{Client, Response, StatusCode};
use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use tokio::io::AsyncRead;
use url::Url;
use crate::error::{Result, Error, ErrorCategory, StorageErrorType, ErrorSeverity};

#[derive(Debug, Clone)]
pub struct WebDavClient {
    client: Client,
    base_url: Url,
    username: String,
    password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WebDavFile {
    pub path: PathBuf,
    pub size: u64,
    pub modified: chrono::DateTime<chrono::Utc>,
    pub is_dir: bool,
}

#[async_trait]
impl WebDavClient {
    pub fn new(base_url: &str, username: String, password: String) -> Result<Self> {
        let base_url = Url::parse(base_url).map_err(|e| Error::new(
            ErrorCategory::Storage(Some(StorageErrorType::InvalidSubaccount)),
            ErrorSeverity::High,
            format!("Invalid WebDAV URL: {}", e),
            "webdav_client".to_string(),
        ))?;

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| Error::new(
                ErrorCategory::Storage(Some(StorageErrorType::StorageBoxUnavailable)),
                ErrorSeverity::High,
                format!("Failed to create WebDAV client: {}", e),
                "webdav_client".to_string(),
            ))?;

        Ok(Self {
            client,
            base_url,
            username,
            password,
        })
    }

    pub async fn list_directory(&self, path: &Path) -> Result<Vec<WebDavFile>> {
        let url = self.build_url(path)?;
        let response = self.client
            .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), url)
            .basic_auth(&self.username, Some(&self.password))
            .header("Depth", "1")
            .send()
            .await
            .map_err(|e| Error::new(
                ErrorCategory::Storage(Some(StorageErrorType::StorageBoxUnavailable)),
                ErrorSeverity::High,
                format!("Failed to list directory: {}", e),
                "webdav_client".to_string(),
            ))?;

        self.handle_response(response).await?;

        // TODO: Parse WebDAV PROPFIND response
        // For now, return empty vec
        Ok(Vec::new())
    }

    pub async fn create_directory(&self, path: &Path) -> Result<()> {
        let url = self.build_url(path)?;
        let response = self.client
            .request(reqwest::Method::from_bytes(b"MKCOL").unwrap(), url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .map_err(|e| Error::new(
                ErrorCategory::Storage(Some(StorageErrorType::StorageBoxUnavailable)),
                ErrorSeverity::High,
                format!("Failed to create directory: {}", e),
                "webdav_client".to_string(),
            ))?;

        self.handle_response(response).await
    }

    pub async fn upload_file<R>(&self, path: &Path, content: R, size: u64) -> Result<()>
    where
        R: AsyncRead + Send + Unpin,
    {
        let url = self.build_url(path)?;
        let response = self.client
            .put(url)
            .basic_auth(&self.username, Some(&self.password))
            .body(reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(content)))
            .header("Content-Length", size.to_string())
            .send()
            .await
            .map_err(|e| Error::new(
                ErrorCategory::Storage(Some(StorageErrorType::StorageBoxUnavailable)),
                ErrorSeverity::High,
                format!("Failed to upload file: {}", e),
                "webdav_client".to_string(),
            ))?;

        self.handle_response(response).await
    }

    pub async fn download_file(&self, path: &Path) -> Result<Response> {
        let url = self.build_url(path)?;
        let response = self.client
            .get(url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .map_err(|e| Error::new(
                ErrorCategory::Storage(Some(StorageErrorType::StorageBoxUnavailable)),
                ErrorSeverity::High,
                format!("Failed to download file: {}", e),
                "webdav_client".to_string(),
            ))?;

        Ok(response)
    }

    pub async fn delete(&self, path: &Path) -> Result<()> {
        let url = self.build_url(path)?;
        let response = self.client
            .delete(url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .map_err(|e| Error::new(
                ErrorCategory::Storage(Some(StorageErrorType::StorageBoxUnavailable)),
                ErrorSeverity::High,
                format!("Failed to delete resource: {}", e),
                "webdav_client".to_string(),
            ))?;

        self.handle_response(response).await
    }

    fn build_url(&self, path: &Path) -> Result<Url> {
        self.base_url.join(path.to_str().ok_or_else(|| Error::new(
            ErrorCategory::Storage(Some(StorageErrorType::StorageBoxUnavailable)),
            ErrorSeverity::High,
            "Invalid path".to_string(),
            "webdav_client".to_string(),
        ))?).map_err(|e| Error::new(
            ErrorCategory::Storage(Some(StorageErrorType::StorageBoxUnavailable)),
            ErrorSeverity::High,
            format!("Failed to build URL: {}", e),
            "webdav_client".to_string(),
        ))
    }

    async fn handle_response(&self, response: Response) -> Result<()> {
        match response.status() {
            StatusCode::OK | StatusCode::CREATED | StatusCode::NO_CONTENT => Ok(()),
            StatusCode::UNAUTHORIZED => Err(Error::new(
                ErrorCategory::Storage(Some(StorageErrorType::AuthenticationFailed)),
                ErrorSeverity::High,
                "Authentication failed".to_string(),
                "webdav_client".to_string(),
            )),
            StatusCode::FORBIDDEN => Err(Error::new(
                ErrorCategory::Storage(Some(StorageErrorType::AccessDenied)),
                ErrorSeverity::High,
                "Access denied".to_string(),
                "webdav_client".to_string(),
            )),
            StatusCode::NOT_FOUND => Err(Error::new(
                ErrorCategory::Storage(Some(StorageErrorType::StorageBoxUnavailable)),
                ErrorSeverity::High,
                "Resource not found".to_string(),
                "webdav_client".to_string(),
            )),
            StatusCode::INSUFFICIENT_STORAGE => Err(Error::new(
                ErrorCategory::Storage(Some(StorageErrorType::QuotaExceeded)),
                ErrorSeverity::High,
                "Storage quota exceeded".to_string(),
                "webdav_client".to_string(),
            )),
            _ => Err(Error::new(
                ErrorCategory::Storage(Some(StorageErrorType::StorageBoxUnavailable)),
                ErrorSeverity::High,
                format!("Unexpected status code: {}", response.status()),
                "webdav_client".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;
    use std::io::Cursor;

    #[test]
    async fn test_webdav_client_creation() {
        let client = WebDavClient::new(
            "https://example.com/dav/",
            "user".to_string(),
            "pass".to_string(),
        ).unwrap();

        assert_eq!(client.base_url.as_str(), "https://example.com/dav/");
        assert_eq!(client.username, "user");
        assert_eq!(client.password, "pass");
    }

    #[test]
    async fn test_invalid_url() {
        let result = WebDavClient::new(
            "not a url",
            "user".to_string(),
            "pass".to_string(),
        );

        assert!(result.is_err());
    }

    #[test]
    async fn test_url_building() {
        let client = WebDavClient::new(
            "https://example.com/dav/",
            "user".to_string(),
            "pass".to_string(),
        ).unwrap();

        let url = client.build_url(Path::new("test/file.txt")).unwrap();
        assert_eq!(url.as_str(), "https://example.com/dav/test/file.txt");
    }
}
