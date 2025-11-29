//! Backblaze B2 Storage Provider
//!
//! Provides integration with Backblaze B2 cloud storage using the native B2 API.

use crate::storage::{StorageConfig, StorageItem, UploadOptions, DownloadOptions, StorageBackend};
use crate::{Result, SkylockError};
use crate::error_types::StorageErrorType;
use async_trait::async_trait;
use std::path::PathBuf;
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sha1::{Sha1, Digest};
use tracing::{info, debug, warn, error};
use std::sync::Arc;
use tokio::sync::RwLock;

const B2_API_URL: &str = "https://api.backblazeb2.com";
const B2_API_VERSION: &str = "b2api/v2";
const DEFAULT_LARGE_FILE_THRESHOLD: u64 = 100 * 1024 * 1024;
const MIN_PART_SIZE: u64 = 5 * 1024 * 1024;
const DEFAULT_PART_SIZE: u64 = 100 * 1024 * 1024;
const MAX_PART_SIZE: u64 = 5 * 1024 * 1024 * 1024;
const MAX_PARTS: usize = 10000;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2AuthResponse {
    account_id: String,
    authorization_token: String,
    api_url: String,
    download_url: String,
    #[serde(default)]
    allowed: B2Allowed,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2Allowed {
    bucket_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2Bucket {
    bucket_id: String,
    bucket_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2ListBucketsResponse {
    buckets: Vec<B2Bucket>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2UploadUrlResponse {
    upload_url: String,
    authorization_token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2FileInfo {
    file_id: String,
    content_length: u64,
    content_sha1: String,
    upload_timestamp: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2StartLargeFileResponse {
    file_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2UploadPartUrlResponse {
    upload_url: String,
    authorization_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2FinishLargeFileResponse {
    file_id: String,
    content_sha1: String,
    upload_timestamp: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2ListFilesResponse {
    files: Vec<B2FileVersion>,
    next_file_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct B2FileVersion {
    file_id: String,
    file_name: String,
    content_length: u64,
    content_sha1: String,
    #[serde(default)]
    file_info: std::collections::HashMap<String, String>,
    upload_timestamp: u64,
    action: String,
}

#[derive(Debug, Clone)]
struct B2Auth {
    auth_token: String,
    api_url: String,
    download_url: String,
    bucket_id: String,
    bucket_name: String,
    expires_at: DateTime<Utc>,
}

/// Backblaze B2 storage provider using native B2 API
#[derive(Debug)]
pub struct BackblazeStorageProvider {
    #[allow(dead_code)]
    config: StorageConfig,
    client: reqwest::Client,
    auth: Arc<RwLock<Option<B2Auth>>>,
    application_key_id: String,
    application_key: String,
    bucket_name: String,
    large_file_threshold: u64,
    part_size: u64,
}

impl BackblazeStorageProvider {
    pub async fn new(config: &StorageConfig) -> Result<Self> {
        let bucket_name = config.bucket_name.as_ref()
            .ok_or_else(|| SkylockError::Storage(StorageErrorType::ConfigError))?
            .clone();

        let application_key_id = config.access_key_id.as_ref()
            .ok_or_else(|| SkylockError::Storage(StorageErrorType::ConfigError))?
            .clone();

        let application_key = config.secret_access_key.as_ref()
            .ok_or_else(|| SkylockError::Storage(StorageErrorType::ConfigError))?
            .clone();

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| SkylockError::Storage(StorageErrorType::ConnectionFailed(e.to_string())))?;

        let large_file_threshold = config.multipart_threshold.unwrap_or(DEFAULT_LARGE_FILE_THRESHOLD);
        let mut part_size = config.multipart_part_size.unwrap_or(DEFAULT_PART_SIZE);

        if part_size < MIN_PART_SIZE {
            part_size = MIN_PART_SIZE;
        }
        if part_size > MAX_PART_SIZE {
            part_size = MAX_PART_SIZE;
        }

        let provider = Self {
            config: config.clone(),
            client,
            auth: Arc::new(RwLock::new(None)),
            application_key_id,
            application_key,
            bucket_name,
            large_file_threshold,
            part_size,
        };

        provider.authorize().await?;
        info!("Backblaze B2 provider initialized: bucket={}", provider.bucket_name);

        Ok(provider)
    }

    async fn authorize(&self) -> Result<B2Auth> {
        {
            let auth_guard = self.auth.read().await;
            if let Some(ref auth) = *auth_guard {
                if auth.expires_at > Utc::now() {
                    return Ok(auth.clone());
                }
            }
        }

        let auth_string = format!("{}:{}", self.application_key_id, self.application_key);
        let auth_header = format!("Basic {}", base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            auth_string.as_bytes()
        ));

        let response = self.client
            .get(format!("{}/{}/b2_authorize_account", B2_API_URL, B2_API_VERSION))
            .header("Authorization", &auth_header)
            .send()
            .await
            .map_err(|_| SkylockError::Storage(StorageErrorType::AuthenticationFailed))?;

        if !response.status().is_success() {
            return Err(SkylockError::Storage(StorageErrorType::AuthenticationFailed));
        }

        let auth_response: B2AuthResponse = response.json().await
            .map_err(|_| SkylockError::Storage(StorageErrorType::AuthenticationFailed))?;

        let bucket_id = self.get_bucket_id(&auth_response).await?;

        let auth = B2Auth {
            auth_token: auth_response.authorization_token,
            api_url: auth_response.api_url,
            download_url: auth_response.download_url,
            bucket_id,
            bucket_name: self.bucket_name.clone(),
            expires_at: Utc::now() + chrono::Duration::hours(23),
        };

        {
            let mut auth_guard = self.auth.write().await;
            *auth_guard = Some(auth.clone());
        }

        Ok(auth)
    }

    async fn get_bucket_id(&self, auth: &B2AuthResponse) -> Result<String> {
        if let Some(ref bucket_id) = auth.allowed.bucket_id {
            return Ok(bucket_id.clone());
        }

        let response = self.client
            .post(format!("{}/{}/b2_list_buckets", auth.api_url, B2_API_VERSION))
            .header("Authorization", &auth.authorization_token)
            .json(&serde_json::json!({"accountId": auth.account_id, "bucketName": self.bucket_name}))
            .send()
            .await
            .map_err(|_| SkylockError::Storage(StorageErrorType::ReadError))?;

        let list_response: B2ListBucketsResponse = response.json().await
            .map_err(|_| SkylockError::Storage(StorageErrorType::ReadError))?;

        list_response.buckets.into_iter()
            .find(|b| b.bucket_name == self.bucket_name)
            .map(|b| b.bucket_id)
            .ok_or_else(|| SkylockError::Storage(StorageErrorType::PathNotFound))
    }

    async fn get_upload_url(&self) -> Result<B2UploadUrlResponse> {
        let auth = self.authorize().await?;
        
        let response = self.client
            .post(format!("{}/{}/b2_get_upload_url", auth.api_url, B2_API_VERSION))
            .header("Authorization", &auth.auth_token)
            .json(&serde_json::json!({"bucketId": auth.bucket_id}))
            .send()
            .await
            .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))?;

        response.json().await
            .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))
    }

    fn path_to_filename(&self, path: &PathBuf) -> String {
        path.to_string_lossy().trim_start_matches('/').to_string()
    }

    fn sha1_hash(data: &[u8]) -> String {
        let mut hasher = Sha1::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    async fn upload_small_file(&self, data: Vec<u8>, filename: &str, content_type: Option<String>) -> Result<StorageItem> {
        let upload_url = self.get_upload_url().await?;
        let sha1 = Self::sha1_hash(&data);
        let size = data.len() as u64;

        let response = self.client
            .post(&upload_url.upload_url)
            .header("Authorization", &upload_url.authorization_token)
            .header("X-Bz-File-Name", urlencoding::encode(filename).as_ref())
            .header("Content-Type", content_type.as_deref().unwrap_or("application/octet-stream"))
            .header("Content-Length", size)
            .header("X-Bz-Content-Sha1", &sha1)
            .body(data)
            .send()
            .await
            .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))?;

        if !response.status().is_success() {
            return Err(SkylockError::Storage(StorageErrorType::WriteError));
        }

        let file_info: B2FileInfo = response.json().await
            .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))?;

        Ok(StorageItem {
            path: PathBuf::from("/").join(filename),
            size,
            last_modified: DateTime::from_timestamp_millis(file_info.upload_timestamp as i64),
            metadata: None,
            etag: Some(file_info.content_sha1),
        })
    }

    async fn upload_large_file(&self, data: Vec<u8>, filename: &str, content_type: Option<String>) -> Result<StorageItem> {
        let auth = self.authorize().await?;

        // Start large file
        let start_resp = self.client
            .post(format!("{}/{}/b2_start_large_file", auth.api_url, B2_API_VERSION))
            .header("Authorization", &auth.auth_token)
            .json(&serde_json::json!({
                "bucketId": auth.bucket_id,
                "fileName": filename,
                "contentType": content_type.as_deref().unwrap_or("application/octet-stream")
            }))
            .send()
            .await
            .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))?;

        let start_response: B2StartLargeFileResponse = start_resp.json().await
            .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))?;

        let file_id = start_response.file_id;
        let mut part_sha1_array: Vec<String> = Vec::new();
        let mut offset = 0usize;
        let mut part_number = 1u32;
        let total_size = data.len();

        // Upload parts
        while offset < total_size {
            let end = std::cmp::min(offset + self.part_size as usize, total_size);
            let part_data = data[offset..end].to_vec();
            let sha1 = Self::sha1_hash(&part_data);

            let part_url_resp = self.client
                .post(format!("{}/{}/b2_get_upload_part_url", auth.api_url, B2_API_VERSION))
                .header("Authorization", &auth.auth_token)
                .json(&serde_json::json!({"fileId": file_id}))
                .send()
                .await
                .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))?;

            let part_url: B2UploadPartUrlResponse = part_url_resp.json().await
                .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))?;

            let upload_resp = self.client
                .post(&part_url.upload_url)
                .header("Authorization", &part_url.authorization_token)
                .header("X-Bz-Part-Number", part_number)
                .header("Content-Length", part_data.len())
                .header("X-Bz-Content-Sha1", &sha1)
                .body(part_data)
                .send()
                .await
                .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))?;

            if !upload_resp.status().is_success() {
                let _ = self.cancel_large_file(&file_id).await;
                return Err(SkylockError::Storage(StorageErrorType::WriteError));
            }

            part_sha1_array.push(sha1);
            offset = end;
            part_number += 1;

            if part_number > MAX_PARTS as u32 {
                let _ = self.cancel_large_file(&file_id).await;
                return Err(SkylockError::Storage(StorageErrorType::WriteError));
            }
        }

        // Finish large file
        let finish_resp = self.client
            .post(format!("{}/{}/b2_finish_large_file", auth.api_url, B2_API_VERSION))
            .header("Authorization", &auth.auth_token)
            .json(&serde_json::json!({"fileId": file_id, "partSha1Array": part_sha1_array}))
            .send()
            .await
            .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))?;

        let finish_response: B2FinishLargeFileResponse = finish_resp.json().await
            .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))?;

        info!("B2 large file upload complete: file_id={}", file_id);

        Ok(StorageItem {
            path: PathBuf::from("/").join(filename),
            size: total_size as u64,
            last_modified: DateTime::from_timestamp_millis(finish_response.upload_timestamp as i64),
            metadata: None,
            etag: Some(finish_response.content_sha1),
        })
    }

    async fn cancel_large_file(&self, file_id: &str) -> Result<()> {
        let auth = self.authorize().await?;
        warn!("Canceling large file upload: file_id={}", file_id);
        
        let _ = self.client
            .post(format!("{}/{}/b2_cancel_large_file", auth.api_url, B2_API_VERSION))
            .header("Authorization", &auth.auth_token)
            .json(&serde_json::json!({"fileId": file_id}))
            .send()
            .await;

        Ok(())
    }

    async fn get_file_info_by_name(&self, filename: &str) -> Result<Option<B2FileVersion>> {
        let auth = self.authorize().await?;

        let response = self.client
            .post(format!("{}/{}/b2_list_file_names", auth.api_url, B2_API_VERSION))
            .header("Authorization", &auth.auth_token)
            .json(&serde_json::json!({"bucketId": auth.bucket_id, "prefix": filename, "maxFileCount": 1}))
            .send()
            .await
            .map_err(|_| SkylockError::Storage(StorageErrorType::ReadError))?;

        let list_response: B2ListFilesResponse = response.json().await
            .map_err(|_| SkylockError::Storage(StorageErrorType::ReadError))?;

        Ok(list_response.files.into_iter()
            .find(|f| f.file_name == filename && f.action == "upload"))
    }
}

