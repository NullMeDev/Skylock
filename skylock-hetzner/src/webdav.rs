use std::path::Path;
use std::collections::HashMap;
use anyhow::{Result, anyhow};
use reqwest::{Client, Method, Response};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, CONTENT_LENGTH};
use base64::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::{info, debug, warn, error};
use url::Url;
use indicatif::ProgressBar;
use tokio::io::AsyncReadExt;

#[derive(Debug, Clone)]
pub struct WebDAVConfig {
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub base_path: String,
}

#[derive(Debug, Clone)]
pub struct HetznerWebDAVClient {
    client: Client,
    config: WebDAVConfig,
    auth_header: HeaderValue,
}

#[derive(Debug, Deserialize)]
struct PropfindResponse {
    #[serde(rename = "multistatus")]
    multistatus: MultiStatus,
}

#[derive(Debug, Deserialize)]
struct MultiStatus {
    #[serde(rename = "response")]
    responses: Vec<DavResponse>,
}

#[derive(Debug, Deserialize)]
struct DavResponse {
    href: String,
    propstat: PropStat,
}

#[derive(Debug, Deserialize)]
struct PropStat {
    prop: DavProperties,
    status: String,
}

#[derive(Debug, Deserialize)]
struct DavProperties {
    #[serde(rename = "resourcetype")]
    resource_type: Option<ResourceType>,
    #[serde(rename = "getcontentlength")]
    content_length: Option<String>,
    #[serde(rename = "getlastmodified")]
    last_modified: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ResourceType {
    collection: Option<String>,
}

impl HetznerWebDAVClient {
    pub fn new(config: WebDAVConfig) -> Result<Self> {
        // Create HTTP client with TLS support (rustls)
        let client = Client::builder()
            .use_rustls_tls()
            .timeout(std::time::Duration::from_secs(300)) // 5 minutes for large uploads
            .build()?;

        // Create Basic Auth header
        let credentials = format!("{}:{}", config.username, config.password);
        let encoded = BASE64_STANDARD.encode(credentials.as_bytes());
        let auth_header = HeaderValue::from_str(&format!("Basic {}", encoded))?;

        Ok(Self {
            client,
            config,
            auth_header,
        })
    }

    fn build_url(&self, path: &str) -> Result<Url> {
        let clean_path = path.trim_start_matches('/');
        
        let full_path = if self.config.base_path == "/" || self.config.base_path.is_empty() {
            // For root base_path, just use the clean path
            if clean_path.is_empty() {
                "/".to_string()
            } else {
                format!("/{}", clean_path)
            }
        } else {
            // For non-root base_path, combine them
            if clean_path.is_empty() {
                self.config.base_path.clone()
            } else {
                format!("{}/{}", self.config.base_path.trim_end_matches('/'), clean_path)
            }
        };

        let base_url = self.config.base_url.trim_end_matches('/');
        let full_url = format!("{}{}", base_url, full_path);
        
        Url::parse(&full_url)
            .map_err(|e| anyhow!("Failed to build URL: {}", e))
    }

    pub async fn test_connection(&self) -> Result<()> {
        let url = self.build_url("/")?;
        info!("Testing WebDAV connection to {}", url);
        
        // Use HEAD request instead of PROPFIND for simple connectivity test
        let response = self.client
            .head(url.clone())
            .header(AUTHORIZATION, &self.auth_header)
            .send()
            .await?;

        let status = response.status();
        debug!("WebDAV HEAD response status: {} for URL: {}", status, url);
        
        if status.is_success() {
            info!("WebDAV connection successful");
            Ok(())
        } else {
            error!("WebDAV connection failed: {}", status);
            Err(anyhow!("Connection failed with status: {}", status))
        }
    }

    pub async fn create_directory(&self, path: &str) -> Result<()> {
        debug!("Creating directory: {}", path);
        
        let url = self.build_url(path)?;
        let response = self.client
            .request(Method::from_bytes(b"MKCOL")?, url)
            .header(AUTHORIZATION, &self.auth_header)
            .send()
            .await?;

        if response.status().is_success() || response.status().as_u16() == 405 {
            // 405 Method Not Allowed usually means directory already exists
            debug!("Directory created or already exists: {}", path);
            Ok(())
        } else {
            warn!("Failed to create directory {}: {}", path, response.status());
            Err(anyhow!("Failed to create directory: {}", response.status()))
        }
    }

    pub async fn upload_file(&self, local_path: &Path, remote_path: &str) -> Result<()> {
        self.upload_file_with_progress(local_path, remote_path, None).await
    }

