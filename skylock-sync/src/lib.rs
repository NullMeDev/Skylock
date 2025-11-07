use reqwest::Client;
use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use url::Url;

mod error;
pub use error::{Error, Result};

#[derive(Debug)]
pub struct SyncthingClient {
    client: Client,
    api_url: Url,
    api_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FolderStatus {
    pub id: String,
    pub label: String,
    pub path: String,
    pub state: String,
}

impl SyncthingClient {
    pub fn new(api_url: &str, api_key: &str) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|_| SkylockError::from(SyncErrorType::ServiceFailure))?;

        let api_url = Url::parse(api_url)
            .map_err(|_| SkylockError::from(SyncErrorType::InvalidConfig))?;

        Ok(Self {
            client,
            api_url,
            api_key: api_key.to_string(),
        })
    }

    pub async fn get_folders(&self) -> Result<Vec<FolderStatus>> {
        let url = self.api_url.join("/rest/config/folders")
            .map_err(|_| SkylockError::from(SyncErrorType::InvalidConfig))?;

        let response = self.client
            .get(url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await
            .map_err(|e| SkylockError::Syncthing(format!("Network error: {}", e)))?;

        if !response.status().is_success() {
            return Err(SyncErrorType::ServiceFailure.into());
        }

        response.json().await
            .map_err(|_| SyncErrorType::ServiceFailure.into())
    }

    pub async fn add_folder(&self, path: PathBuf, label: &str) -> Result<()> {
        let url = self.api_url.join("/rest/config/folders")
            .map_err(|_| SkylockError::from(SyncErrorType::InvalidConfig))?;

        let folder_id = path.to_string_lossy().replace(':', "").replace('\\', "-");

        #[derive(Serialize)]
        struct FolderConfig {
            id: String,
            label: String,
            path: String,
            #[serde(rename = "type")]
            folder_type: String,
            #[serde(rename = "fsWatcherEnabled")]
            fs_watcher_enabled: bool,
            #[serde(rename = "fsWatcherDelayS")]
            fs_watcher_delay_s: i32,
        }

        let config = FolderConfig {
            id: folder_id,
            label: label.to_string(),
            path: path.to_string_lossy().to_string(),
            folder_type: "sendreceive".to_string(),
            fs_watcher_enabled: true,
            fs_watcher_delay_s: 10,
        };

        let response = self.client
            .post(url)
            .header("X-API-Key", &self.api_key)
            .json(&config)
            .send()
            .await
            .map_err(|e| SkylockError::Syncthing(format!("Failed to add folder: {}", e)))?;

        if !response.status().is_success() {
            return Err(SyncErrorType::ServiceFailure.into());
        }

        Ok(())
    }

    pub async fn scan_folder(&self, folder_id: &str) -> Result<()> {
        let url = self.api_url.join(&format!("/rest/db/scan?folder={}", folder_id))
            .map_err(|_| SkylockError::from(SyncErrorType::InvalidConfig))?;

        let response = self.client
            .post(url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await
            .map_err(|e| SkylockError::Syncthing(format!("Failed to scan folder: {}", e)))?;

        if !response.status().is_success() {
            return Err(SkylockError::Syncthing(
                format!("Failed to scan folder: {}", response.status())
            ));
        }

        Ok(())
    }
    pub async fn get_events(&self, since: Option<i64>) -> Result<Vec<SyncthingEvent>> {
        let mut url = self.api_url.join("/rest/events")?;

        if let Some(since) = since {
            url.set_query(Some(&format!("since={}", since)));
        }

        let response = self.client
            .get(url)
            .header("X-API-Key", &self.api_key)
            .send()
            .await
            .map_err(|e| SkylockError::Syncthing(format!("Failed to get events: {}", e)))?;

        if !response.status().is_success() {
            return Err(SkylockError::Syncthing(
                format!("Failed to get events: {}", response.status())
            ));
        }

        response.json().await
            .map_err(|e| SkylockError::Syncthing(format!("Failed to parse events: {}", e)))
    }
}

#[derive(Debug, Deserialize)]
pub struct SyncthingEvent {
    pub id: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    pub time: String,
    pub data: serde_json::Value,
}
