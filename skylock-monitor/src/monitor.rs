use std::collections::HashMap;
use tokio::sync::mpsc;
use chrono::{DateTime, Utc, Duration};
use serde::{Serialize, Deserialize};
use skylock_core::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub check_interval: Duration,
    pub storage_thresholds: StorageThresholds,
    pub backup_thresholds: BackupThresholds,
    pub notification_settings: NotificationSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageThresholds {
    pub warning_percentage: f64,
    pub critical_percentage: f64,
    pub min_free_space: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupThresholds {
    pub max_backup_age: Duration,
    pub max_verification_age: Duration,
    pub min_restore_points: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub enabled: bool,
    pub email_recipients: Vec<String>,
    pub webhook_urls: Vec<String>,
    pub toast_notifications: bool,
    pub log_file: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SystemAlert {
    StorageWarning {
        path: String,
        used_percentage: f64,
        free_space: u64,
    },
    StorageCritical {
        path: String,
        used_percentage: f64,
        free_space: u64,
    },
    BackupOutdated {
        last_backup: DateTime<Utc>,
        elapsed: Duration,
    },
    VerificationNeeded {
        point_id: String,
        last_verified: DateTime<Utc>,
    },
    RestorePointLow {
        current_count: usize,
        minimum_required: usize,
    },
    DeduplicationNeeded {
        wasted_space: u64,
        potential_savings: f64,
    },
    SystemError {
        component: String,
        error: String,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Debug, Clone)]
pub enum ComponentStatus {
    Healthy,
    Warning(String),
    Error(String),
    Inactive,
}

#[derive(Debug, Clone)]
pub struct SystemStatus {
    pub timestamp: DateTime<Utc>,
    pub components: HashMap<String, ComponentStatus>,
    pub storage_usage: StorageUsage,
    pub backup_status: BackupStatus,
    pub alerts: Vec<SystemAlert>,
}

#[derive(Debug, Clone)]
pub struct StorageUsage {
    pub total_space: u64,
    pub used_space: u64,
    pub free_space: u64,
    pub dedup_savings: f64,
}

#[derive(Debug, Clone)]
pub struct BackupStatus {
    pub last_backup: DateTime<Utc>,
    pub last_verification: DateTime<Utc>,
    pub restore_points: usize,
    pub total_size: u64,
}

pub struct SystemMonitor {
    config: MonitoringConfig,
    status: SystemStatus,
    alert_tx: mpsc::Sender<SystemAlert>,
}

impl SystemMonitor {
    pub fn new(config: MonitoringConfig, alert_tx: mpsc::Sender<SystemAlert>) -> Self {
        Self {
            config,
            status: SystemStatus {
                timestamp: Utc::now(),
                components: HashMap::new(),
                storage_usage: StorageUsage {
                    total_space: 0,
                    used_space: 0,
                    free_space: 0,
                    dedup_savings: 0.0,
                },
                backup_status: BackupStatus {
                    last_backup: Utc::now(),
                    last_verification: Utc::now(),
                    restore_points: 0,
                    total_size: 0,
                },
                alerts: Vec::new(),
            },
            alert_tx,
        }
    }

    pub async fn start_monitoring(&mut self) -> Result<()> {
        loop {
            self.check_system_status().await?;
            tokio::time::sleep(self.config.check_interval.to_std()?).await;
        }
    }

    async fn check_system_status(&mut self) -> Result<()> {
        self.status.timestamp = Utc::now();
        self.status.alerts.clear();

        // Check storage
        self.check_storage().await?;

        // Check backup status
        self.check_backup_status().await?;

        // Check component health
        self.check_components().await?;

        // Check deduplication efficiency
        self.check_deduplication().await?;

        Ok(())
    }

    async fn check_storage(&mut self) -> Result<()> {
        let usage = &self.status.storage_usage;
        let used_percentage = (usage.used_space as f64 / usage.total_space as f64) * 100.0;

        if usage.free_space < self.config.storage_thresholds.min_free_space {
            self.add_alert(SystemAlert::StorageCritical {
                path: "backup_storage".to_string(),
                used_percentage,
                free_space: usage.free_space,
            }).await?;
        } else if used_percentage >= self.config.storage_thresholds.critical_percentage {
            self.add_alert(SystemAlert::StorageCritical {
                path: "backup_storage".to_string(),
                used_percentage,
                free_space: usage.free_space,
            }).await?;
        } else if used_percentage >= self.config.storage_thresholds.warning_percentage {
            self.add_alert(SystemAlert::StorageWarning {
                path: "backup_storage".to_string(),
                used_percentage,
                free_space: usage.free_space,
            }).await?;
        }

        Ok(())
    }

    async fn check_backup_status(&mut self) -> Result<()> {
        let now = Utc::now();
        let backup_status = &self.status.backup_status;

        // Check backup age
        let backup_age = now - backup_status.last_backup;
        if backup_age > self.config.backup_thresholds.max_backup_age {
            self.add_alert(SystemAlert::BackupOutdated {
                last_backup: backup_status.last_backup,
                elapsed: backup_age,
            }).await?;
        }

        // Check verification age
        let verification_age = now - backup_status.last_verification;
        if verification_age > self.config.backup_thresholds.max_verification_age {
            self.add_alert(SystemAlert::VerificationNeeded {
                point_id: "latest".to_string(),
                last_verified: backup_status.last_verification,
            }).await?;
        }

        // Check restore points count
        if backup_status.restore_points < self.config.backup_thresholds.min_restore_points {
            self.add_alert(SystemAlert::RestorePointLow {
                current_count: backup_status.restore_points,
                minimum_required: self.config.backup_thresholds.min_restore_points,
            }).await?;
        }

        Ok(())
    }

    async fn check_components(&mut self) -> Result<()> {
        for (component, status) in &self.status.components {
            match status {
                ComponentStatus::Error(error) => {
                    self.add_alert(SystemAlert::SystemError {
                        component: component.clone(),
                        error: error.clone(),
                        timestamp: Utc::now(),
                    }).await?;
                },
                ComponentStatus::Warning(warning) => {
                    // Log warning but don't alert
                    println!("Warning in component {}: {}", component, warning);
                },
                _ => {},
            }
        }

        Ok(())
    }

    async fn check_deduplication(&mut self) -> Result<()> {
        let dedup_savings = self.status.storage_usage.dedup_savings;
        if dedup_savings < 10.0 {  // Less than 10% savings
            let wasted_space = self.status.storage_usage.used_space / 10;  // Estimate
            self.add_alert(SystemAlert::DeduplicationNeeded {
                wasted_space,
                potential_savings: 20.0,  // Conservative estimate
            }).await?;
        }

        Ok(())
    }

    async fn add_alert(&mut self, alert: SystemAlert) -> Result<()> {
        self.status.alerts.push(alert.clone());
        self.alert_tx.send(alert).await?;
        Ok(())
    }

    pub fn update_component_status(&mut self, component: String, status: ComponentStatus) {
        self.status.components.insert(component, status);
    }

    pub fn update_storage_usage(&mut self, usage: StorageUsage) {
        self.status.storage_usage = usage;
    }

    pub fn update_backup_status(&mut self, status: BackupStatus) {
        self.status.backup_status = status;
    }

    pub fn get_current_status(&self) -> &SystemStatus {
        &self.status
    }
}
