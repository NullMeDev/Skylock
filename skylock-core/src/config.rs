use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use crate::Result;
use crate::storage::DriveConfig;
use crate::error::SkylockError;
use crate::virtual_drive::SyncMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub storage: StorageConfig,
    pub sync: SyncConfig,
    pub backup: BackupConfig,
    pub monitor: MonitorConfig,
    pub security: SecurityConfig,
    pub ui: UiConfig,
    pub error_handling: ErrorHandlingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorHandlingConfig {
    pub max_retry_attempts: usize,
    pub retry_delay_ms: u64,
    pub error_history_size: usize,
    pub circuit_breaker_threshold: usize,
    pub circuit_breaker_timeout_ms: u64,
    pub logging_level: String,
    pub notification_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub provider: String,
    pub api_token: Option<String>,
    pub box_id: Option<u64>,
    pub subaccount_id: Option<u64>,
    pub connection_string: Option<String>,
    pub drives: Vec<DriveConfig>,
    pub encryption_key: Option<String>,
    pub compression_enabled: bool,
    pub cache_path: PathBuf,
    pub cache_size_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub mode: SyncMode,
    pub schedule: Vec<DateTime<Utc>>,
    pub directories: Vec<PathBuf>,
    pub ignore_patterns: Vec<String>,
    pub batch_size: usize,
    pub verify_transfers: bool,
    pub bandwidth_limit_mbps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub schedule: Vec<DateTime<Utc>>,
    pub retention_policy: RetentionPolicy,
    pub snapshot_config: SnapshotConfig,
    pub deduplication_enabled: bool,
    pub compression_level: u32,
    pub vss_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    pub notification_enabled: bool,
    pub email_alerts: Option<EmailConfig>,
    pub webhooks: Vec<WebhookConfig>,
    pub log_path: PathBuf,
    pub log_level: String,
    pub metrics_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub encryption_algorithm: String,
    pub key_rotation_days: u32,
    pub require_password: bool,
    pub allow_remote_access: bool,
    pub trusted_networks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub auto_start: bool,
    pub minimize_to_tray: bool,
    pub theme: String,
    pub language: String,
    pub show_notifications: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub max_snapshots: usize,
    pub keep_daily: usize,
    pub keep_weekly: usize,
    pub keep_monthly: usize,
    pub min_age_hours: u32,
    pub max_age_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotConfig {
    pub automatic: bool,
    pub compression_enabled: bool,
    pub verify_after_create: bool,
    pub replication_targets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub smtp_server: String,
    pub smtp_port: u16,
    pub username: String,
    pub password: String,
    pub from_address: String,
    pub to_addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
    pub events: Vec<String>,
    pub headers: std::collections::HashMap<String, String>,
}

impl Default for ErrorHandlingConfig {
    fn default() -> Self {
        Self {
            max_retry_attempts: 3,
            retry_delay_ms: 5000,
            error_history_size: 1000,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout_ms: 60000,
            logging_level: "info".to_string(),
            notification_level: "error".to_string(),
        }
    }
}

impl Config {
    pub fn load(path: &PathBuf) -> Result<Self> {
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

    pub fn save(&self, path: Option<PathBuf>) -> Result<()> {
        let path = path.unwrap_or_else(|| {
            directories::ProjectDirs::from("com", "skylock", "skylock-hybrid")
                .map(|proj_dirs| proj_dirs.config_dir().join("config.toml"))
                .unwrap_or_else(|| PathBuf::from("config.toml"))
        });

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| SkylockError::IO(format!("Failed to create config directory: {}", e)))?;
        }

        let config_str = toml::to_string_pretty(self)
            .map_err(|e| SkylockError::Config(format!("Failed to serialize config: {}", e)))?;

        std::fs::write(&path, config_str)
            .map_err(|e| SkylockError::IO(format!("Failed to write config file: {}", e)))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            storage: StorageConfig {
                provider: "hetzner".to_string(),
                api_token: None,
                box_id: None,
                subaccount_id: None,
                connection_string: None,
                drives: Vec::new(),
                encryption_key: None,
                compression_enabled: true,
                cache_path: dirs::cache_dir()
                    .unwrap_or_else(|| PathBuf::from("cache"))
                    .join("skylock"),
                cache_size_mb: 1024 * 10, // 10GB
            },
            sync: SyncConfig {
                mode: SyncMode::RealTime,
                schedule: Vec::new(),
                directories: Vec::new(),
                ignore_patterns: vec![
                    String::from("*.tmp"),
                    String::from("*.temp"),
                    String::from("~*"),
                    String::from(".git/"),
                ],
                batch_size: 1000,
                verify_transfers: true,
                bandwidth_limit_mbps: None,
            },
            backup: BackupConfig {
                schedule: Vec::new(),
                retention_policy: RetentionPolicy {
                    max_snapshots: 100,
                    keep_daily: 7,
                    keep_weekly: 4,
                    keep_monthly: 3,
                    min_age_hours: 1,
                    max_age_days: 90,
                },
                snapshot_config: SnapshotConfig {
                    automatic: true,
                    compression_enabled: true,
                    verify_after_create: true,
                    replication_targets: Vec::new(),
                },
                deduplication_enabled: true,
                compression_level: 6,
                vss_enabled: true,
            },
            monitor: MonitorConfig {
                notification_enabled: true,
                email_alerts: None,
                webhooks: Vec::new(),
                log_path: dirs::data_local_dir()
                    .unwrap_or_else(|| PathBuf::from("logs"))
                    .join("skylock")
                    .join("skylock.log"),
                log_level: "info".to_string(),
                metrics_enabled: true,
            },
            security: SecurityConfig {
                encryption_algorithm: "AES-256-GCM".to_string(),
                key_rotation_days: 90,
                require_password: false,
                allow_remote_access: false,
                trusted_networks: Vec::new(),
            },
            ui: UiConfig {
                auto_start: true,
                minimize_to_tray: true,
                theme: "system".to_string(),
                language: "en".to_string(),
                show_notifications: true,
            },
        }
    }
}
