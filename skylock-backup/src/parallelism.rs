//! Dynamic Parallelism Controller
//!
//! Adaptively scales concurrent upload threads from 4-32 based on:
//! - Available CPU cores
//! - Network bandwidth utilization
//! - System memory pressure
//! - I/O throughput metrics
//!
//! Implements feedback-based control to optimize throughput while
//! preventing system resource exhaustion.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use tracing::{debug, info, warn};

/// Minimum number of concurrent uploads
const MIN_PARALLELISM: usize = 4;

/// Maximum number of concurrent uploads
const MAX_PARALLELISM: usize = 32;

/// Default starting parallelism
const DEFAULT_PARALLELISM: usize = 8;

/// How often to re-evaluate parallelism (in seconds)
const ADJUSTMENT_INTERVAL_SECS: u64 = 10;

/// Target CPU utilization (0.0 - 1.0)
const TARGET_CPU_UTILIZATION: f64 = 0.70;

/// Target bandwidth utilization when bandwidth is known (0.0 - 1.0)
const TARGET_BANDWIDTH_UTILIZATION: f64 = 0.85;

/// Memory pressure threshold to scale down (0.0 - 1.0)
const MEMORY_PRESSURE_THRESHOLD: f64 = 0.85;

/// Throughput metrics collected during uploads
#[derive(Debug, Default)]
pub struct ThroughputMetrics {
    /// Total bytes uploaded in current window
    bytes_uploaded: AtomicU64,
    /// Total uploads completed in current window
    uploads_completed: AtomicU64,
    /// Total upload errors in current window
    upload_errors: AtomicU64,
    /// Sum of upload latencies (ms) for averaging
    total_latency_ms: AtomicU64,
}

impl ThroughputMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful upload
    pub fn record_upload(&self, bytes: u64, latency_ms: u64) {
        self.bytes_uploaded.fetch_add(bytes, Ordering::Relaxed);
        self.uploads_completed.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ms.fetch_add(latency_ms, Ordering::Relaxed);
    }

    /// Record an upload error
    pub fn record_error(&self) {
        self.upload_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Get bytes per second throughput
    pub fn bytes_per_second(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs <= 0.0 {
            return 0.0;
        }
        self.bytes_uploaded.load(Ordering::Relaxed) as f64 / elapsed_secs
    }

    /// Get average upload latency in milliseconds
    pub fn average_latency_ms(&self) -> f64 {
        let completed = self.uploads_completed.load(Ordering::Relaxed);
        if completed == 0 {
            return 0.0;
        }
        self.total_latency_ms.load(Ordering::Relaxed) as f64 / completed as f64
    }

    /// Get error rate (0.0 - 1.0)
    pub fn error_rate(&self) -> f64 {
        let completed = self.uploads_completed.load(Ordering::Relaxed);
        let errors = self.upload_errors.load(Ordering::Relaxed);
        let total = completed + errors;
        if total == 0 {
            return 0.0;
        }
        errors as f64 / total as f64
    }

    /// Reset metrics for new window
    pub fn reset(&self) {
        self.bytes_uploaded.store(0, Ordering::Relaxed);
        self.uploads_completed.store(0, Ordering::Relaxed);
        self.upload_errors.store(0, Ordering::Relaxed);
        self.total_latency_ms.store(0, Ordering::Relaxed);
    }
}

/// System resource metrics
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    /// CPU utilization (0.0 - 1.0)
    pub cpu_utilization: f64,
    /// Memory utilization (0.0 - 1.0)
    pub memory_utilization: f64,
    /// Available memory in bytes
    pub available_memory_bytes: u64,
    /// Number of CPU cores
    pub cpu_cores: usize,
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self {
            cpu_utilization: 0.5,
            memory_utilization: 0.5,
            available_memory_bytes: 4 * 1024 * 1024 * 1024, // 4GB default
            cpu_cores: num_cpus(),
        }
    }
}

/// Get number of CPU cores
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

