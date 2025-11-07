use chrono::{DateTime, Utc, Duration};
use serde::{Serialize, Deserialize};
use skylock_core::Result;
use std::{path::PathBuf, collections::HashMap};
use tokio::fs;
use handlebars::Handlebars;
use crate::monitoring::{SystemStatus, DailyReport, Alert, AlertSeverity};

#[derive(Debug, Clone)]
pub struct ReportManager {
    base_path: PathBuf,
    handlebars: Handlebars<'static>,
    config: ReportConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportConfig {
    pub daily_report_hour: u32,
    pub weekly_report_day: u32,
    pub monthly_report_day: u32,
    pub retention_days: u32,
    pub email_recipients: Vec<String>,
    pub report_formats: Vec<ReportFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportFormat {
    HTML,
    JSON,
    PDF,
    CSV,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub backup_stats: BackupStats,
    pub sync_stats: SyncStats,
    pub system_stats: SystemStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStats {
    pub total_backups: u64,
    pub successful_backups: u64,
    pub failed_backups: u64,
    pub total_size: u64,
    pub average_duration: Duration,
    pub restore_points_created: u64,
    pub space_saved_by_dedup: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStats {
    pub files_synced: u64,
    pub bytes_transferred: u64,
    pub conflicts_resolved: u64,
    pub sync_errors: u64,
    pub average_sync_speed: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    pub average_cpu_usage: f64,
    pub average_memory_usage: f64,
    pub peak_disk_usage: f64,
    pub total_alerts: u64,
    pub critical_alerts: u64,
}

impl ReportManager {
    pub fn new(base_path: PathBuf, config: ReportConfig) -> Result<Self> {
        let mut handlebars = Handlebars::new();

        // Register templates
        handlebars.register_template_string("daily_report", include_str!("../templates/daily_report.hbs"))?;
        handlebars.register_template_string("weekly_report", include_str!("../templates/weekly_report.hbs"))?;
        handlebars.register_template_string("monthly_report", include_str!("../templates/monthly_report.hbs"))?;

        Ok(Self {
            base_path,
            handlebars,
            config,
        })
    }

    pub async fn generate_daily_report(&self, report: DailyReport) -> Result<()> {
        let report_path = self.get_report_path("daily", &report.date);
        fs::create_dir_all(&report_path).await?;

        for format in &self.config.report_formats {
            match format {
                ReportFormat::HTML => {
                    let html = self.handlebars.render("daily_report", &report)?;
                    fs::write(report_path.join("report.html"), html).await?;
                }
                ReportFormat::JSON => {
                    let json = serde_json::to_string_pretty(&report)?;
                    fs::write(report_path.join("report.json"), json).await?;
                }
                ReportFormat::PDF => {
                    // Convert HTML to PDF using headless browser or PDF library
                    // Implementation details...
                }
                ReportFormat::CSV => {
                    let mut csv = String::new();
                    // Convert report data to CSV format
                    // Implementation details...
                    fs::write(report_path.join("report.csv"), csv).await?;
                }
            }
        }

        // Send email if configured
        if !self.config.email_recipients.is_empty() {
            self.send_report_email(&report).await?;
        }

        Ok(())
    }

    pub async fn generate_performance_report(&self, period: Duration) -> Result<PerformanceReport> {
        let end = Utc::now();
        let start = end - period;

        let report = PerformanceReport {
            period_start: start,
            period_end: end,
            backup_stats: self.collect_backup_stats(start, end).await?,
            sync_stats: self.collect_sync_stats(start, end).await?,
            system_stats: self.collect_system_stats(start, end).await?,
        };

        Ok(report)
    }

    pub async fn generate_alerts_digest(&self, alerts: &[Alert]) -> Result<String> {
        let mut digest = String::new();
        let mut by_severity: HashMap<AlertSeverity, Vec<&Alert>> = HashMap::new();

        // Group alerts by severity
        for alert in alerts {
            by_severity.entry(alert.severity)
                .or_default()
                .push(alert);
        }

        // Generate digest
        for (severity, alerts) in by_severity.iter() {
            digest.push_str(&format!("\n{:?} Alerts ({}):\n", severity, alerts.len()));
            for alert in alerts {
                digest.push_str(&format!("- [{}] {}: {}\n",
                    alert.timestamp.format("%Y-%m-%d %H:%M:%S"),
                    alert.component,
                    alert.message
                ));
            }
        }

        Ok(digest)
    }

    async fn collect_backup_stats(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<BackupStats> {
        // Collect backup statistics from logs and database
        // Implementation details...
        Ok(BackupStats {
            total_backups: 0,
            successful_backups: 0,
            failed_backups: 0,
            total_size: 0,
            average_duration: Duration::seconds(0),
            restore_points_created: 0,
            space_saved_by_dedup: 0,
        })
    }

    async fn collect_sync_stats(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<SyncStats> {
        // Collect sync statistics from logs and database
        // Implementation details...
        Ok(SyncStats {
            files_synced: 0,
            bytes_transferred: 0,
            conflicts_resolved: 0,
            sync_errors: 0,
            average_sync_speed: 0.0,
        })
    }

    async fn collect_system_stats(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<SystemStats> {
        // Collect system statistics from monitoring data
        // Implementation details...
        Ok(SystemStats {
            average_cpu_usage: 0.0,
            average_memory_usage: 0.0,
            peak_disk_usage: 0.0,
            total_alerts: 0,
            critical_alerts: 0,
        })
    }

    fn get_report_path(&self, report_type: &str, date: &DateTime<Utc>) -> PathBuf {
        self.base_path
            .join("reports")
            .join(report_type)
            .join(date.format("%Y/%m/%d").to_string())
    }

    async fn send_report_email(&self, report: &DailyReport) -> Result<()> {
        // Send email using configured SMTP settings
        // Implementation details...
        Ok(())
    }

    pub async fn cleanup_old_reports(&self) -> Result<()> {
        let retention_date = Utc::now() - Duration::days(self.config.retention_days as i64);
        let reports_dir = self.base_path.join("reports");

        let mut entries = fs::read_dir(&reports_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_dir() {
                let modified: DateTime<Utc> = metadata.modified()?.into();
                if modified < retention_date {
                    fs::remove_dir_all(entry.path()).await?;
                }
            }
        }

        Ok(())
    }
}