#[async_trait]
impl StorageBackend for BackblazeStorageProvider {
    async fn upload(
        &self,
        mut source: Pin<Box<dyn AsyncRead + Send>>,
        destination: &PathBuf,
        options: Option<UploadOptions>,
    ) -> Result<StorageItem> {
        let filename = self.path_to_filename(destination);
        let content_type = options.as_ref().and_then(|o| o.content_type.clone());

        let mut data = Vec::new();
        source.read_to_end(&mut data).await?;

        if data.len() as u64 <= self.large_file_threshold {
            debug!("B2 using standard upload: name={}, size={}", filename, data.len());
            self.upload_small_file(data, &filename, content_type).await
        } else {
            debug!("B2 using large file upload: name={}, size={}", filename, data.len());
            self.upload_large_file(data, &filename, content_type).await
        }
    }

    async fn download(
        &self,
        source: &PathBuf,
        mut destination: Pin<Box<dyn AsyncWrite + Send>>,
        options: Option<DownloadOptions>,
    ) -> Result<()> {
        let auth = self.authorize().await?;
        let filename = self.path_to_filename(source);
        let url = format!("{}/file/{}/{}", auth.download_url, 
            urlencoding::encode(&auth.bucket_name), urlencoding::encode(&filename));

        let mut request = self.client.get(&url).header("Authorization", &auth.auth_token);

        if let Some(opts) = options {
            if let Some((start, end)) = opts.range {
                request = request.header("Range", format!("bytes={}-{}", start, end - 1));
            }
        }

        let response = request.send().await
            .map_err(|_| SkylockError::Storage(StorageErrorType::ReadError))?;

        if !response.status().is_success() {
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Err(SkylockError::Storage(StorageErrorType::FileNotFound));
            }
            return Err(SkylockError::Storage(StorageErrorType::ReadError));
        }