/// Collect current system metrics
#[cfg(target_os = "linux")]
fn collect_system_metrics() -> SystemMetrics {
    let cpu_cores = num_cpus();
    
    // Read /proc/stat for CPU utilization
    let cpu_utilization = read_cpu_utilization().unwrap_or(0.5);
    
    // Read /proc/meminfo for memory utilization
    let (memory_utilization, available_memory_bytes) = read_memory_info()
        .unwrap_or((0.5, 4 * 1024 * 1024 * 1024));
    
    SystemMetrics {
        cpu_utilization,
        memory_utilization,
        available_memory_bytes,
        cpu_cores,
    }
}

#[cfg(not(target_os = "linux"))]
fn collect_system_metrics() -> SystemMetrics {
    // On non-Linux platforms, use defaults with core detection
    SystemMetrics {
        cpu_utilization: 0.5,
        memory_utilization: 0.5,
        available_memory_bytes: 4 * 1024 * 1024 * 1024,
        cpu_cores: num_cpus(),
    }
}

/// Read CPU utilization from /proc/stat (Linux only)
#[cfg(target_os = "linux")]
fn read_cpu_utilization() -> Option<f64> {
    use std::fs;
    use std::sync::OnceLock;
    use std::sync::Mutex;
    
    // Store previous values for delta calculation
    static PREV_CPU: OnceLock<Mutex<(u64, u64)>> = OnceLock::new();
    let prev = PREV_CPU.get_or_init(|| Mutex::new((0, 0)));
    
    let content = fs::read_to_string("/proc/stat").ok()?;
    let first_line = content.lines().next()?;
    
    if !first_line.starts_with("cpu ") {
        return None;
    }
    
    let values: Vec<u64> = first_line
        .split_whitespace()
        .skip(1)
        .filter_map(|s| s.parse().ok())
        .collect();
    
    if values.len() < 4 {
        return None;
    }
    
    // user + nice + system + idle + iowait + irq + softirq
    let idle = values.get(3).copied().unwrap_or(0) + values.get(4).copied().unwrap_or(0);
    let total: u64 = values.iter().take(7).sum();
    
    let mut prev_guard = prev.lock().ok()?;
    let (prev_idle, prev_total) = *prev_guard;
    *prev_guard = (idle, total);
    
    if prev_total == 0 {
        return Some(0.5); // First read, return default
    }
    
    let idle_delta = idle.saturating_sub(prev_idle);
    let total_delta = total.saturating_sub(prev_total);
    
    if total_delta == 0 {
        return Some(0.5);
    }
    
    Some(1.0 - (idle_delta as f64 / total_delta as f64))
}

/// Read memory info from /proc/meminfo (Linux only)
#[cfg(target_os = "linux")]
fn read_memory_info() -> Option<(f64, u64)> {
    use std::fs;
    
    let content = fs::read_to_string("/proc/meminfo").ok()?;
    
    let mut total_kb = 0u64;
    let mut available_kb = 0u64;
    
    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = line
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        } else if line.starts_with("MemAvailable:") {
            available_kb = line
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        }
    }
    
    if total_kb == 0 {
        return None;
    }
    
    let utilization = 1.0 - (available_kb as f64 / total_kb as f64);
    let available_bytes = available_kb * 1024;
    
    Some((utilization, available_bytes))
}

/// Configuration for the parallelism controller
#[derive(Debug, Clone)]
pub struct ParallelismConfig {
    /// Minimum concurrent uploads
    pub min_parallelism: usize,
    /// Maximum concurrent uploads
    pub max_parallelism: usize,
    /// Starting parallelism level
    pub initial_parallelism: usize,
    /// How often to adjust (seconds)
    pub adjustment_interval_secs: u64,
    /// Target CPU utilization (0.0 - 1.0)
    pub target_cpu_utilization: f64,
    /// Target bandwidth utilization (0.0 - 1.0)
    pub target_bandwidth_utilization: f64,
    /// Known bandwidth limit (bytes/sec), if any
    pub known_bandwidth_limit: Option<u64>,
    /// Memory pressure threshold to scale down
    pub memory_pressure_threshold: f64,
}

