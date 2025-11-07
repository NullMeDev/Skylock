use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use skylock_core::Result;
use std::collections::HashMap;
use tokio::sync::{broadcast, mpsc, RwLock};
use std::sync::Arc;
use std::path::PathBuf;
use tracing::{info, warn, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub timestamp: DateTime<Utc>,
    pub components: HashMap<String, ComponentStatus>,
    pub resources: SystemResources,
    pub alerts: Vec<Alert>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub name: String,
    pub status: Status,
    pub last_success: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub metrics: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemResources {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub disk_space: HashMap<PathBuf, DiskSpace>,
    pub network_stats: NetworkStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskSpace {
    pub total: u64,
    pub used: u64,
    pub available: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub current_upload_speed: f64,
    pub current_download_speed: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub timestamp: DateTime<Utc>,
    pub severity: AlertSeverity,
    pub component: String,
    pub message: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Running,
    Idle,
    Error,
    Disabled,
}

pub struct SystemMonitor {
    status: Arc<RwLock<SystemStatus>>,
    alert_tx: broadcast::Sender<Alert>,
    metric_tx: mpsc::Sender<MetricUpdate>,
    resource_monitor: ResourceMonitor,
}

#[derive(Debug, Clone)]
struct MetricUpdate {
    component: String,
    metric: String,
    value: f64,
}

impl SystemMonitor {
    pub async fn new() -> Result<Self> {
        let (alert_tx, _) = broadcast::channel(100);
        let (metric_tx, metric_rx) = mpsc::channel(100);

        let status = Arc::new(RwLock::new(SystemStatus {
            timestamp: Utc::now(),
            components: HashMap::new(),
            resources: SystemResources {
                cpu_usage: 0.0,
                memory_usage: 0.0,
                disk_space: HashMap::new(),
                network_stats: NetworkStats {
                    bytes_sent: 0,
                    bytes_received: 0,
                    current_upload_speed: 0.0,
                    current_download_speed: 0.0,
                },
            },
            alerts: Vec::new(),
        }));

        let resource_monitor = ResourceMonitor::new();
        let monitor = Self {
            status,
            alert_tx,
            metric_tx,
            resource_monitor,
        };

        monitor.start_metric_processor(metric_rx);
        monitor.start_resource_monitor();

        Ok(monitor)
    }

    pub fn subscribe_alerts(&self) -> broadcast::Receiver<Alert> {
        self.alert_tx.subscribe()
    }

    pub async fn report_status(&self, component: &str, new_status: Status) {
        let mut status = self.status.write().await;
        let component_status = status.components.entry(component.to_string())
            .or_insert_with(|| ComponentStatus {
                name: component.to_string(),
                status: Status::Idle,
                last_success: None,
                last_error: None,
                metrics: HashMap::new(),
            });

        component_status.status = new_status;
        if new_status != Status::Error {
            component_status.last_success = Some(Utc::now());
        }
    }

    pub async fn report_error(&self, component: &str, error: String) {
        let mut status = self.status.write().await;
        let component_status = status.components.entry(component.to_string())
            .or_insert_with(|| ComponentStatus {
                name: component.to_string(),
                status: Status::Error,
                last_success: None,
                last_error: None,
                metrics: HashMap::new(),
            });

        component_status.status = Status::Error;
        component_status.last_error = Some(error.clone());

        let alert = Alert {
            timestamp: Utc::now(),
            severity: AlertSeverity::Error,
            component: component.to_string(),
            message: error,
            details: None,
        };

        let _ = self.alert_tx.send(alert.clone());
        status.alerts.push(alert);
    }

    pub async fn update_metric(&self, component: &str, metric: &str, value: f64) {
        let _ = self.metric_tx.send(MetricUpdate {
            component: component.to_string(),
            metric: metric.to_string(),
            value,
        }).await;
    }

    pub async fn get_status(&self) -> SystemStatus {
        self.status.read().await.clone()
    }

    fn start_metric_processor(&self, mut metric_rx: mpsc::Receiver<MetricUpdate>) {
        let status = self.status.clone();
        tokio::spawn(async move {
            while let Some(update) = metric_rx.recv().await {
                let mut system_status = status.write().await;
                let component = system_status.components
                    .entry(update.component)
                    .or_insert_with(|| ComponentStatus {
                        name: update.component.clone(),
                        status: Status::Running,
                        last_success: None,
                        last_error: None,
                        metrics: HashMap::new(),
                    });

                component.metrics.insert(update.metric, update.value);
            }
        });
    }

    fn start_resource_monitor(&self) {
        let status = self.status.clone();
        let resource_monitor = self.resource_monitor.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;

                if let Ok(resources) = resource_monitor.collect_metrics().await {
                    let mut system_status = status.write().await;
                    system_status.resources = resources;
                    system_status.timestamp = Utc::now();
                }
            }
        });
    }

    pub async fn generate_daily_report(&self) -> Result<DailyReport> {
        let status = self.status.read().await;
        let report = DailyReport {
            date: Utc::now(),
            component_summaries: status.components.iter()
                .map(|(name, status)| ComponentSummary {
                    name: name.clone(),
                    uptime_percentage: self.calculate_uptime(status),
                    error_count: self.count_errors(name, &status.alerts),
                    metrics_summary: status.metrics.clone(),
                })
                .collect(),
            resource_usage: ResourceSummary {
                average_cpu: status.resources.cpu_usage,
                average_memory: status.resources.memory_usage,
                peak_network_usage: self.calculate_peak_network(&status.resources.network_stats),
                disk_usage: status.resources.disk_space.clone(),
            },
            alerts_summary: self.summarize_alerts(&status.alerts),
        };

        Ok(report)
    }

    fn calculate_uptime(&self, status: &ComponentStatus) -> f64 {
        // Calculate uptime percentage based on error periods
        // Implementation details...
        95.5 // Placeholder
    }

    fn count_errors(&self, component: &str, alerts: &[Alert]) -> usize {
        alerts.iter()
            .filter(|alert| alert.component == component &&
                   alert.severity == AlertSeverity::Error)
            .count()
    }

    fn calculate_peak_network(&self, stats: &NetworkStats) -> f64 {
        stats.current_upload_speed.max(stats.current_download_speed)
    }

    fn summarize_alerts(&self, alerts: &[Alert]) -> AlertsSummary {
        AlertsSummary {
            total: alerts.len(),
            by_severity: alerts.iter()
                .fold(HashMap::new(), |mut map, alert| {
                    *map.entry(alert.severity).or_insert(0) += 1;
                    map
                }),
            by_component: alerts.iter()
                .fold(HashMap::new(), |mut map, alert| {
                    *map.entry(alert.component.clone()).or_insert(0) += 1;
                    map
                }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyReport {
    pub date: DateTime<Utc>,
    pub component_summaries: Vec<ComponentSummary>,
    pub resource_usage: ResourceSummary,
    pub alerts_summary: AlertsSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSummary {
    pub name: String,
    pub uptime_percentage: f64,
    pub error_count: usize,
    pub metrics_summary: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSummary {
    pub average_cpu: f64,
    pub average_memory: f64,
    pub peak_network_usage: f64,
    pub disk_usage: HashMap<PathBuf, DiskSpace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertsSummary {
    pub total: usize,
    pub by_severity: HashMap<AlertSeverity, usize>,
    pub by_component: HashMap<String, usize>,
}