        let bytes = response.bytes().await
            .map_err(|_| SkylockError::Storage(StorageErrorType::ReadError))?;
        destination.write_all(&bytes).await?;
        destination.flush().await?;

        Ok(())
    }

    async fn delete(&self, path: &PathBuf) -> Result<()> {
        let auth = self.authorize().await?;
        let filename = self.path_to_filename(path);

        let file_info = self.get_file_info_by_name(&filename).await?
            .ok_or_else(|| SkylockError::Storage(StorageErrorType::FileNotFound))?;

        let response = self.client
            .post(format!("{}/{}/b2_delete_file_version", auth.api_url, B2_API_VERSION))
            .header("Authorization", &auth.auth_token)
            .json(&serde_json::json!({"fileId": file_info.file_id, "fileName": filename}))
            .send()
            .await
            .map_err(|_| SkylockError::Storage(StorageErrorType::DeleteError))?;

        if !response.status().is_success() {
            return Err(SkylockError::Storage(StorageErrorType::DeleteError));
        }

        Ok(())
    }

    async fn list(&self, prefix: Option<&PathBuf>, _recursive: bool) -> Result<Vec<StorageItem>> {
        let auth = self.authorize().await?;
        let prefix_str = prefix.map(|p| self.path_to_filename(p)).unwrap_or_default();
        let mut items = Vec::new();
        let mut next_file_name: Option<String> = None;

        loop {
            let mut body = serde_json::json!({"bucketId": auth.bucket_id, "maxFileCount": 1000});
            if !prefix_str.is_empty() {
                body["prefix"] = serde_json::json!(prefix_str);
            }
            if let Some(ref name) = next_file_name {
                body["startFileName"] = serde_json::json!(name);
            }

            let response = self.client
                .post(format!("{}/{}/b2_list_file_names", auth.api_url, B2_API_VERSION))
                .header("Authorization", &auth.auth_token)
                .json(&body)
                .send()
                .await
                .map_err(|_| SkylockError::Storage(StorageErrorType::ReadError))?;

            let list_response: B2ListFilesResponse = response.json().await
                .map_err(|_| SkylockError::Storage(StorageErrorType::ReadError))?;

            for file in list_response.files {
                if file.action == "upload" {
                    items.push(StorageItem {
                        path: PathBuf::from("/").join(&file.file_name),
                        size: file.content_length,
                        last_modified: DateTime::from_timestamp_millis(file.upload_timestamp as i64),
                        metadata: Some(file.file_info),
                        etag: Some(file.content_sha1),
                    });
                }
            }

            next_file_name = list_response.next_file_name;
            if next_file_name.is_none() {
                break;
            }
        }

        Ok(items)
    }

    async fn get_metadata(&self, path: &PathBuf) -> Result<Option<StorageItem>> {
        let filename = self.path_to_filename(path);
        
        match self.get_file_info_by_name(&filename).await? {
            Some(file) => Ok(Some(StorageItem {
                path: path.clone(),
                size: file.content_length,
                last_modified: DateTime::from_timestamp_millis(file.upload_timestamp as i64),
                metadata: Some(file.file_info),
                etag: Some(file.content_sha1),
            })),
            None => Ok(None),
        }
    }

    async fn copy(&self, source: &PathBuf, destination: &PathBuf) -> Result<StorageItem> {
        let auth = self.authorize().await?;
        let source_filename = self.path_to_filename(source);
        let dest_filename = self.path_to_filename(destination);

        let source_file = self.get_file_info_by_name(&source_filename).await?
            .ok_or_else(|| SkylockError::Storage(StorageErrorType::FileNotFound))?;

        let response = self.client
            .post(format!("{}/{}/b2_copy_file", auth.api_url, B2_API_VERSION))
            .header("Authorization", &auth.auth_token)
            .json(&serde_json::json!({"sourceFileId": source_file.file_id, "fileName": dest_filename}))
            .send()
            .await
            .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))?;

        if !response.status().is_success() {
            return Err(SkylockError::Storage(StorageErrorType::WriteError));
        }

        let file_info: B2FileInfo = response.json().await
            .map_err(|_| SkylockError::Storage(StorageErrorType::WriteError))?;

        Ok(StorageItem {
            path: PathBuf::from("/").join(&dest_filename),
            size: file_info.content_length,
            last_modified: DateTime::from_timestamp_millis(file_info.upload_timestamp as i64),
            metadata: None,
            etag: Some(file_info.content_sha1),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha1_hash() {
        let data = b"Hello, World!";
        let hash = BackblazeStorageProvider::sha1_hash(data);
        assert_eq!(hash, "0a0a9f2a6772942557ab5355d76af442f8f65e01");
    }
}