impl Default for ParallelismConfig {
    fn default() -> Self {
        Self {
            min_parallelism: MIN_PARALLELISM,
            max_parallelism: MAX_PARALLELISM,
            initial_parallelism: DEFAULT_PARALLELISM,
            adjustment_interval_secs: ADJUSTMENT_INTERVAL_SECS,
            target_cpu_utilization: TARGET_CPU_UTILIZATION,
            target_bandwidth_utilization: TARGET_BANDWIDTH_UTILIZATION,
            known_bandwidth_limit: None,
            memory_pressure_threshold: MEMORY_PRESSURE_THRESHOLD,
        }
    }
}

impl ParallelismConfig {
    /// Create config with known bandwidth limit
    pub fn with_bandwidth_limit(mut self, bytes_per_sec: u64) -> Self {
        self.known_bandwidth_limit = Some(bytes_per_sec);
        self
    }
    
    /// Create config optimized for high-bandwidth connections
    pub fn high_bandwidth() -> Self {
        Self {
            min_parallelism: 8,
            max_parallelism: 32,
            initial_parallelism: 16,
            ..Default::default()
        }
    }
    
    /// Create config optimized for low-bandwidth connections
    pub fn low_bandwidth() -> Self {
        Self {
            min_parallelism: 2,
            max_parallelism: 8,
            initial_parallelism: 4,
            ..Default::default()
        }
    }
    
    /// Create config based on detected system resources
    pub fn auto_detect() -> Self {
        let cores = num_cpus();
        let metrics = collect_system_metrics();
        
        // Scale based on available resources
        let max_by_cores = (cores * 4).min(MAX_PARALLELISM);
        let max_by_memory = if metrics.available_memory_bytes > 8 * 1024 * 1024 * 1024 {
            MAX_PARALLELISM
        } else if metrics.available_memory_bytes > 4 * 1024 * 1024 * 1024 {
            24
        } else if metrics.available_memory_bytes > 2 * 1024 * 1024 * 1024 {
            16
        } else {
            8
        };
        
        let max_parallelism = max_by_cores.min(max_by_memory);
        let initial = (max_parallelism / 2).max(MIN_PARALLELISM);
        
        info!(
            "Auto-detected parallelism config: {} cores, {}GB available RAM -> max={}, initial={}",
            cores,
            metrics.available_memory_bytes / 1024 / 1024 / 1024,
            max_parallelism,
            initial
        );
        
        Self {
            max_parallelism,
            initial_parallelism: initial,
            ..Default::default()
        }
    }
}

/// Adaptive parallelism controller
/// 
/// Dynamically adjusts the number of concurrent uploads based on
/// system resource utilization and throughput metrics.
pub struct ParallelismController {
    /// Current parallelism level
    current_parallelism: AtomicUsize,
    /// Configuration
    config: ParallelismConfig,
    /// Throughput metrics for current window
    metrics: Arc<ThroughputMetrics>,
    /// Window start time
    window_start: RwLock<Instant>,
    /// Previous throughput for comparison
    prev_throughput: AtomicU64,
    /// Semaphore for controlling concurrency
    semaphore: Arc<RwLock<Arc<Semaphore>>>,
}

impl ParallelismController {
    /// Create a new parallelism controller with default config
    pub fn new() -> Self {
        Self::with_config(ParallelismConfig::default())
    }
    
    /// Create a new parallelism controller with custom config
    pub fn with_config(config: ParallelismConfig) -> Self {
        let initial = config.initial_parallelism
            .max(config.min_parallelism)
            .min(config.max_parallelism);
        
        Self {
            current_parallelism: AtomicUsize::new(initial),
            config,
            metrics: Arc::new(ThroughputMetrics::new()),
            window_start: RwLock::new(Instant::now()),
            prev_throughput: AtomicU64::new(0),
            semaphore: Arc::new(RwLock::new(Arc::new(Semaphore::new(initial)))),
        }
    }
    
