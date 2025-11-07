use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use std::time::Duration;
use std::collections::HashMap;
use metrics::{Counter, Gauge, Histogram};
use metrics_exporter_prometheus::PrometheusBuilder;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerformanceMetrics {
    pub timestamp: DateTime<Utc>,
    pub backup_duration: Duration,
    pub bytes_processed: u64,
    pub compression_ratio: f64,
    pub dedup_ratio: f64,
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
    pub concurrent_operations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationTiming {
    pub operation: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration: Option<Duration>,
}

pub struct PerformanceMonitor {
    metrics: Arc<RwLock<Vec<PerformanceMetrics>>>,
    operation_timings: Arc<RwLock<HashMap<String, OperationTiming>>>,
    bytes_processed_counter: Counter,
    compression_ratio_gauge: Gauge,
    dedup_ratio_gauge: Gauge,
    operation_duration_histogram: Histogram,
}

impl Clone for PerformanceMonitor {
    fn clone(&self) -> Self {
        Self {
            metrics: self.metrics.clone(),
            operation_timings: self.operation_timings.clone(),
            bytes_processed_counter: self.bytes_processed_counter.clone(),
            compression_ratio_gauge: self.compression_ratio_gauge.clone(),
            dedup_ratio_gauge: self.dedup_ratio_gauge.clone(),
            operation_duration_histogram: self.operation_duration_histogram.clone(),
        }
    }
}

impl PerformanceMonitor {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize Prometheus metrics exporter
        let builder = PrometheusBuilder::new();
        builder.install()?;

        Ok(Self {
            metrics: Arc::new(RwLock::new(Vec::new())),
            operation_timings: Arc::new(RwLock::new(HashMap::new())),
            bytes_processed_counter: metrics::counter!("skylock_bytes_processed_total"),
            compression_ratio_gauge: metrics::gauge!("skylock_compression_ratio"),
            dedup_ratio_gauge: metrics::gauge!("skylock_dedup_ratio"),
            operation_duration_histogram: metrics::histogram!("skylock_operation_duration_seconds"),
        })
    }

    pub async fn start_operation(&self, operation: &str) {
        let timing = OperationTiming {
            operation: operation.to_string(),
            start_time: Utc::now(),
            end_time: None,
            duration: None,
        };

        self.operation_timings.write().await
            .insert(operation.to_string(), timing);
    }

    pub async fn end_operation(&self, operation: &str) {
        let mut timings = self.operation_timings.write().await;
        if let Some(timing) = timings.get_mut(operation) {
            let end_time = Utc::now();
            timing.end_time = Some(end_time);
            timing.duration = Some(end_time.signed_duration_since(timing.start_time).to_std().unwrap());

            // Record operation duration in histogram
            if let Some(duration) = timing.duration {
                self.operation_duration_histogram.record(duration.as_secs_f64());
            }
        }
    }

    pub async fn record_metrics(&self, metrics: PerformanceMetrics) {
        // Update Prometheus metrics
        self.bytes_processed_counter.increment(metrics.bytes_processed);
        self.compression_ratio_gauge.set(metrics.compression_ratio);
        self.dedup_ratio_gauge.set(metrics.dedup_ratio);

        // Store metrics history
        self.metrics.write().await.push(metrics);
    }

    pub async fn get_operation_timing(&self, operation: &str) -> Option<OperationTiming> {
        self.operation_timings.read().await
            .get(operation)
            .cloned()
    }

    pub async fn get_metrics_history(&self) -> Vec<PerformanceMetrics> {
        self.metrics.read().await.clone()
    }

    pub async fn save_metrics(&self, path: &std::path::Path) -> std::io::Result<()> {
        let metrics = self.metrics.read().await;
        let json = serde_json::to_string_pretty(&*metrics)?;
        tokio::fs::write(path, json).await
    }

    pub async fn load_metrics(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = tokio::fs::read_to_string(path).await?;
        let loaded_metrics: Vec<PerformanceMetrics> = serde_json::from_str(&json)?;
        *self.metrics.write().await = loaded_metrics;
        Ok(())
    }

    pub async fn analyze_performance(&self) -> PerformanceAnalysis {
        let metrics = self.metrics.read().await;
        let timings = self.operation_timings.read().await;

        let mut analysis = PerformanceAnalysis::default();

        if !metrics.is_empty() {
            // Calculate averages
            let total_metrics = metrics.len() as f64;
            analysis.avg_compression_ratio = metrics.iter().map(|m| m.compression_ratio).sum::<f64>() / total_metrics;
            analysis.avg_dedup_ratio = metrics.iter().map(|m| m.dedup_ratio).sum::<f64>() / total_metrics;
            analysis.avg_backup_duration = Duration::from_secs_f64(
                metrics.iter().map(|m| m.backup_duration.as_secs_f64()).sum::<f64>() / total_metrics
            );

            // Find bottlenecks
            analysis.bottlenecks = self.identify_bottlenecks(&metrics);

            // Calculate operation timings
            for timing in timings.values() {
                if let Some(duration) = timing.duration {
                    analysis.operation_timings.push((timing.operation.clone(), duration));
                }
            }
            analysis.operation_timings.sort_by_key(|(_, duration)| *duration);
        }

        analysis
    }

    fn identify_bottlenecks(&self, metrics: &[PerformanceMetrics]) -> Vec<String> {
        let mut bottlenecks = Vec::new();

        // CPU usage threshold (80%)
        if metrics.iter().any(|m| m.cpu_usage > 80.0) {
            bottlenecks.push("High CPU usage".to_string());
        }

        // I/O threshold (100MB/s)
        const IO_THRESHOLD: u64 = 100 * 1024 * 1024;
        if metrics.iter().any(|m| m.io_write_bytes > IO_THRESHOLD) {
            bottlenecks.push("High disk write activity".to_string());
        }

        // Memory usage threshold (80% of system memory)
        if metrics.iter().any(|m| m.memory_usage > system_memory() * 8 / 10) {
            bottlenecks.push("High memory usage".to_string());
        }

        // Poor compression ratio (less than 0.5)
        if metrics.iter().any(|m| m.compression_ratio > 0.5) {
            bottlenecks.push("Ineffective compression".to_string());
        }

        bottlenecks
    }
}

