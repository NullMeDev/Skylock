use anyhow::Result;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use tracing::{info, warn, error, debug};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error_handler::{ErrorHandler, HealthStatus};

/// System monitoring and health management
pub struct SystemMonitor {
    metrics: RwLock<MetricsCollector>,
    health_checks: Vec<Box<dyn HealthCheck + Send + Sync>>,
    alert_thresholds: AlertThresholds,
    notification_channels: Vec<Box<dyn NotificationChannel + Send + Sync>>,
    monitoring_interval: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub timestamp: DateTime<Utc>,
    pub backup_metrics: BackupMetrics,
    pub storage_metrics: StorageMetrics,
    pub system_metrics: SystemResourceMetrics,
    pub error_metrics: ErrorMetrics,
    pub performance_metrics: PerformanceMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetrics {
    pub total_backups: u64,
    pub successful_backups_24h: u64,
    pub failed_backups_24h: u64,
    pub avg_backup_time_seconds: f64,
    pub last_backup_timestamp: Option<DateTime<Utc>>,
    pub next_scheduled_backup: Option<DateTime<Utc>>,
    pub total_files_backed_up: u64,
    pub total_bytes_backed_up: u64,
    pub compression_ratio: f64,
    pub deduplication_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMetrics {
    pub local_storage_used_bytes: u64,
    pub local_storage_available_bytes: u64,
    pub remote_storage_used_bytes: u64,
    pub remote_storage_available_bytes: u64,
    pub storage_health_status: String,
    pub connection_status: ConnectionStatus,
    pub last_verification_timestamp: Option<DateTime<Utc>>,
    pub integrity_check_results: IntegrityResults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemResourceMetrics {
    pub cpu_usage_percent: f64,
    pub memory_usage_bytes: u64,
    pub memory_available_bytes: u64,
    pub disk_io_read_bytes_per_sec: u64,
    pub disk_io_write_bytes_per_sec: u64,
    pub network_rx_bytes_per_sec: u64,
    pub network_tx_bytes_per_sec: u64,
    pub system_load_average: f64,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMetrics {
    pub total_errors: u64,
    pub errors_last_hour: u64,
    pub errors_last_24h: u64,
    pub critical_errors_last_hour: u64,
    pub error_rate_per_minute: f64,
    pub most_common_errors: Vec<(String, u64)>,
    pub recovery_success_rate: f64,
    pub circuit_breaker_states: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub backup_throughput_mbps: f64,
    pub restore_throughput_mbps: f64,
    pub compression_speed_mbps: f64,
    pub encryption_speed_mbps: f64,
    pub deduplication_speed_mbps: f64,
    pub avg_response_time_ms: f64,
    pub queue_depth: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatus {
    pub primary_endpoint: EndpointHealth,
    pub backup_endpoints: Vec<EndpointHealth>,
    pub last_connection_test: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointHealth {
    pub endpoint: String,
    pub status: String, // "healthy", "degraded", "down"
    pub latency_ms: Option<f64>,
    pub last_error: Option<String>,
    pub uptime_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityResults {
    pub last_check_timestamp: Option<DateTime<Utc>>,
    pub total_files_checked: u64,
    pub corrupted_files: u64,
    pub missing_files: u64,
    pub integrity_percentage: f64,
}

#[derive(Debug, Clone)]
pub struct AlertThresholds {
    pub max_error_rate_per_minute: f64,
    pub max_backup_failure_rate: f64,
    pub min_storage_available_gb: f64,
    pub max_cpu_usage_percent: f64,
    pub max_memory_usage_percent: f64,
    pub max_backup_time_hours: f64,
    pub min_integrity_percentage: f64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            max_error_rate_per_minute: 5.0,
            max_backup_failure_rate: 0.1, // 10%
            min_storage_available_gb: 10.0,
            max_cpu_usage_percent: 90.0,
            max_memory_usage_percent: 85.0,
            max_backup_time_hours: 12.0,
            min_integrity_percentage: 99.5,
        }
    }
}

/// Health check trait for system components
pub trait HealthCheck {
    async fn check_health(&self) -> Result<ComponentHealth>;
    fn component_name(&self) -> String;
    fn check_interval(&self) -> Duration;
}

/// Notification channel trait for alerts
pub trait NotificationChannel {
    async fn send_notification(&self, notification: &SystemNotification) -> Result<()>;
    fn channel_name(&self) -> String;
    fn supports_severity(&self, severity: &NotificationSeverity) -> bool;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub component: String,
    pub status: HealthLevel,
    pub message: String,
    pub last_check: DateTime<Utc>,
    pub response_time_ms: f64,
    pub details: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthLevel {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemNotification {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub severity: NotificationSeverity,
    pub component: String,
    pub title: String,
    pub message: String,
    pub details: HashMap<String, String>,
    pub resolution_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationSeverity {
    Info,
    Warning,
    Critical,
    Emergency,
}

pub struct MetricsCollector {
    current_metrics: SystemMetrics,
    historical_metrics: Vec<SystemMetrics>,
    max_history_size: usize,
}

impl SystemMonitor {
    pub fn new() -> Self {
        Self {
            metrics: RwLock::new(MetricsCollector::new()),
            health_checks: Vec::new(),
            alert_thresholds: AlertThresholds::default(),
            notification_channels: Vec::new(),
            monitoring_interval: Duration::from_secs(60),
        }
    }

    /// Add a health check for a system component
    pub fn add_health_check(&mut self, check: Box<dyn HealthCheck + Send + Sync>) {
        info!("Adding health check for component: {}", check.component_name());
        self.health_checks.push(check);
    }

    /// Add a notification channel
    pub fn add_notification_channel(&mut self, channel: Box<dyn NotificationChannel + Send + Sync>) {
        info!("Adding notification channel: {}", channel.channel_name());
        self.notification_channels.push(channel);
    }

    /// Start monitoring system
    pub async fn start_monitoring(&mut self) -> Result<()> {
        info!("Starting system monitoring with interval: {:?}", self.monitoring_interval);

        // Start metrics collection task
        let metrics_handle = tokio::spawn(async move {
            // Metrics collection loop would go here
            loop {
                // Collect and update metrics
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });

        // Start health check task
        let health_check_handle = self.start_health_checks().await;

        // Start alerting task
        let alerting_handle = self.start_alerting().await;

        // Wait for tasks (they run indefinitely)
        tokio::select! {
            _ = metrics_handle => info!("Metrics collection task ended"),
            _ = health_check_handle => info!("Health check task ended"),
            _ = alerting_handle => info!("Alerting task ended"),
        }

        Ok(())
    }

    /// Get current system metrics
    pub async fn get_current_metrics(&self) -> SystemMetrics {
        let metrics = self.metrics.read().await;
        metrics.current_metrics.clone()
    }

    /// Get historical metrics
    pub async fn get_historical_metrics(&self, hours: u32) -> Vec<SystemMetrics> {
        let metrics = self.metrics.read().await;
        let cutoff = Utc::now() - chrono::Duration::hours(hours as i64);
        
        metrics.historical_metrics
            .iter()
            .filter(|m| m.timestamp > cutoff)
            .cloned()
            .collect()
    }

    /// Perform comprehensive system health check
    pub async fn comprehensive_health_check(&self) -> Result<SystemHealthReport> {
        let mut component_healths = Vec::new();

        for check in &self.health_checks {
            match check.check_health().await {
                Ok(health) => component_healths.push(health),
                Err(e) => {
                    error!("Health check failed for {}: {}", check.component_name(), e);
                    component_healths.push(ComponentHealth {
                        component: check.component_name(),
                        status: HealthLevel::Critical,
                        message: format!("Health check failed: {}", e),
                        last_check: Utc::now(),
                        response_time_ms: 0.0,
                        details: HashMap::new(),
                    });
                }
            }
        }

        // Calculate overall system health
        let overall_status = self.calculate_overall_health(&component_healths);
        let metrics = self.get_current_metrics().await;

        Ok(SystemHealthReport {
            overall_status,
            component_healths,
            system_metrics: metrics,
            recommendations: self.generate_recommendations(&component_healths).await,
            timestamp: Utc::now(),
        })
    }

    /// Generate system status report
    pub async fn generate_status_report(&self) -> Result<StatusReport> {
        let metrics = self.get_current_metrics().await;
        let health_report = self.comprehensive_health_check().await?;
        
        let backup_status = self.analyze_backup_status(&metrics.backup_metrics);
        let storage_status = self.analyze_storage_status(&metrics.storage_metrics);
        let performance_status = self.analyze_performance(&metrics.performance_metrics);
        let security_status = self.analyze_security_status(&metrics.error_metrics);

        Ok(StatusReport {
            timestamp: Utc::now(),
            system_health: health_report.overall_status,
            backup_status,
            storage_status,
            performance_status,
            security_status,
            recent_activities: self.get_recent_activities().await,
            uptime_stats: self.calculate_uptime_stats(&metrics),
            resource_utilization: ResourceUtilization {
                cpu_usage: metrics.system_metrics.cpu_usage_percent,
                memory_usage: (metrics.system_metrics.memory_usage_bytes as f64 / 
                             (metrics.system_metrics.memory_usage_bytes + metrics.system_metrics.memory_available_bytes) as f64) * 100.0,
                disk_usage: self.calculate_disk_usage(&metrics.storage_metrics),
                network_activity: NetworkActivity {
                    rx_mbps: metrics.system_metrics.network_rx_bytes_per_sec as f64 / (1024.0 * 1024.0),
                    tx_mbps: metrics.system_metrics.network_tx_bytes_per_sec as f64 / (1024.0 * 1024.0),
                }
            },
        })
    }

    /// Update alert thresholds
    pub fn update_alert_thresholds(&mut self, thresholds: AlertThresholds) {
        info!("Updating alert thresholds");
        self.alert_thresholds = thresholds;
    }

    /// Get monitoring statistics
    pub async fn get_monitoring_stats(&self) -> MonitoringStatistics {
        let metrics = self.metrics.read().await;
        
        MonitoringStatistics {
            monitoring_uptime_seconds: 0, // TODO: Calculate actual uptime
            total_metrics_collected: metrics.historical_metrics.len() as u64,
            health_checks_performed: 0, // TODO: Track health check count
            alerts_generated: 0, // TODO: Track alert count
            last_successful_backup: metrics.current_metrics.backup_metrics.last_backup_timestamp,
            system_load_average: metrics.current_metrics.system_metrics.system_load_average,
            error_rate_trend: "stable".to_string(), // TODO: Calculate trend
        }
    }

    // Private helper methods

    async fn start_health_checks(&self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                // Health check loop would go here
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        })
    }

    async fn start_alerting(&self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                // Alerting loop would go here
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        })
    }

    fn calculate_overall_health(&self, component_healths: &[ComponentHealth]) -> HealthLevel {
        let mut critical_count = 0;
        let mut warning_count = 0;
        let mut healthy_count = 0;

        for health in component_healths {
            match health.status {
                HealthLevel::Critical => critical_count += 1,
                HealthLevel::Warning => warning_count += 1,
                HealthLevel::Healthy => healthy_count += 1,
                HealthLevel::Unknown => warning_count += 1,
            }
        }

        if critical_count > 0 {
            HealthLevel::Critical
        } else if warning_count > 0 {
            HealthLevel::Warning
        } else {
            HealthLevel::Healthy
        }
    }

    async fn generate_recommendations(&self, _component_healths: &[ComponentHealth]) -> Vec<String> {
        // TODO: Implement intelligent recommendation system
        vec![
            "Consider running backup verification".to_string(),
            "Monitor disk space usage".to_string(),
        ]
    }

    fn analyze_backup_status(&self, metrics: &BackupMetrics) -> BackupStatusAnalysis {
        let success_rate = if metrics.successful_backups_24h + metrics.failed_backups_24h > 0 {
            metrics.successful_backups_24h as f64 / 
            (metrics.successful_backups_24h + metrics.failed_backups_24h) as f64
        } else {
            1.0
        };

        let status = if success_rate < 0.9 {
            HealthLevel::Critical
        } else if success_rate < 0.95 {
            HealthLevel::Warning
        } else {
            HealthLevel::Healthy
        };

        BackupStatusAnalysis {
            status,
            success_rate,
            avg_backup_time: metrics.avg_backup_time_seconds,
            last_backup_age_hours: metrics.last_backup_timestamp
                .map(|ts| (Utc::now() - ts).num_hours() as f64)
                .unwrap_or(f64::INFINITY),
            compression_efficiency: metrics.compression_ratio,
            deduplication_efficiency: metrics.deduplication_ratio,
        }
    }

    fn analyze_storage_status(&self, metrics: &StorageMetrics) -> StorageStatusAnalysis {
        let local_usage_percent = if metrics.local_storage_available_bytes + metrics.local_storage_used_bytes > 0 {
            (metrics.local_storage_used_bytes as f64 / 
             (metrics.local_storage_used_bytes + metrics.local_storage_available_bytes) as f64) * 100.0
        } else {
            0.0
        };

        let status = if local_usage_percent > 95.0 {
            HealthLevel::Critical
        } else if local_usage_percent > 85.0 {
            HealthLevel::Warning
        } else {
            HealthLevel::Healthy
        };

        StorageStatusAnalysis {
            status,
            local_usage_percent,
            remote_usage_percent: 0.0, // TODO: Calculate remote usage
            integrity_score: metrics.integrity_check_results.integrity_percentage,
            connection_health: metrics.connection_status.primary_endpoint.status.clone(),
        }
    }

    fn analyze_performance(&self, metrics: &PerformanceMetrics) -> PerformanceAnalysis {
        let overall_throughput = (metrics.backup_throughput_mbps + metrics.restore_throughput_mbps) / 2.0;
        
        let status = if overall_throughput < 10.0 {
            HealthLevel::Warning
        } else {
            HealthLevel::Healthy
        };

        PerformanceAnalysis {
            status,
            overall_throughput_mbps: overall_throughput,
            avg_response_time_ms: metrics.avg_response_time_ms,
            queue_depth: metrics.queue_depth,
            bottleneck_analysis: self.identify_bottlenecks(metrics),
        }
    }

    fn analyze_security_status(&self, metrics: &ErrorMetrics) -> SecurityAnalysis {
        let status = if metrics.critical_errors_last_hour > 0 {
            HealthLevel::Critical
        } else if metrics.error_rate_per_minute > 1.0 {
            HealthLevel::Warning
        } else {
            HealthLevel::Healthy
        };

        SecurityAnalysis {
            status,
            error_rate: metrics.error_rate_per_minute,
            security_incidents: 0, // TODO: Track security-specific incidents
            encryption_status: "active".to_string(), // TODO: Verify encryption status
            access_anomalies: Vec::new(), // TODO: Detect access anomalies
        }
    }

    fn identify_bottlenecks(&self, _metrics: &PerformanceMetrics) -> Vec<String> {
        // TODO: Implement bottleneck analysis
        Vec::new()
    }

    fn calculate_disk_usage(&self, metrics: &StorageMetrics) -> f64 {
        if metrics.local_storage_used_bytes + metrics.local_storage_available_bytes > 0 {
            (metrics.local_storage_used_bytes as f64 / 
             (metrics.local_storage_used_bytes + metrics.local_storage_available_bytes) as f64) * 100.0
        } else {
            0.0
        }
    }

    fn calculate_uptime_stats(&self, metrics: &SystemMetrics) -> UptimeStats {
        UptimeStats {
            system_uptime_seconds: metrics.system_metrics.uptime_seconds,
            service_uptime_seconds: 0, // TODO: Track service uptime
            availability_percentage: 99.9, // TODO: Calculate actual availability
            mtbf_hours: 720.0, // TODO: Calculate mean time between failures
            mttr_minutes: 15.0, // TODO: Calculate mean time to recovery
        }
    }

    async fn get_recent_activities(&self) -> Vec<RecentActivity> {
        // TODO: Get actual recent activities from logs/events
        Vec::new()
    }
}

impl MetricsCollector {
    fn new() -> Self {
        Self {
            current_metrics: SystemMetrics::default(),
            historical_metrics: Vec::new(),
            max_history_size: 1000,
        }
    }
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self {
            timestamp: Utc::now(),
            backup_metrics: BackupMetrics::default(),
            storage_metrics: StorageMetrics::default(),
            system_metrics: SystemResourceMetrics::default(),
            error_metrics: ErrorMetrics::default(),
            performance_metrics: PerformanceMetrics::default(),
        }
    }
}

// Default implementations for metric types
impl Default for BackupMetrics {
    fn default() -> Self {
        Self {
            total_backups: 0,
            successful_backups_24h: 0,
            failed_backups_24h: 0,
            avg_backup_time_seconds: 0.0,
            last_backup_timestamp: None,
            next_scheduled_backup: None,
            total_files_backed_up: 0,
            total_bytes_backed_up: 0,
            compression_ratio: 0.0,
            deduplication_ratio: 0.0,
        }
    }
}

impl Default for StorageMetrics {
    fn default() -> Self {
        Self {
            local_storage_used_bytes: 0,
            local_storage_available_bytes: 0,
            remote_storage_used_bytes: 0,
            remote_storage_available_bytes: 0,
            storage_health_status: "unknown".to_string(),
            connection_status: ConnectionStatus::default(),
            last_verification_timestamp: None,
            integrity_check_results: IntegrityResults::default(),
        }
    }
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        Self {
            primary_endpoint: EndpointHealth::default(),
            backup_endpoints: Vec::new(),
            last_connection_test: Utc::now(),
        }
    }
}

impl Default for EndpointHealth {
    fn default() -> Self {
        Self {
            endpoint: "unknown".to_string(),
            status: "unknown".to_string(),
            latency_ms: None,
            last_error: None,
            uptime_percentage: 0.0,
        }
    }
}

impl Default for IntegrityResults {
    fn default() -> Self {
        Self {
            last_check_timestamp: None,
            total_files_checked: 0,
            corrupted_files: 0,
            missing_files: 0,
            integrity_percentage: 0.0,
        }
    }
}

impl Default for SystemResourceMetrics {
    fn default() -> Self {
        Self {
            cpu_usage_percent: 0.0,
            memory_usage_bytes: 0,
            memory_available_bytes: 0,
            disk_io_read_bytes_per_sec: 0,
            disk_io_write_bytes_per_sec: 0,
            network_rx_bytes_per_sec: 0,
            network_tx_bytes_per_sec: 0,
            system_load_average: 0.0,
            uptime_seconds: 0,
        }
    }
}

impl Default for ErrorMetrics {
    fn default() -> Self {
        Self {
            total_errors: 0,
            errors_last_hour: 0,
            errors_last_24h: 0,
            critical_errors_last_hour: 0,
            error_rate_per_minute: 0.0,
            most_common_errors: Vec::new(),
            recovery_success_rate: 0.0,
            circuit_breaker_states: HashMap::new(),
        }
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            backup_throughput_mbps: 0.0,
            restore_throughput_mbps: 0.0,
            compression_speed_mbps: 0.0,
            encryption_speed_mbps: 0.0,
            deduplication_speed_mbps: 0.0,
            avg_response_time_ms: 0.0,
            queue_depth: 0,
        }
    }
}

// Additional types for status reporting

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthReport {
    pub overall_status: HealthLevel,
    pub component_healths: Vec<ComponentHealth>,
    pub system_metrics: SystemMetrics,
    pub recommendations: Vec<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusReport {
    pub timestamp: DateTime<Utc>,
    pub system_health: HealthLevel,
    pub backup_status: BackupStatusAnalysis,
    pub storage_status: StorageStatusAnalysis,
    pub performance_status: PerformanceAnalysis,
    pub security_status: SecurityAnalysis,
    pub recent_activities: Vec<RecentActivity>,
    pub uptime_stats: UptimeStats,
    pub resource_utilization: ResourceUtilization,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStatusAnalysis {
    pub status: HealthLevel,
    pub success_rate: f64,
    pub avg_backup_time: f64,
    pub last_backup_age_hours: f64,
    pub compression_efficiency: f64,
    pub deduplication_efficiency: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStatusAnalysis {
    pub status: HealthLevel,
    pub local_usage_percent: f64,
    pub remote_usage_percent: f64,
    pub integrity_score: f64,
    pub connection_health: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceAnalysis {
    pub status: HealthLevel,
    pub overall_throughput_mbps: f64,
    pub avg_response_time_ms: f64,
    pub queue_depth: u64,
    pub bottleneck_analysis: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAnalysis {
    pub status: HealthLevel,
    pub error_rate: f64,
    pub security_incidents: u64,
    pub encryption_status: String,
    pub access_anomalies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUtilization {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub disk_usage: f64,
    pub network_activity: NetworkActivity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkActivity {
    pub rx_mbps: f64,
    pub tx_mbps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UptimeStats {
    pub system_uptime_seconds: u64,
    pub service_uptime_seconds: u64,
    pub availability_percentage: f64,
    pub mtbf_hours: f64, // Mean time between failures
    pub mttr_minutes: f64, // Mean time to recovery
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentActivity {
    pub timestamp: DateTime<Utc>,
    pub activity_type: String,
    pub description: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MonitoringStatistics {
    pub monitoring_uptime_seconds: u64,
    pub total_metrics_collected: u64,
    pub health_checks_performed: u64,
    pub alerts_generated: u64,
    pub last_successful_backup: Option<DateTime<Utc>>,
    pub system_load_average: f64,
    pub error_rate_trend: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_system_monitor_creation() {
        let monitor = SystemMonitor::new();
        let metrics = monitor.get_current_metrics().await;
        assert_eq!(metrics.backup_metrics.total_backups, 0);
    }
}