    /// Create controller with auto-detected configuration
    pub fn auto_detect() -> Self {
        Self::with_config(ParallelismConfig::auto_detect())
    }
    
    /// Get current parallelism level
    pub fn current_parallelism(&self) -> usize {
        self.current_parallelism.load(Ordering::Relaxed)
    }
    
    /// Get the semaphore for controlling concurrency
    pub async fn semaphore(&self) -> Arc<Semaphore> {
        self.semaphore.read().await.clone()
    }
    
    /// Get metrics collector
    pub fn metrics(&self) -> Arc<ThroughputMetrics> {
        self.metrics.clone()
    }
    
    /// Record a successful upload
    pub fn record_upload(&self, bytes: u64, latency_ms: u64) {
        self.metrics.record_upload(bytes, latency_ms);
    }
    
    /// Record an upload error
    pub fn record_error(&self) {
        self.metrics.record_error();
    }
    
    /// Check if adjustment is needed and perform it
    /// 
    /// Should be called periodically (e.g., after each upload or on a timer)
    pub async fn maybe_adjust(&self) {
        let window_start = *self.window_start.read().await;
        let elapsed = window_start.elapsed();
        
        if elapsed < Duration::from_secs(self.config.adjustment_interval_secs) {
            return;
        }
        
        // Time to adjust
        self.adjust_parallelism(elapsed.as_secs_f64()).await;
        
        // Reset window
        *self.window_start.write().await = Instant::now();
        self.metrics.reset();
    }
    
    /// Force an immediate adjustment
    pub async fn force_adjust(&self) {
        let window_start = *self.window_start.read().await;
        let elapsed = window_start.elapsed();
        
        self.adjust_parallelism(elapsed.as_secs_f64()).await;
        
        // Reset window
        *self.window_start.write().await = Instant::now();
        self.metrics.reset();
    }
    
    /// Perform parallelism adjustment based on metrics
    async fn adjust_parallelism(&self, elapsed_secs: f64) {
        let current = self.current_parallelism.load(Ordering::Relaxed);
        let system = collect_system_metrics();
        
        // Calculate throughput
        let throughput = self.metrics.bytes_per_second(elapsed_secs);
        let prev_throughput = self.prev_throughput.load(Ordering::Relaxed) as f64;
        let error_rate = self.metrics.error_rate();
        let avg_latency = self.metrics.average_latency_ms();
        
        debug!(
            "Parallelism adjustment: current={}, throughput={:.2}MB/s, errors={:.1}%, latency={:.0}ms, CPU={:.1}%, mem={:.1}%",
            current,
            throughput / 1024.0 / 1024.0,
            error_rate * 100.0,
            avg_latency,
            system.cpu_utilization * 100.0,
            system.memory_utilization * 100.0
        );
        
        let new_parallelism = self.calculate_new_parallelism(
            current,
            throughput,
            prev_throughput,
            error_rate,
            &system,
        );
        
        if new_parallelism != current {
            info!(
                "Adjusting parallelism: {} -> {} (throughput: {:.2}MB/s, CPU: {:.1}%)",
                current,
                new_parallelism,
                throughput / 1024.0 / 1024.0,
                system.cpu_utilization * 100.0
            );
            
            self.set_parallelism(new_parallelism).await;
        }
        
        // Store throughput for next comparison
        self.prev_throughput.store(throughput as u64, Ordering::Relaxed);
    }
    
