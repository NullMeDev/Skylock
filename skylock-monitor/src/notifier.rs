use tokio::sync::mpsc;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use skylock_core::Result;
use std::collections::VecDeque;
use crate::monitor::SystemAlert;
use reqwest::Client;
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    pub email: EmailConfig,
    pub webhook: WebhookConfig,
    pub toast: ToastConfig,
    pub logging: LogConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub enabled: bool,
    pub smtp_server: String,
    pub smtp_port: u16,
    pub username: String,
    pub password: String,
    pub from_address: String,
    pub recipients: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub enabled: bool,
    pub urls: Vec<String>,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToastConfig {
    pub enabled: bool,
    pub max_notifications: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
    pub enabled: bool,
    pub log_file: PathBuf,
    pub max_size: u64,
    pub max_files: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Notification {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub severity: NotificationSeverity,
    pub title: String,
    pub message: String,
    pub source: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum NotificationSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

pub struct NotificationManager {
    config: NotificationConfig,
    alert_rx: mpsc::Receiver<SystemAlert>,
    http_client: Client,
    recent_notifications: VecDeque<Notification>,
}

impl NotificationManager {
    pub fn new(config: NotificationConfig, alert_rx: mpsc::Receiver<SystemAlert>) -> Self {
        Self {
            config,
            alert_rx,
            http_client: Client::new(),
            recent_notifications: VecDeque::with_capacity(100),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        while let Some(alert) = self.alert_rx.recv().await {
            let notification = self.create_notification(alert);

            // Store in recent notifications
            self.add_recent_notification(notification.clone());

            // Process notification through all enabled channels
            self.process_notification(&notification).await?;
        }

        Ok(())
    }

    fn create_notification(&self, alert: SystemAlert) -> Notification {
        let (severity, title, message) = match &alert {
            SystemAlert::StorageWarning { path, used_percentage, free_space } => (
                NotificationSeverity::Warning,
                "Storage Space Warning".to_string(),
                format!(
                    "Storage space is running low on {}. Used: {:.1}%, Free: {} bytes",
                    path, used_percentage, free_space
                ),
            ),
            SystemAlert::StorageCritical { path, used_percentage, free_space } => (
                NotificationSeverity::Critical,
                "Critical Storage Space Alert".to_string(),
                format!(
                    "Critical storage space situation on {}. Used: {:.1}%, Free: {} bytes",
                    path, used_percentage, free_space
                ),
            ),
            SystemAlert::BackupOutdated { last_backup, elapsed } => (
                NotificationSeverity::Warning,
                "Backup Outdated".to_string(),
                format!(
                    "Last backup was performed {} days ago. Maximum age exceeded.",
                    elapsed.num_days()
                ),
            ),
            SystemAlert::VerificationNeeded { point_id, last_verified } => (
                NotificationSeverity::Warning,
                "Backup Verification Required".to_string(),
                format!(
                    "Restore point {} needs verification. Last verified: {}",
                    point_id, last_verified
                ),
            ),
            SystemAlert::RestorePointLow { current_count, minimum_required } => (
                NotificationSeverity::Warning,
                "Low Restore Point Count".to_string(),
                format!(
                    "Current restore points: {}. Minimum required: {}",
                    current_count, minimum_required
                ),
            ),
            SystemAlert::DeduplicationNeeded { wasted_space, potential_savings } => (
                NotificationSeverity::Info,
                "Deduplication Recommended".to_string(),
                format!(
                    "Potential space savings of {:.1}% ({} bytes) through deduplication",
                    potential_savings, wasted_space
                ),
            ),
            SystemAlert::SystemError { component, error, timestamp } => (
                NotificationSeverity::Error,
                format!("Error in {}", component),
                format!("Error occurred at {}: {}", timestamp, error),
            ),
        };

        Notification {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            severity,
            title,
            message,
            source: "Skylock".to_string(),
            metadata: serde_json::to_value(&alert).unwrap_or_default(),
        }
    }

    async fn process_notification(&self, notification: &Notification) -> Result<()> {
        // Log notification
        if self.config.logging.enabled {
            self.log_notification(notification).await?;
        }

        // Send email if enabled and severity warrants it
        if self.config.email.enabled && notification.severity >= NotificationSeverity::Warning {
            self.send_email(notification).await?;
        }

        // Send webhook if enabled
        if self.config.webhook.enabled {
            self.send_webhook(notification).await?;
        }

        // Show toast notification if enabled and severity warrants it
        if self.config.toast.enabled && notification.severity >= NotificationSeverity::Warning {
            self.show_toast(notification).await?;
        }

        Ok(())
    }

    async fn log_notification(&self, notification: &Notification) -> Result<()> {
        let log_entry = format!(
            "[{}] [{:?}] {}: {}\n",
            notification.timestamp,
            notification.severity,
            notification.title,
            notification.message
        );

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.config.logging.log_file)
            .await?;

        file.write_all(log_entry.as_bytes()).await?;

        // Check file size and rotate if needed
        if file.metadata().await?.len() > self.config.logging.max_size {
            self.rotate_logs().await?;
        }

        Ok(())
    }

    async fn rotate_logs(&self) -> Result<()> {
        let base_path = self.config.logging.log_file.parent().unwrap_or(&PathBuf::from("."));
        let file_name = self.config.logging.log_file.file_name()
            .ok_or_else(|| Error::InvalidConfig("Log file path must have a file name".to_string()))?;

        // Rotate existing log files
        for i in (1..self.config.logging.max_files).rev() {
            let src = base_path.join(format!("{}.{}", file_name.to_string_lossy(), i));
            let dst = base_path.join(format!("{}.{}", file_name.to_string_lossy(), i + 1));

            if src.exists() {
                tokio::fs::rename(src, dst).await?;
            }
        }

        // Rotate current log to .1
        let new_log = base_path.join(format!("{}.1", file_name.to_string_lossy()));
        tokio::fs::rename(&self.config.logging.log_file, new_log).await?;

        // Create new empty log file
        OpenOptions::new()
            .create(true)
            .write(true)
            .open(&self.config.logging.log_file)
            .await?;

        Ok(())
    }

    async fn send_email(&self, notification: &Notification) -> Result<()> {
        let email_body = format!(
            "Severity: {:?}\nTitle: {}\nMessage: {}\nTimestamp: {}\n",
            notification.severity,
            notification.title,
            notification.message,
            notification.timestamp
        );

        let smtp_transport = lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::relay(&self.config.email.smtp_server)
            .unwrap()
            .credentials(lettre::transport::smtp::authentication::Credentials::new(
                self.config.email.username.clone(),
                self.config.email.password.clone(),
            ))
            .build();

        let email = lettre::Message::builder()
            .from(self.config.email.from_address.parse()?)
            .to(self.config.email.recipients[0].parse()?) // First recipient
            .subject(&notification.title)
            .body(email_body)?;

        smtp_transport.send(email).await?;

        Ok(())
    }

    async fn send_webhook(&self, notification: &Notification) -> Result<()> {
        for url in &self.config.webhook.urls {
            let response = self.http_client
                .post(url)
                .json(&notification)
                .headers(self.build_webhook_headers())
                .send()
                .await?;

            if !response.status().is_success() {
                eprintln!("Failed to send webhook: {}", response.status());
            }
        }

        Ok(())
    }

    fn build_webhook_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        for (key, value) in &self.config.webhook.headers {
            if let (Ok(name), Ok(val)) = (
                reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                reqwest::header::HeaderValue::from_str(value)
            ) {
                headers.insert(name, val);
            }
        }
        headers
    }

    async fn show_toast(&self, notification: &Notification) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            use windows_notification::{Toast, NotificationBuilder};

            let toast = NotificationBuilder::new()
                .title(&notification.title)
                .text(&notification.message)
                .build()?;

            toast.show()?;
        }

        Ok(())
    }

    fn add_recent_notification(&mut self, notification: Notification) {
        self.recent_notifications.push_back(notification);

        // Keep only the most recent notifications
        while self.recent_notifications.len() > 100 {
            self.recent_notifications.pop_front();
        }
    }

    pub fn get_recent_notifications(&self) -> Vec<&Notification> {
        self.recent_notifications.iter().collect()
    }
}
