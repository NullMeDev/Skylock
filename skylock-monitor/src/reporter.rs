use std::path::PathBuf;
use chrono::{DateTime, Utc, Duration};
use serde::{Serialize, Deserialize};
use skylock_core::Result;
use tokio::fs;
use crate::monitor::{SystemStatus, ComponentStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportConfig {
    pub report_dir: PathBuf,
    pub formats: Vec<ReportFormat>,
    pub intervals: Vec<ReportInterval>,
    pub retention: ReportRetention,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportFormat {
    JSON,
    HTML,
    PDF,
    CSV,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportInterval {
    Daily,
    Weekly,
    Monthly,
    Custom(Duration),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportRetention {
    pub max_reports: usize,
    pub max_age_days: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemReport {
    pub timestamp: DateTime<Utc>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub summary: ReportSummary,
    pub details: ReportDetails,
    pub recommendations: Vec<Recommendation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportSummary {
    pub total_files: usize,
    pub total_size: u64,
    pub backup_points: usize,
    pub space_savings: f64,
    pub health_score: f64,
    pub alerts_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportDetails {
    pub storage_metrics: StorageMetrics,
    pub backup_metrics: BackupMetrics,
    pub performance_metrics: PerformanceMetrics,
    pub component_health: Vec<ComponentHealth>,
    pub alerts: Vec<AlertSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageMetrics {
    pub total_space: u64,
    pub used_space: u64,
    pub free_space: u64,
    pub dedup_ratio: f64,
    pub compression_ratio: f64,
    pub growth_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupMetrics {
    pub successful_backups: usize,
    pub failed_backups: usize,
    pub average_backup_size: u64,
    pub average_backup_time: Duration,
    pub restore_points: usize,
    pub verified_points: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PerformanceMetrics {
    pub average_scan_time: Duration,
    pub average_sync_time: Duration,
    pub average_dedup_ratio: f64,
    pub cpu_usage: f64,
    pub memory_usage: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: ComponentStatus,
    pub uptime: Duration,
    pub error_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlertSummary {
    pub severity: String,
    pub count: usize,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub category: String,
    pub priority: u32,
    pub title: String,
    pub description: String,
    pub action_items: Vec<String>,
}

pub struct ReportGenerator {
    config: ReportConfig,
    system_status: Vec<SystemStatus>,
    last_report: Option<DateTime<Utc>>,
}

impl ReportGenerator {
    pub fn new(config: ReportConfig) -> Self {
        Self {
            config,
            system_status: Vec::new(),
            last_report: None,
        }
    }

    pub fn add_status(&mut self, status: SystemStatus) {
        self.system_status.push(status);

        // Keep only relevant history
        let max_age = chrono::Duration::days(30); // Keep last 30 days
        let cutoff = Utc::now() - max_age;
        self.system_status.retain(|s| s.timestamp > cutoff);
    }

    pub async fn generate_reports(&mut self) -> Result<()> {
        for interval in &self.config.intervals {
            if self.should_generate_report(interval) {
                let report = self.create_report(interval)?;
                self.save_report(&report).await?;
                self.last_report = Some(Utc::now());
            }
        }

        // Apply retention policy
        self.apply_retention_policy().await?;

        Ok(())
    }

    fn should_generate_report(&self, interval: &ReportInterval) -> bool {
        let now = Utc::now();

        if let Some(last) = self.last_report {
            match interval {
                ReportInterval::Daily => {
                    now.date_naive() != last.date_naive()
                },
                ReportInterval::Weekly => {
                    now.iso_week() != last.iso_week()
                },
                ReportInterval::Monthly => {
                    now.month() != last.month() || now.year() != last.year()
                },
                ReportInterval::Custom(duration) => {
                    now - last > *duration
                },
            }
        } else {
            true
        }
    }

    fn create_report(&self, interval: &ReportInterval) -> Result<SystemReport> {
        let now = Utc::now();
        let (period_start, period_end) = self.calculate_period(interval);

        let relevant_status: Vec<_> = self.system_status.iter()
            .filter(|s| s.timestamp >= period_start && s.timestamp <= period_end)
            .collect();

        let summary = self.generate_summary(&relevant_status);
        let details = self.generate_details(&relevant_status);
        let recommendations = self.generate_recommendations(&summary, &details);

        Ok(SystemReport {
            timestamp: now,
            period_start,
            period_end,
            summary,
            details,
            recommendations,
        })
    }

    fn calculate_period(&self, interval: &ReportInterval) -> (DateTime<Utc>, DateTime<Utc>) {
        let now = Utc::now();

        match interval {
            ReportInterval::Daily => {
                let start = now.date_naive().and_hms_opt(0, 0, 0)
                    .unwrap_or_else(|| now.date_naive().and_hms_opt(0, 0, 0).expect("Invalid time"));
                (
                    DateTime::from_naive_utc_and_offset(start, Utc),
                    now,
                )
            },
            ReportInterval::Weekly => {
                let start = now - chrono::Duration::days(7);
                (start, now)
            },
            ReportInterval::Monthly => {
                let start = now - chrono::Duration::days(30);
                (start, now)
            },
            ReportInterval::Custom(duration) => {
                let start = now - *duration;
                (start, now)
            },
        }
    }

    fn generate_summary(&self, status_history: &[&SystemStatus]) -> Option<ReportSummary> {
        if status_history.is_empty() {
            return None;
        }

        let latest = status_history.last().unwrap_or(&status_history[0]);
        let alerts_count = status_history.iter()
            .map(|s| s.alerts.len())
            .sum();

        Some(ReportSummary {
            total_files: 1000, // Replace with actual metrics
            total_size: latest.storage_usage.total_space,
            backup_points: latest.backup_status.restore_points,
            space_savings: latest.storage_usage.dedup_savings,
            health_score: self.calculate_health_score(status_history),
            alerts_count,
        })
    }

    fn generate_details(&self, status_history: &[&SystemStatus]) -> Option<ReportDetails> {
        let latest = status_history.last()?;

        ReportDetails {
            storage_metrics: self.calculate_storage_metrics(status_history),
            backup_metrics: self.calculate_backup_metrics(status_history),
            performance_metrics: self.calculate_performance_metrics(status_history),
            component_health: self.calculate_component_health(status_history),
            alerts: self.summarize_alerts(status_history),
        }
    }

    fn calculate_storage_metrics(&self, status_history: &[&SystemStatus]) -> StorageMetrics {
        let latest = status_history.last().unwrap();

        StorageMetrics {
            total_space: latest.storage_usage.total_space,
            used_space: latest.storage_usage.used_space,
            free_space: latest.storage_usage.free_space,
            dedup_ratio: latest.storage_usage.dedup_savings,
            compression_ratio: 2.0, // Replace with actual metric
            growth_rate: self.calculate_growth_rate(status_history),
        }
    }

    fn calculate_backup_metrics(&self, status_history: &[&SystemStatus]) -> BackupMetrics {
        let latest = status_history.last().unwrap();

        BackupMetrics {
            successful_backups: 10, // Replace with actual metrics
            failed_backups: 0,
            average_backup_size: latest.backup_status.total_size / latest.backup_status.restore_points as u64,
            average_backup_time: Duration::hours(1), // Replace with actual metric
            restore_points: latest.backup_status.restore_points,
            verified_points: 5, // Replace with actual metric
        }
    }

    fn calculate_performance_metrics(&self, _status_history: &[&SystemStatus]) -> PerformanceMetrics {
        PerformanceMetrics {
            average_scan_time: Duration::seconds(30),
            average_sync_time: Duration::minutes(5),
            average_dedup_ratio: 2.5,
            cpu_usage: 15.0,
            memory_usage: 256.0,
        }
    }

    fn calculate_component_health(&self, status_history: &[&SystemStatus]) -> Vec<ComponentHealth> {
        let latest = status_history.last().unwrap();

        latest.components.iter().map(|(name, status)| {
            ComponentHealth {
                name: name.clone(),
                status: status.clone(),
                uptime: Duration::hours(24), // Replace with actual metric
                error_count: self.count_component_errors(name, status_history),
            }
        }).collect()
    }

    fn summarize_alerts(&self, status_history: &[&SystemStatus]) -> Vec<AlertSummary> {
        let mut summaries = HashMap::new();

        for status in status_history {
            for alert in &status.alerts {
                let key = format!("{:?}", alert);
                let entry = summaries.entry(key.clone()).or_insert_with(|| AlertSummary {
                    severity: "Warning".to_string(), // Replace with actual severity
                    count: 0,
                    first_seen: status.timestamp,
                    last_seen: status.timestamp,
                    source: "System".to_string(),
                });

                entry.count += 1;
                entry.last_seen = status.timestamp;
            }
        }

        summaries.into_values().collect()
    }

    fn generate_recommendations(
        &self,
        summary: &ReportSummary,
        details: &ReportDetails,
    ) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        // Storage recommendations
        if details.storage_metrics.free_space < 1_000_000_000 { // 1GB
            recommendations.push(Recommendation {
                category: "Storage".to_string(),
                priority: 1,
                title: "Low Storage Space".to_string(),
                description: "Available storage space is running low.".to_string(),
                action_items: vec![
                    "Clean up unnecessary files".to_string(),
                    "Run deduplication".to_string(),
                    "Consider adding more storage".to_string(),
                ],
            });
        }

        // Backup recommendations
        if details.backup_metrics.verified_points < details.backup_metrics.restore_points {
            recommendations.push(Recommendation {
                category: "Backup".to_string(),
                priority: 2,
                title: "Unverified Backup Points".to_string(),
                description: "Some backup points haven't been verified.".to_string(),
                action_items: vec![
                    "Run verification on unverified points".to_string(),
                    "Schedule regular verifications".to_string(),
                ],
            });
        }

        recommendations
    }

    fn calculate_health_score(&self, status_history: &[&SystemStatus]) -> f64 {
        let mut score = 100.0;

        // Deduct points for alerts
        let alert_count = status_history.iter()
            .map(|s| s.alerts.len())
            .sum::<usize>();
        score -= (alert_count as f64) * 5.0;

        // Deduct points for component issues
        let latest = status_history.last().unwrap();
        for (_, status) in &latest.components {
            match status {
                ComponentStatus::Error(_) => score -= 20.0,
                ComponentStatus::Warning(_) => score -= 10.0,
                ComponentStatus::Inactive => score -= 15.0,
                _ => {},
            }
        }

        score.max(0.0).min(100.0)
    }

    fn calculate_growth_rate(&self, status_history: &[&SystemStatus]) -> f64 {
        if status_history.len() < 2 {
            return 0.0;
        }

        let first = status_history.first().unwrap();
        let last = status_history.last().unwrap();
        let time_diff = (last.timestamp - first.timestamp).num_days() as f64;

        if time_diff == 0.0 {
            return 0.0;
        }

        let size_diff = last.storage_usage.used_space as f64 - first.storage_usage.used_space as f64;
        (size_diff / time_diff) / (1024.0 * 1024.0) // MB per day
    }

    fn count_component_errors(&self, component: &str, status_history: &[&SystemStatus]) -> usize {
        status_history.iter()
            .filter(|s| matches!(s.components.get(component), Some(ComponentStatus::Error(_))))
            .count()
    }

    async fn save_report(&self, report: &SystemReport) -> Result<()> {
        fs::create_dir_all(&self.config.report_dir).await?;

        for format in &self.config.formats {
            let filename = format!(
                "report_{}_{}_{}.{}",
                report.period_start.format("%Y%m%d"),
                report.period_end.format("%Y%m%d"),
                uuid::Uuid::new_v4(),
                format.extension()
            );
            let path = self.config.report_dir.join(filename);

            match format {
                ReportFormat::JSON => {
                    let json = serde_json::to_string_pretty(report)?;
                    fs::write(path, json).await?;
                },
                ReportFormat::HTML => {
                    let html = self.generate_html_report(report)?;
                    fs::write(path, html).await?;
                },
                ReportFormat::PDF => {
                    let pdf = self.generate_pdf_report(report)?;
                    fs::write(path, pdf).await?;
                },
                ReportFormat::CSV => {
                    let csv = self.generate_csv_report(report)?;
                    fs::write(path, csv).await?;
                },
            }
        }

        Ok(())
    }

    async fn apply_retention_policy(&self) -> Result<()> {
        let mut entries = fs::read_dir(&self.config.report_dir).await?;
        let mut reports = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            let modified = DateTime::from(metadata.modified()?);

            reports.push((entry.path(), modified));
        }

        // Sort by modification time (newest first)
        reports.sort_by(|a, b| b.1.cmp(&a.1));

        // Apply max reports limit
        if reports.len() > self.config.retention.max_reports {
            for (path, _) in reports.iter().skip(self.config.retention.max_reports) {
                fs::remove_file(path).await?;
            }
        }

        // Apply age limit
        let age_limit = Utc::now() - chrono::Duration::days(self.config.retention.max_age_days);
        for (path, modified) in reports {
            if modified < age_limit {
                fs::remove_file(path).await?;
            }
        }

        Ok(())
    }

    fn generate_html_report(&self, report: &SystemReport) -> Result<String> {
        // Basic HTML template
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <title>System Report - {}</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 40px; }}
        .summary {{ background: #f5f5f5; padding: 20px; margin-bottom: 20px; }}
        .metrics {{ display: grid; grid-template-columns: repeat(3, 1fr); gap: 20px; }}
        .alerts {{ color: #d32f2f; }}
    </style>
</head>
<body>
    <h1>System Report</h1>
    <div class="summary">
        <h2>Summary</h2>
        <p>Period: {} to {}</p>
        <p>Health Score: {:.1}%</p>
        <p>Total Size: {} GB</p>
        <p>Backup Points: {}</p>
    </div>
    <div class="metrics">
        <div>
            <h3>Storage</h3>
            <p>Used: {:.1} GB</p>
            <p>Free: {:.1} GB</p>
            <p>Growth Rate: {:.1} MB/day</p>
        </div>
        <div>
            <h3>Backups</h3>
            <p>Success: {}</p>
            <p>Failed: {}</p>
            <p>Verified: {}</p>
        </div>
        <div>
            <h3>Performance</h3>
            <p>CPU Usage: {:.1}%</p>
            <p>Memory: {} MB</p>
            <p>Dedup Ratio: {:.2}x</p>
        </div>
    </div>
</body>
</html>"#,
            report.timestamp.format("%Y-%m-%d %H:%M:%S"),
            report.period_start.format("%Y-%m-%d"),
            report.period_end.format("%Y-%m-%d"),
            report.summary.health_score,
            report.summary.total_size / (1024 * 1024 * 1024),
            report.summary.backup_points,
            report.details.storage_metrics.used_space as f64 / (1024.0 * 1024.0 * 1024.0),
            report.details.storage_metrics.free_space as f64 / (1024.0 * 1024.0 * 1024.0),
            report.details.storage_metrics.growth_rate,
            report.details.backup_metrics.successful_backups,
            report.details.backup_metrics.failed_backups,
            report.details.backup_metrics.verified_points,
            report.details.performance_metrics.cpu_usage,
            report.details.performance_metrics.memory_usage,
            report.details.storage_metrics.dedup_ratio,
        );

        Ok(html)
    }

    fn generate_pdf_report(&self, report: &SystemReport) -> Result<Vec<u8>> {
        // Basic PDF generation
        // In a real implementation, you would use a PDF library like printpdf
        let html = self.generate_html_report(report)?;
        Ok(html.into_bytes()) // Placeholder
    }

    fn generate_csv_report(&self, report: &SystemReport) -> Result<String> {
        use csv::WriterBuilder;
        let mut writer = WriterBuilder::new().from_writer(vec![]);

        // Write summary
        writer.write_record(&[
            "Timestamp",
            "Health Score",
            "Total Size",
            "Backup Points",
            "Alerts",
        ])?;

        writer.write_record(&[
            &report.timestamp.to_string(),
            &report.summary.health_score.to_string(),
            &report.summary.total_size.to_string(),
            &report.summary.backup_points.to_string(),
            &report.summary.alerts_count.to_string(),
        ])?;

        let csv_data = String::from_utf8(writer.into_inner()?)?;
        Ok(csv_data)
    }
}

impl ReportFormat {
    fn extension(&self) -> &'static str {
        match self {
            ReportFormat::JSON => "json",
            ReportFormat::HTML => "html",
            ReportFormat::PDF => "pdf",
            ReportFormat::CSV => "csv",
        }
    }
}