    /// Calculate new parallelism level based on metrics
    fn calculate_new_parallelism(
        &self,
        current: usize,
        throughput: f64,
        prev_throughput: f64,
        error_rate: f64,
        system: &SystemMetrics,
    ) -> usize {
        // Start with current value
        let mut new_value = current as f64;
        
        // Factor 1: Error rate - scale down if errors are high
        if error_rate > 0.05 {
            // More than 5% errors, scale down aggressively
            new_value *= 0.7;
            debug!("High error rate ({:.1}%), scaling down", error_rate * 100.0);
        } else if error_rate > 0.01 {
            // 1-5% errors, scale down slightly
            new_value *= 0.9;
        }
        
        // Factor 2: Memory pressure - scale down if memory is tight
        if system.memory_utilization > self.config.memory_pressure_threshold {
            new_value *= 0.8;
            debug!(
                "Memory pressure ({:.1}% > {:.1}%), scaling down",
                system.memory_utilization * 100.0,
                self.config.memory_pressure_threshold * 100.0
            );
        }
        
        // Factor 3: CPU utilization - try to hit target
        if system.cpu_utilization < self.config.target_cpu_utilization * 0.5 {
            // CPU is under-utilized, scale up
            new_value *= 1.2;
        } else if system.cpu_utilization > self.config.target_cpu_utilization * 1.2 {
            // CPU is over-utilized, scale down
            new_value *= 0.85;
        }
        
        // Factor 4: Bandwidth utilization (if known)
        if let Some(bandwidth_limit) = self.config.known_bandwidth_limit {
            let bandwidth_utilization = throughput / bandwidth_limit as f64;
            
            if bandwidth_utilization < self.config.target_bandwidth_utilization * 0.7 {
                // Not hitting bandwidth target, scale up
                new_value *= 1.15;
            } else if bandwidth_utilization > 0.95 {
                // Near bandwidth limit, don't increase further
                // Actually might want to scale down to reduce contention
                new_value *= 0.95;
            }
        }
        
        // Factor 5: Throughput trend - if throughput improved, try going higher
        if prev_throughput > 0.0 {
            let throughput_ratio = throughput / prev_throughput;
            
            if throughput_ratio > 1.1 {
                // Throughput improved significantly, try increasing more
                new_value *= 1.05;
            } else if throughput_ratio < 0.8 {
                // Throughput dropped, scale back
                new_value *= 0.9;
            }
        }
        
        // Clamp to configured bounds
        let new_parallelism = (new_value.round() as usize)
            .max(self.config.min_parallelism)
            .min(self.config.max_parallelism);
        
        // Don't change by more than 50% at once
        let max_change = (current / 2).max(2);
        let bounded = if new_parallelism > current {
            (current + max_change).min(new_parallelism)
        } else {
            current.saturating_sub(max_change).max(new_parallelism)
        };
        
        bounded
    }
    
    /// Set parallelism to a specific value
    async fn set_parallelism(&self, new_value: usize) {
        let clamped = new_value
            .max(self.config.min_parallelism)
            .min(self.config.max_parallelism);
        
        self.current_parallelism.store(clamped, Ordering::Relaxed);
        
        // Create new semaphore with updated permits
        // Existing permits will continue to work until released
        *self.semaphore.write().await = Arc::new(Semaphore::new(clamped));
    }
    
    /// Get configuration
    pub fn config(&self) -> &ParallelismConfig {
        &self.config
    }
}

