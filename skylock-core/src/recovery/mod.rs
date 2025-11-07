use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};
use tracing::{info, warn, error};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ShutdownManager {
    shutdown_tx: mpsc::Sender<()>,
    shutdown_rx: mpsc::Receiver<()>,
    active_operations: Arc<Mutex<Vec<ActiveOperation>>>,
}

#[derive(Debug)]
struct ActiveOperation {
    id: String,
    operation_type: String,
    start_time: std::time::Instant,
}

impl ShutdownManager {
    pub fn new() -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        Self {
            shutdown_tx,
            shutdown_rx,
            active_operations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn register_operation(&self, id: &str, operation_type: &str) {
        let mut ops = self.active_operations.lock().await;
        ops.push(ActiveOperation {
            id: id.to_string(),
            operation_type: operation_type.to_string(),
            start_time: std::time::Instant::now(),
        });
    }

    pub async fn complete_operation(&self, id: &str) {
        let mut ops = self.active_operations.lock().await;
        if let Some(pos) = ops.iter().position(|op| op.id == id) {
            ops.remove(pos);
        }
    }

    pub async fn initiate_shutdown(&self) {
        info!("Initiating graceful shutdown");

        // Signal shutdown
        if let Err(e) = self.shutdown_tx.send(()).await {
            error!("Failed to send shutdown signal: {}", e);
        }

        // Wait for active operations to complete
        let timeout = Duration::from_secs(30);
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            let ops = self.active_operations.lock().await;
            if ops.is_empty() {
                info!("All operations completed, proceeding with shutdown");
                return;
            }

            for op in ops.iter() {
                info!(
                    "Waiting for operation to complete: {} ({}) running for {:?}",
                    op.id, op.operation_type, op.start_time.elapsed()
                );
            }

            drop(ops);
            sleep(Duration::from_secs(1)).await;
        }

        warn!("Shutdown timeout reached, some operations may be interrupted");
    }

    pub async fn wait_for_shutdown(&mut self) {
        let _ = self.shutdown_rx.recv().await;
    }
}

pub struct RecoveryManager {
    recovery_file: PathBuf,
}

impl RecoveryManager {
    pub fn new(recovery_file: PathBuf) -> Self {
        Self { recovery_file }
    }

    pub async fn save_recovery_point(&self, state: &RecoveryState) -> Result<()> {
        let json = serde_json::to_string(state)?;
        fs::write(&self.recovery_file, json).await?;
        Ok(())
    }

    pub async fn load_recovery_point(&self) -> Result<Option<RecoveryState>> {
        if !self.recovery_file.exists() {
            return Ok(None);
        }

        let json = fs::read_to_string(&self.recovery_file).await?;
        let state = serde_json::from_str(&json)?;
        Ok(Some(state))
    }

    pub async fn clear_recovery_point(&self) -> Result<()> {
        if self.recovery_file.exists() {
            fs::remove_file(&self.recovery_file).await?;
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct RecoveryState {
    pub timestamp: DateTime<Utc>,
    pub operation_type: String,
    pub progress: f64,
    pub affected_files: Vec<PathBuf>,
    pub error: Option<String>,
}
