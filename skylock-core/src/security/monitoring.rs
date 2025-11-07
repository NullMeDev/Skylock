use chrono::{DateTime, Utc, Duration};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use tokio::sync::RwLock;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyManagerMetrics {
    pub total_keys: usize,
    pub rotated_keys_last_24h: usize,
    pub failed_operations: usize,
    pub avg_key_age_days: f64,
    pub oldest_key_age_days: u64,
    pub keys_needing_rotation: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStatus {
    pub available_space_bytes: u64,
    pub total_space_bytes: u64,
    pub usage_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyManagerHealth {
    pub total_keys: usize,
    pub keys_needing_rotation: usize,
    pub storage_status: StorageStatus,
    pub last_successful_backup: Option<DateTime<Utc>>,
    pub consecutive_failures: u32,
}

#[derive(Debug)]
pub struct MetricsCollector {
    metrics: Arc<RwLock<KeyManagerMetrics>>,
    operation_log: Arc<RwLock<Vec<OperationLog>>>,
}

#[derive(Debug, Clone)]
struct OperationLog {
    timestamp: DateTime<Utc>,
    operation: String,
    success: bool,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(KeyManagerMetrics {
                total_keys: 0,
                rotated_keys_last_24h: 0,
                failed_operations: 0,
                avg_key_age_days: 0.0,
                oldest_key_age_days: 0,
                keys_needing_rotation: 0,
            })),
            operation_log: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn record_operation(&self, operation: String, success: bool) {
        let log_entry = OperationLog {
            timestamp: Utc::now(),
            operation,
            success,
        };

        let mut log = self.operation_log.write().await;
        log.push(log_entry);

        // Keep only last 1000 operations
        if log.len() > 1000 {
            log.remove(0);
        }

        // Update metrics
        let mut metrics = self.metrics.write().await;
        if !success {
            metrics.failed_operations += 1;
        }
    }

    pub async fn update_key_metrics(&self, key_ages: &[Duration]) {
        if key_ages.is_empty() {
            return;
        }

        let mut metrics = self.metrics.write().await;
        metrics.total_keys = key_ages.len();
        
        // Calculate average age
        let total_days: f64 = key_ages.iter()
            .map(|d| d.num_days() as f64)
            .sum();
        metrics.avg_key_age_days = total_days / key_ages.len() as f64;

        // Find oldest key
        metrics.oldest_key_age_days = key_ages.iter()
            .map(|d| d.num_days() as u64)
            .max()
            .unwrap_or(0);
    }

    pub async fn get_metrics(&self) -> KeyManagerMetrics {
        self.metrics.read().await.clone()
    }

    pub async fn get_recent_failures(&self) -> Vec<(DateTime<Utc>, String)> {
        let log = self.operation_log.read().await;
        log.iter()
            .filter(|entry| !entry.success)
            .map(|entry| (entry.timestamp, entry.operation.clone()))
            .collect()
    }
}