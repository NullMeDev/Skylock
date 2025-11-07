use chrono::{DateTime, Utc, Duration};
use tokio::sync::RwLock;
use std::sync::Arc;
use std::collections::HashMap;
use skylock_core::{Result, SkylockError, notifications::NotificationManager};
use crate::vss::VssManager;
use tracing::{info, error};

pub struct BackupScheduler {
    vss: Arc<VssManager>,
    notifications: NotificationManager,
    schedules: Arc<RwLock<HashMap<String, Schedule>>>,
}

#[derive(Debug, Clone)]
struct Schedule {
    name: String,
    interval: Duration,
    last_run: Option<DateTime<Utc>>,
    enabled: bool,
}

impl BackupScheduler {
    pub fn new(vss: VssManager, notifications: NotificationManager) -> Self {
        Self {
            vss: Arc::new(vss),
            notifications,
            schedules: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn start(&self) -> Result<()> {
        // Initialize default schedules
        self.init_default_schedules().await?;

        // Start the scheduler loop
        self.run_scheduler().await?;

        Ok(())
    }

    async fn init_default_schedules(&self) -> Result<()> {
        let mut schedules = self.schedules.write().await;

        // Hourly reconciliation
        schedules.insert("hourly".into(), Schedule {
            name: "Hourly Reconciliation".into(),
            interval: Duration::hours(1),
            last_run: None,
            enabled: true,
        });

        // Daily backup
        schedules.insert("daily".into(), Schedule {
            name: "Daily Backup".into(),
            interval: Duration::days(1),
            last_run: None,
            enabled: true,
        });

        // Weekly integrity check
        schedules.insert("weekly".into(), Schedule {
            name: "Weekly Integrity Check".into(),
            interval: Duration::weeks(1),
            last_run: None,
            enabled: true,
        });

        Ok(())
    }

    async fn run_scheduler(&self) -> Result<()> {
        let vss = self.vss.clone();
        let schedules = self.schedules.clone();
        let notifications = self.notifications.clone();

        tokio::spawn(async move {
            loop {
                let now = Utc::now();
                let mut schedules = schedules.write().await;

                for schedule in schedules.values_mut() {
                    if !schedule.enabled {
                        continue;
                    }

                    let should_run = match schedule.last_run {
                        Some(last_run) => now - last_run >= schedule.interval,
                        None => true,
                    };

                    if should_run {
                        info!("Running scheduled task: {}", schedule.name);
                        match schedule.name.as_str() {
                            "Hourly Reconciliation" => {
                                if let Err(e) = Self::run_reconciliation(&vss, &notifications).await {
                                    error!("Reconciliation failed: {}", e);
                                    notifications.notify_backup_failed(e.to_string())?;
                                }
                            }
                            "Daily Backup" => {
                                if let Err(e) = Self::run_daily_backup(&vss, &notifications).await {
                                    error!("Daily backup failed: {}", e);
                                    notifications.notify_backup_failed(e.to_string())?;
                                }
                            }
                            "Weekly Integrity Check" => {
                                if let Err(e) = Self::run_integrity_check(&vss, &notifications).await {
                                    error!("Integrity check failed: {}", e);
                                    notifications.notify_backup_failed(e.to_string())?;
                                }
                            }
                            _ => {}
                        }
                        schedule.last_run = Some(now);
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            }
        });

        Ok(())
    }

    async fn run_reconciliation(
        vss: &VssManager,
        notifications: &NotificationManager,
    ) -> Result<()> {
        // Create VSS snapshot
        let snapshot = vss.create_snapshot().await?;

        // Verify file integrity from snapshot
        vss.verify_files(&snapshot).await?;

        notifications.notify_backup_progress("Reconciliation complete".into(), 100)?;
        Ok(())
    }

    async fn run_daily_backup(
        vss: &VssManager,
        notifications: &NotificationManager,
    ) -> Result<()> {
        // Create VSS snapshot
        let snapshot = vss.create_snapshot().await?;

        // Perform full backup using snapshot
        vss.backup_files(&snapshot).await?;

        notifications.notify_backup_progress("Daily backup complete".into(), 100)?;
        Ok(())
    }

    async fn run_integrity_check(
        vss: &VssManager,
        notifications: &NotificationManager,
    ) -> Result<()> {
        // Run deep integrity verification
        vss.verify_integrity().await?;

        notifications.notify_backup_progress("Weekly integrity check complete".into(), 100)?;
        Ok(())
    }
}