impl Default for ParallelismController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = ParallelismConfig::default();
        assert_eq!(config.min_parallelism, MIN_PARALLELISM);
        assert_eq!(config.max_parallelism, MAX_PARALLELISM);
        assert_eq!(config.initial_parallelism, DEFAULT_PARALLELISM);
    }

    #[test]
    fn test_config_auto_detect() {
        let config = ParallelismConfig::auto_detect();
        assert!(config.min_parallelism >= 2);
        assert!(config.max_parallelism <= MAX_PARALLELISM);
        assert!(config.initial_parallelism >= config.min_parallelism);
        assert!(config.initial_parallelism <= config.max_parallelism);
    }

    #[test]
    fn test_throughput_metrics() {
        let metrics = ThroughputMetrics::new();
        
        // Record some uploads
        metrics.record_upload(1024, 100);
        metrics.record_upload(2048, 200);
        metrics.record_error();
        
        // Check metrics
        assert_eq!(metrics.bytes_per_second(1.0), 3072.0);
        assert_eq!(metrics.average_latency_ms(), 150.0);
        assert!((metrics.error_rate() - 0.333).abs() < 0.01);
        
        // Reset
        metrics.reset();
        assert_eq!(metrics.bytes_per_second(1.0), 0.0);
    }

    #[tokio::test]
    async fn test_controller_creation() {
        let controller = ParallelismController::new();
        assert_eq!(controller.current_parallelism(), DEFAULT_PARALLELISM);
        
        let sem = controller.semaphore().await;
        assert_eq!(sem.available_permits(), DEFAULT_PARALLELISM);
    }

    #[tokio::test]
    async fn test_controller_with_config() {
        let config = ParallelismConfig {
            min_parallelism: 2,
            max_parallelism: 16,
            initial_parallelism: 8,
            ..Default::default()
        };
        
        let controller = ParallelismController::with_config(config);
        assert_eq!(controller.current_parallelism(), 8);
    }

    #[test]
    fn test_calculate_new_parallelism_high_errors() {
        let config = ParallelismConfig::default();
        let controller = ParallelismController::with_config(config);
        
        let system = SystemMetrics::default();
        
        // High error rate should reduce parallelism
        let new_val = controller.calculate_new_parallelism(
            16,      // current
            1000.0,  // throughput
            1000.0,  // prev_throughput
            0.10,    // 10% error rate
            &system,
        );
        
        assert!(new_val < 16, "High errors should reduce parallelism");
    }

    #[test]
    fn test_calculate_new_parallelism_low_cpu() {
        let config = ParallelismConfig::default();
        let controller = ParallelismController::with_config(config);
        
        let system = SystemMetrics {
            cpu_utilization: 0.2, // Low CPU
            ..Default::default()
        };
        
        // Low CPU should increase parallelism
        let new_val = controller.calculate_new_parallelism(
            8,       // current
            1000.0,  // throughput
            1000.0,  // prev_throughput
            0.0,     // no errors
            &system,
        );
        
        assert!(new_val >= 8, "Low CPU should allow increasing parallelism");
    }

    #[test]
    fn test_calculate_new_parallelism_memory_pressure() {
        let config = ParallelismConfig::default();
        let controller = ParallelismController::with_config(config);
        
        let system = SystemMetrics {
            memory_utilization: 0.95, // High memory pressure
            ..Default::default()
        };
        
        // High memory should reduce parallelism
        let new_val = controller.calculate_new_parallelism(
            16,      // current
            1000.0,  // throughput
            1000.0,  // prev_throughput
            0.0,     // no errors
            &system,
        );
        
        assert!(new_val < 16, "Memory pressure should reduce parallelism");
    }

    #[tokio::test]
    async fn test_record_and_adjust() {
        let config = ParallelismConfig {
            adjustment_interval_secs: 0, // Immediate adjustment for testing
            ..Default::default()
        };
        
        let controller = ParallelismController::with_config(config);
        
        // Record some uploads
        for _ in 0..10 {
            controller.record_upload(1024 * 1024, 100);
        }
        
        // Force adjustment
        controller.force_adjust().await;
        
        // Parallelism should be adjusted (exact value depends on system metrics)
        let new_val = controller.current_parallelism();
        assert!(new_val >= MIN_PARALLELISM);
        assert!(new_val <= MAX_PARALLELISM);
    }

    #[test]
    fn test_bounds_enforcement() {
        let config = ParallelismConfig {
            min_parallelism: 4,
            max_parallelism: 16,
            initial_parallelism: 100, // Too high
            ..Default::default()
        };
        
        let controller = ParallelismController::with_config(config);
        assert_eq!(controller.current_parallelism(), 16); // Clamped to max
    }
}