    pub async fn upload_file_with_progress(&self, local_path: &Path, remote_path: &str, progress: Option<ProgressBar>) -> Result<()> {
        info!("Uploading {} to {}", local_path.display(), remote_path);
        
        let url = self.build_url(remote_path)?;
        info!("Full upload URL: {}", url);
        let file = tokio::fs::File::open(local_path).await?;
        let file_size = file.metadata().await?.len();
        
        // Set progress bar to show it's starting
        if let Some(pb) = &progress {
            pb.set_position(0);
            pb.set_message(format!("📤 Uploading..."));
        }
        
        // Read entire file into memory (reqwest needs the full body)
        // This is fast and not the bottleneck - network upload is
        let buffer = tokio::fs::read(local_path).await?;
        
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, self.auth_header.clone());
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
        headers.insert(CONTENT_LENGTH, HeaderValue::from_str(&file_size.to_string())?);

        // Now do the actual upload (this is where the time is spent)
        let response = self.client
            .put(url)
            .headers(headers)
            .body(buffer)
            .send()
            .await?;

        // Mark as complete after successful upload
        if let Some(pb) = &progress {
            pb.set_position(file_size);
        }

        if response.status().is_success() {
            info!("Successfully uploaded {}", remote_path);
            Ok(())
        } else {
            let status = response.status();
            error!("Upload failed for {}: {}", remote_path, status);
            let error_body = response.text().await.unwrap_or_default();
            Err(anyhow!("Upload failed: {} - {}", status, error_body))
        }
    }

    pub async fn download_file(&self, remote_path: &str, local_path: &Path) -> Result<()> {
        info!("Downloading {} to {}", remote_path, local_path.display());
        
        let url = self.build_url(remote_path)?;
        let response = self.client
            .get(url)
            .header(AUTHORIZATION, &self.auth_header)
            .send()
            .await?;

        if response.status().is_success() {
            let content = response.bytes().await?;
            
            // Ensure local directory exists
            if let Some(parent) = local_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            
            tokio::fs::write(local_path, content).await?;
            info!("Successfully downloaded {}", remote_path);
            Ok(())
        } else {
            error!("Download failed for {}: {}", remote_path, response.status());
            Err(anyhow!("Download failed: {}", response.status()))
        }
    }

    pub async fn delete_file(&self, remote_path: &str) -> Result<()> {
        debug!("Deleting {}", remote_path);
        
        let url = self.build_url(remote_path)?;
        let response = self.client
            .delete(url)
            .header(AUTHORIZATION, &self.auth_header)
            .send()
            .await?;

        if response.status().is_success() {
            debug!("Successfully deleted {}", remote_path);
            Ok(())
        } else {
            warn!("Delete failed for {}: {}", remote_path, response.status());
            Err(anyhow!("Delete failed: {}", response.status()))
        }
    }

    pub async fn list_files(&self, path: &str) -> Result<Vec<String>> {
        debug!("Listing files in {}", path);
        
        let url = self.build_url(path)?;
        
        let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
    <D:prop>
        <D:resourcetype/>
        <D:getcontentlength/>
        <D:getlastmodified/>
    </D:prop>
</D:propfind>"#;

        let response = self.client
            .request(Method::from_bytes(b"PROPFIND")?, url)
            .header(AUTHORIZATION, &self.auth_header)
            .header("Depth", "1")
            .header(CONTENT_TYPE, "text/xml; charset=utf-8")
            .body(propfind_body)
            .send()
            .await?;

        if response.status().is_success() {
            let body = response.text().await?;
            self.parse_propfind_response(&body)
        } else {
            error!("List files failed for {}: {}", path, response.status());
            Err(anyhow!("List files failed: {}", response.status()))
        }
    }

    fn parse_propfind_response(&self, xml: &str) -> Result<Vec<String>> {
        // For now, use a simple XML parsing approach
        // In production, you might want to use a proper XML parser like quick-xml
        let mut files = Vec::new();
        
        // Extract href values from XML
        let lines: Vec<&str> = xml.lines().collect();
        let mut in_response = false;
        let mut current_href: Option<String> = None;
        let mut is_collection = false;
        
        for line in lines {
            let line = line.trim();
            
            if line.starts_with("<D:response") {
                in_response = true;
                current_href = None;
                is_collection = false;
            } else if line.starts_with("</D:response>") {
                if let Some(ref href) = current_href {
                    // Only include files, not directories
                    if !is_collection {
                        // Clean up the href path
                        let clean_path = href.trim_start_matches(&self.config.base_path)
                            .trim_start_matches('/')
                            .to_string();
                        if !clean_path.is_empty() && clean_path != "." {
                            files.push(clean_path);
                        }
                    }
                }
                in_response = false;
            } else if in_response && line.starts_with("<D:href>") && line.ends_with("</D:href>") {
                let href = line
                    .strip_prefix("<D:href>")
                    .and_then(|s| s.strip_suffix("</D:href>"))
                    .unwrap_or("");
                current_href = Some(href.to_string());
            } else if in_response && line.contains("<D:collection/>") {
                is_collection = true;
            }
        }
        
        debug!("Found {} files in directory", files.len());
        Ok(files)
    }

