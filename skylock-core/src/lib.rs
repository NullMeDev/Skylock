use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use thiserror::Error;

pub mod scheduler;
pub mod security;
pub mod encryption;
pub mod compression;
pub mod storage;
pub mod sync;
pub mod error_types;
pub mod audit;

// Re-export error types
pub use error_types::{Error, ErrorCategory, ErrorSeverity, SystemError};
pub use error_types::{
    SecurityErrorType, SystemErrorType, BackupErrorType,
    StorageErrorType, NetworkErrorType, SyncErrorType
};

// Re-export common types and traits
pub use security::{KeyType, EncryptionEngine, SecureKey};
pub use security::key_manager::{KeyManager, KeyRotationPolicy, KeyStatus, KeyMetadata};
pub use compression::{CompressionConfig, CompressionEngine, CompressionType};

#[derive(Debug, Error)]
pub enum SkylockError {
    #[error("Generic error: {0}")]
    Generic(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Syncthing error: {0}")]
    Syncthing(String),
    #[error("Storage error: {0}")]
    Storage(StorageErrorType),
    #[error("Network error: {0}")]
    Network(NetworkErrorType),
    #[error("Backup error: {0}")]
    Backup(BackupErrorType),
    #[error("Sync error: {0}")]
    Sync(SyncErrorType),
    #[error("System error: {0}")]
    System(SystemErrorType),
    #[error("Security error")]
    Security,
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Configuration error")]
    Configuration,
    #[error("Resource error")]
    Resource,
    #[error("General error: {0}")]
    Other(String),
}

impl From<Error> for SkylockError {
    fn from(err: Error) -> Self {
        match err.category {
            ErrorCategory::Security(_) => SkylockError::Security,
            ErrorCategory::System(e) => SkylockError::System(e),
            ErrorCategory::Network(e) => SkylockError::Generic(format!("Network error: {:?}", e)),
            ErrorCategory::Storage(e) => SkylockError::Storage(e),
            ErrorCategory::Sync(e) => SkylockError::Sync(e),
            ErrorCategory::Resource(_) => SkylockError::Resource,
            ErrorCategory::Config(_) => SkylockError::Configuration,
            ErrorCategory::Backup(e) => SkylockError::Backup(e),
        }
    }
}

impl From<SystemErrorType> for SkylockError {
    fn from(err: SystemErrorType) -> Self {
        SkylockError::System(err)
    }
}

impl From<serde_json::Error> for SkylockError {
    fn from(err: serde_json::Error) -> Self {
        SkylockError::Generic(format!("JSON error: {}", err))
    }
}

impl From<notify::Error> for SkylockError {
    fn from(err: notify::Error) -> Self {
        SkylockError::Generic(format!("File system notification error: {}", err))
    }
}

impl From<argon2::password_hash::Error> for SkylockError {
    fn from(err: argon2::password_hash::Error) -> Self {
        SkylockError::Generic(format!("Password hashing error: {}", err))
    }
}

impl From<aes_gcm::Error> for SkylockError {
    fn from(err: aes_gcm::Error) -> Self {
        SkylockError::Generic(format!("AES encryption error: {}", err))
    }
}

impl From<url::ParseError> for SkylockError {
    fn from(err: url::ParseError) -> Self {
        SkylockError::Other(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, SkylockError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub syncthing: SyncthingConfig,
    pub hetzner: HetznerConfig,
    pub backup: BackupConfig,
    pub ui: UiConfig,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
}

fn default_data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "skylock", "skylock-hybrid")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("./data"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncthingConfig {
    pub api_key: String,
    pub api_url: String,
    pub folders: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HetznerConfig {
    pub endpoint: String,
    pub username: String,
    pub password: String,
    pub encryption_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub vss_enabled: bool,
    pub schedule: String,
    pub retention_days: u32,
    pub backup_paths: Vec<PathBuf>,
    /// Maximum upload speed limit (e.g., "1.5M", "500K", "0" for unlimited)
    #[serde(default)]
    pub max_speed_limit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub always_prompt_deletions: bool,
    pub notification_enabled: bool,
}

impl Config {
    pub fn load(path: Option<PathBuf>) -> Result<Self> {
        let path = path.unwrap_or_else(|| {
            directories::ProjectDirs::from("com", "skylock", "skylock-hybrid")
                .map(|proj_dirs| proj_dirs.config_dir().join("config.toml"))
                .unwrap_or_else(|| PathBuf::from("config.toml"))
        });

        let config_str = std::fs::read_to_string(&path)
            .map_err(|e| SkylockError::Config(format!("Failed to read config file: {}", e)))?;

        toml::from_str(&config_str)
            .map_err(|e| SkylockError::Config(format!("Failed to parse config: {}", e)))
    }
    
    pub fn validate(&self) -> Result<()> {
        // Basic validation
        if self.syncthing.api_key.is_empty() {
            return Err(SkylockError::Config("Syncthing API key is required".to_string()));
        }
        
        if self.hetzner.username.is_empty() {
            return Err(SkylockError::Config("Hetzner username is required".to_string()));
        }
        
        if self.backup.backup_paths.is_empty() {
            return Err(SkylockError::Config("At least one backup path is required".to_string()));
        }
        
        Ok(())
    }
}