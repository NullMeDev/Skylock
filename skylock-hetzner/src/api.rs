use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use skylock_core::{Result, SkylockError, StorageErrorType};
use std::time::Duration;

const API_BASE_URL: &str = "https://api.hetzner.cloud/v1";

#[derive(Debug, Clone)]
pub struct HetznerClient {
    client: Client,
    token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StorageBox {
    id: u64,
    name: String,
    size: u64,
    location: String,
    status: String,
}

#[derive(Debug, Serialize)]
pub struct CreateStorageBoxRequest {
    name: String,
    size: u64,
    location: String,
}

impl HetznerClient {
    pub fn new(token: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| SkylockError::Storage(StorageErrorType::ConnectionFailed(e.to_string())))?;

        Ok(Self { client, token })
    }

    pub async fn get_storage_boxes(&self) -> Result<Vec<StorageBox>> {
        let response = self.client
            .get(&format!("{}/storage_boxes", API_BASE_URL))
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|_| SkylockError::Storage(StorageErrorType::NetworkTimeout))?;

        match response.status() {
            StatusCode::OK => {
                let boxes: Vec<StorageBox> = response.json().await
                    .map_err(|_| SkylockError::Storage(StorageErrorType::ReadError))?;
                Ok(boxes)
            }
            StatusCode::UNAUTHORIZED => Err(SkylockError::Storage(StorageErrorType::AuthenticationFailed)),
            StatusCode::NOT_FOUND => Err(SkylockError::Storage(StorageErrorType::StorageBoxUnavailable)),
            StatusCode::TOO_MANY_REQUESTS => {
                // Get retry-after header if available
                let _retry_after = response.headers()
                    .get("retry-after")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(60);
                Err(SkylockError::Storage(StorageErrorType::RateLimitExceeded))
            },
            _status => {
                let _error_body = response.text().await
                    .unwrap_or_else(|_| String::from("Could not read error response"));
                Err(SkylockError::Storage(StorageErrorType::NetworkTimeout))
            }
        }
    }

    pub async fn create_storage_box(&self, request: CreateStorageBoxRequest) -> Result<StorageBox> {
        let response = self.client
            .post(&format!("{}/storage_boxes", API_BASE_URL))
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&request)
            .send()
            .await
            .map_err(|_| SkylockError::Storage(StorageErrorType::NetworkTimeout))?;

        match response.status() {
            StatusCode::CREATED => {
                let storage_box = response.json().await
                    .map_err(|_| SkylockError::Storage(StorageErrorType::ReadError))?;
                Ok(storage_box)
            }
            StatusCode::UNAUTHORIZED => Err(SkylockError::Storage(StorageErrorType::AuthenticationFailed)),
            StatusCode::PAYMENT_REQUIRED => Err(SkylockError::Storage(StorageErrorType::QuotaExceeded)),
            _ => Err(SkylockError::Storage(StorageErrorType::NetworkTimeout)),
        }
    }

    pub async fn delete_storage_box(&self, id: u64) -> Result<()> {
        let response = self.client
            .delete(&format!("{}/storage_boxes/{}", API_BASE_URL, id))
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|_| SkylockError::Storage(StorageErrorType::NetworkTimeout))?;

        match response.status() {
            StatusCode::NO_CONTENT => Ok(()),
            StatusCode::UNAUTHORIZED => Err(SkylockError::Storage(StorageErrorType::AuthenticationFailed)),
            StatusCode::NOT_FOUND => Err(SkylockError::Storage(StorageErrorType::StorageBoxUnavailable)),
            _ => Err(SkylockError::Storage(StorageErrorType::NetworkTimeout)),
        }
    }

    pub async fn get_storage_box_credentials(&self, id: u64) -> Result<StorageBoxCredentials> {
        let response = self.client
            .get(&format!("{}/storage_boxes/{}/access", API_BASE_URL, id))
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|_| SkylockError::Storage(StorageErrorType::NetworkTimeout))?;

        match response.status() {
            StatusCode::OK => {
                let creds = response.json().await
                    .map_err(|_| SkylockError::Storage(StorageErrorType::ReadError))?;
                Ok(creds)
            }
            StatusCode::UNAUTHORIZED => Err(SkylockError::Storage(StorageErrorType::AuthenticationFailed)),
            StatusCode::NOT_FOUND => Err(SkylockError::Storage(StorageErrorType::StorageBoxUnavailable)),
            _ => Err(SkylockError::Storage(StorageErrorType::NetworkTimeout)),
        }
    }

    pub async fn update_storage_box(&self, id: u64, size: u64) -> Result<StorageBox> {
        let response = self.client
            .put(&format!("{}/storage_boxes/{}", API_BASE_URL, id))
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&serde_json::json!({ "size": size }))
            .send()
            .await
            .map_err(|_| SkylockError::Storage(StorageErrorType::NetworkTimeout))?;

        match response.status() {
            StatusCode::OK => {
                let storage_box = response.json().await
                    .map_err(|_| SkylockError::Storage(StorageErrorType::ReadError))?;
                Ok(storage_box)
            }
            StatusCode::UNAUTHORIZED => Err(SkylockError::Storage(StorageErrorType::AuthenticationFailed)),
            StatusCode::NOT_FOUND => Err(SkylockError::Storage(StorageErrorType::StorageBoxUnavailable)),
            StatusCode::PAYMENT_REQUIRED => Err(SkylockError::Storage(StorageErrorType::QuotaExceeded)),
            _ => Err(SkylockError::Storage(StorageErrorType::NetworkTimeout)),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StorageBoxCredentials {
    username: String,
    password: String,
    host: String,
    ports: StorageBoxPorts,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StorageBoxPorts {
    sftp: u16,
    webdav: u16,
}