    /// List directories (collections) in a path
    pub async fn list_directories(&self, path: &str) -> Result<Vec<String>> {
        debug!("Listing directories in {}", path);
        
        let url = self.build_url(path)?;
        
        let propfind_body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:">
    <D:prop>
        <D:resourcetype/>
        <D:getcontentlength/>
        <D:getlastmodified/>
    </D:prop>
</D:propfind>"#;

        let response = self.client
            .request(Method::from_bytes(b"PROPFIND")?, url)
            .header(AUTHORIZATION, &self.auth_header)
            .header("Depth", "1")
            .header(CONTENT_TYPE, "text/xml; charset=utf-8")
            .body(propfind_body)
            .send()
            .await?;

        if response.status().is_success() {
            let body = response.text().await?;
            self.parse_propfind_directories(&body)
        } else {
            error!("List directories failed for {}: {}", path, response.status());
            Err(anyhow!("List directories failed: {}", response.status()))
        }
    }
    
    fn parse_propfind_directories(&self, xml: &str) -> Result<Vec<String>> {
        let mut directories = Vec::new();
        
        // Extract href values from XML - but only for collections
        let lines: Vec<&str> = xml.lines().collect();
        let mut in_response = false;
        let mut current_href: Option<String> = None;
        let mut is_collection = false;
        
        for line in lines {
            let line = line.trim();
            
            if line.starts_with("<D:response") {
                in_response = true;
                current_href = None;
                is_collection = false;
            } else if line.starts_with("</D:response>") {
                if let Some(ref href) = current_href {
                    // Only include directories (collections)
                    if is_collection {
                        // Clean up the href path
                        let clean_path = href.trim_start_matches(&self.config.base_path)
                            .trim_start_matches('/')
                            .trim_end_matches('/')
                            .to_string();
                        // Skip the parent directory itself
                        if !clean_path.is_empty() && clean_path != "." && !clean_path.ends_with("/skylock/backups") {
                            directories.push(clean_path);
                        }
                    }
                }
                in_response = false;
            } else if in_response && line.starts_with("<D:href>") && line.ends_with("</D:href>") {
                let href = line
                    .strip_prefix("<D:href>")
                    .and_then(|s| s.strip_suffix("</D:href>"))
                    .unwrap_or("");
                current_href = Some(href.to_string());
            } else if in_response && line.contains("<D:collection/>") {
                is_collection = true;
            }
        }
        
        debug!("Found {} directories", directories.len());
        Ok(directories)
    }

    pub async fn file_exists(&self, remote_path: &str) -> Result<bool> {
        let url = self.build_url(remote_path)?;
        let response = self.client
            .head(url)
            .header(AUTHORIZATION, &self.auth_header)
            .send()
            .await?;

        Ok(response.status().is_success())
    }

    pub async fn get_file_size(&self, remote_path: &str) -> Result<Option<u64>> {
        let url = self.build_url(remote_path)?;
        let response = self.client
            .head(url)
            .header(AUTHORIZATION, &self.auth_header)
            .send()
            .await?;

        if response.status().is_success() {
            if let Some(content_length) = response.headers().get("content-length") {
                if let Ok(length_str) = content_length.to_str() {
                    if let Ok(length) = length_str.parse::<u64>() {
                        return Ok(Some(length));
                    }
                }
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_webdav_client_creation() {
        let config = WebDAVConfig {
            base_url: "https://example.com/dav".to_string(),
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            base_path: "/backup".to_string(),
        };

        let client = HetznerWebDAVClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_url_building() {
        let config = WebDAVConfig {
            base_url: "https://uXXXXXX.your-storagebox.de".to_string(),
            username: "uXXXXXX".to_string(),
            password: "password".to_string(),
            base_path: "/backup/skylock".to_string(),
        };

        let client = HetznerWebDAVClient::new(config).unwrap();
        let url = client.build_url("test/file.txt").unwrap();
        assert_eq!(url.as_str(), "https://uXXXXXX.your-storagebox.de/backup/skylock/test/file.txt");
    }
}