#[derive(Debug, Default)]
pub struct PerformanceAnalysis {
    pub avg_compression_ratio: f64,
    pub avg_dedup_ratio: f64,
    pub avg_backup_duration: Duration,
    pub bottlenecks: Vec<String>,
    pub operation_timings: Vec<(String, Duration)>,
}

fn system_memory() -> u64 {
    match sys_info::mem_info() {
        Ok(mem) => mem.total * 1024,  // Convert KB to bytes
        Err(_) => 8 * 1024 * 1024 * 1024,  // Fallback to 8GB
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_performance_monitoring() -> Result<(), Box<dyn std::error::Error>> {
        let monitor = PerformanceMonitor::new()?;

        // Test operation timing
        monitor.start_operation("test_op").await;
        sleep(Duration::from_millis(100)).await;
        monitor.end_operation("test_op").await;

        let timing = monitor.get_operation_timing("test_op").await.unwrap();
        assert!(timing.duration.is_some());

        // Test metrics recording
        let metrics = PerformanceMetrics {
            timestamp: Utc::now(),
            backup_duration: Duration::from_secs(60),
            bytes_processed: 1024 * 1024,
            compression_ratio: 0.5,
            dedup_ratio: 0.3,
            cpu_usage: 50.0,
            memory_usage: 1024 * 1024 * 1024,
            io_read_bytes: 512 * 1024 * 1024,
            io_write_bytes: 256 * 1024 * 1024,
            concurrent_operations: 4,
        };

        monitor.record_metrics(metrics.clone()).await;

        let history = monitor.get_metrics_history().await;
        assert_eq!(history.len(), 1);

        // Test performance analysis
        let analysis = monitor.analyze_performance().await;
        assert_eq!(analysis.avg_compression_ratio, 0.5);
        assert_eq!(analysis.avg_dedup_ratio, 0.3);
        assert_eq!(analysis.avg_backup_duration, Duration::from_secs(60));

        Ok(())
    }
}
