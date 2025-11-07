use anyhow::{Result, Context};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use tracing::{info, warn, error, debug};
use tokio::time::sleep;
use uuid::Uuid;

use crate::crypto::CryptoSuite;
use crate::compression::CompressionEngine;
use crate::deduplication::DeduplicationEngine;

/// Comprehensive error handler with retry logic and recovery strategies
pub struct ErrorHandler {
    retry_policies: HashMap<String, RetryPolicy>,
    circuit_breakers: HashMap<String, CircuitBreaker>,
    error_history: Vec<ErrorEvent>,
    recovery_strategies: HashMap<String, Box<dyn RecoveryStrategy + Send + Sync>>,
    max_history_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: usize,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
    pub jitter_enabled: bool,
    pub retryable_errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    pub name: String,
    pub failure_threshold: usize,
    pub recovery_timeout: Duration,
    pub state: CircuitBreakerState,
    pub failure_count: usize,
    pub last_failure: Option<Instant>,
    pub last_success: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerState {
    Closed,   // Normal operation
    Open,     // Failing, reject requests
    HalfOpen, // Testing if service recovered
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEvent {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub error_type: String,
    pub message: String,
    pub component: String,
    pub operation: String,
    pub retry_count: usize,
    pub recovered: bool,
    pub recovery_strategy: Option<String>,
    pub context: HashMap<String, String>,
    pub severity: ErrorSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Critical,  // System cannot continue
    High,      // Major functionality affected
    Medium,    // Minor functionality affected  
    Low,       // Informational/warning
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryResult {
    pub success: bool,
    pub strategy_used: String,
    pub recovery_time_seconds: f64,
    pub actions_taken: Vec<String>,
    pub remaining_issues: Vec<String>,
}

/// Trait for implementing recovery strategies
pub trait RecoveryStrategy {
    /// Attempt to recover from an error
    async fn recover(&self, error_event: &ErrorEvent) -> Result<RecoveryResult>;
    
    /// Check if this strategy can handle the given error
    fn can_handle(&self, error_event: &ErrorEvent) -> bool;
    
    /// Priority of this strategy (higher = preferred)
    fn priority(&self) -> u32;
    
    /// Strategy name
    fn name(&self) -> String;
}

impl ErrorHandler {
    pub fn new() -> Self {
        let mut handler = Self {
            retry_policies: HashMap::new(),
            circuit_breakers: HashMap::new(),
            error_history: Vec::new(),
            recovery_strategies: HashMap::new(),
            max_history_size: 10000,
        };

        // Set up default policies
        handler.setup_default_policies();
        handler.setup_default_strategies();
        
        handler
    }

    /// Execute an operation with retry logic and error handling
    pub async fn execute_with_retry<F, T, E>(
        &mut self,
        operation_name: &str,
        component: &str,
        operation: F,
    ) -> Result<T, E>
    where
        F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>> + Send>> + Send,
        E: std::error::Error + Send + Sync + 'static,
        T: Send,
    {
        let circuit_breaker_key = format!("{}::{}", component, operation_name);
        
        // Check circuit breaker
        if !self.is_circuit_breaker_closed(&circuit_breaker_key) {
            return Err(anyhow::anyhow!("Circuit breaker open for {}", circuit_breaker_key).into());
        }

        let policy = self.get_retry_policy(component);
        let mut attempt = 0;
        let start_time = Instant::now();

        loop {
            attempt += 1;
            
            match operation().await {
                Ok(result) => {
                    // Success - reset circuit breaker
                    self.record_success(&circuit_breaker_key);
                    
                    if attempt > 1 {
                        info!("Operation {} succeeded after {} attempts", operation_name, attempt);
                    }
                    
                    return Ok(result);
                }
                Err(error) => {
                    let error_msg = error.to_string();
                    
                    // Record error event
                    let error_event = ErrorEvent {
                        id: Uuid::new_v4(),
                        timestamp: Utc::now(),
                        error_type: std::any::type_name::<E>().to_string(),
                        message: error_msg.clone(),
                        component: component.to_string(),
                        operation: operation_name.to_string(),
                        retry_count: attempt,
                        recovered: false,
                        recovery_strategy: None,
                        context: HashMap::new(),
                        severity: self.classify_error_severity(&error_msg),
                    };

                    self.add_error_event(error_event.clone());

                    // Check if error is retryable
                    if attempt >= policy.max_attempts || !self.is_retryable_error(&error_msg, &policy) {
                        error!("Operation {} failed after {} attempts: {}", operation_name, attempt, error_msg);
                        
                        // Record circuit breaker failure
                        self.record_failure(&circuit_breaker_key);
                        
                        // Attempt recovery
                        if let Some(recovery_result) = self.attempt_recovery(&error_event).await {
                            if recovery_result.success {
                                info!("Recovered from error using strategy: {}", recovery_result.strategy_used);
                                continue; // Retry after successful recovery
                            }
                        }
                        
                        return Err(error);
                    }

                    // Calculate delay with backoff
                    let delay = self.calculate_delay(attempt, &policy);
                    warn!("Operation {} failed (attempt {}), retrying in {:?}: {}", 
                          operation_name, attempt, delay, error_msg);
                    
                    sleep(delay).await;
                }
            }
        }
    }

    /// Add a custom recovery strategy
    pub fn add_recovery_strategy(&mut self, strategy: Box<dyn RecoveryStrategy + Send + Sync>) {
        let name = strategy.name();
        self.recovery_strategies.insert(name, strategy);
    }

    /// Get error statistics
    pub fn get_error_statistics(&self) -> ErrorStatistics {
        let now = Utc::now();
        let one_hour_ago = now - chrono::Duration::hours(1);
        let one_day_ago = now - chrono::Duration::days(1);

        let recent_errors: Vec<_> = self.error_history
            .iter()
            .filter(|e| e.timestamp > one_hour_ago)
            .collect();

        let daily_errors: Vec<_> = self.error_history
            .iter()
            .filter(|e| e.timestamp > one_day_ago)
            .collect();

        let mut error_types = HashMap::new();
        let mut component_errors = HashMap::new();
        let mut recovery_success_rate = 0.0;

        for event in &daily_errors {
            *error_types.entry(event.error_type.clone()).or_insert(0) += 1;
            *component_errors.entry(event.component.clone()).or_insert(0) += 1;
        }

        let recovered_count = daily_errors.iter().filter(|e| e.recovered).count();
        if !daily_errors.is_empty() {
            recovery_success_rate = recovered_count as f64 / daily_errors.len() as f64;
        }

        ErrorStatistics {
            total_errors: self.error_history.len(),
            errors_last_hour: recent_errors.len(),
            errors_last_day: daily_errors.len(),
            error_types,
            component_errors,
            recovery_success_rate,
            circuit_breakers_status: self.get_circuit_breaker_status(),
        }
    }

    /// Get recent error events
    pub fn get_recent_errors(&self, limit: usize) -> Vec<ErrorEvent> {
        self.error_history
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Health check for error handler
    pub fn health_check(&self) -> HealthStatus {
        let stats = self.get_error_statistics();
        let critical_errors = self.error_history
            .iter()
            .filter(|e| matches!(e.severity, ErrorSeverity::Critical))
            .filter(|e| e.timestamp > Utc::now() - chrono::Duration::minutes(5))
            .count();

        let status = if critical_errors > 0 {
            HealthLevel::Critical
        } else if stats.errors_last_hour > 10 {
            HealthLevel::Warning
        } else {
            HealthLevel::Healthy
        };

        HealthStatus {
            level: status,
            message: format!("{} errors in last hour, {} critical in last 5 minutes", 
                           stats.errors_last_hour, critical_errors),
            last_updated: Utc::now(),
            recovery_rate: stats.recovery_success_rate,
        }
    }

    // Private helper methods

    fn setup_default_policies(&mut self) {
        // Backup operations policy
        self.retry_policies.insert("backup".to_string(), RetryPolicy {
            max_attempts: 5,
            base_delay_ms: 1000,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
            jitter_enabled: true,
            retryable_errors: vec![
                "Connection".to_string(),
                "Timeout".to_string(),
                "TemporaryFailure".to_string(),
            ],
        });

        // Restoration policy
        self.retry_policies.insert("restore".to_string(), RetryPolicy {
            max_attempts: 3,
            base_delay_ms: 2000,
            max_delay_ms: 60000,
            backoff_multiplier: 1.5,
            jitter_enabled: true,
            retryable_errors: vec![
                "Connection".to_string(),
                "Timeout".to_string(),
            ],
        });

        // Network operations policy
        self.retry_policies.insert("network".to_string(), RetryPolicy {
            max_attempts: 10,
            base_delay_ms: 500,
            max_delay_ms: 15000,
            backoff_multiplier: 1.8,
            jitter_enabled: true,
            retryable_errors: vec![
                "Connection".to_string(),
                "Timeout".to_string(),
                "DNS".to_string(),
                "NetworkUnreachable".to_string(),
            ],
        });
    }

    fn setup_default_strategies(&mut self) {
        // Connection recovery strategy
        self.add_recovery_strategy(Box::new(ConnectionRecoveryStrategy));
        
        // Disk space recovery strategy
        self.add_recovery_strategy(Box::new(DiskSpaceRecoveryStrategy));
        
        // Corruption recovery strategy
        self.add_recovery_strategy(Box::new(CorruptionRecoveryStrategy));
        
        // Permission recovery strategy
        self.add_recovery_strategy(Box::new(PermissionRecoveryStrategy));
    }

    fn get_retry_policy(&self, component: &str) -> RetryPolicy {
        self.retry_policies.get(component)
            .cloned()
            .unwrap_or_else(|| RetryPolicy {
                max_attempts: 3,
                base_delay_ms: 1000,
                max_delay_ms: 10000,
                backoff_multiplier: 2.0,
                jitter_enabled: true,
                retryable_errors: vec!["Connection".to_string(), "Timeout".to_string()],
            })
    }

    fn is_retryable_error(&self, error_msg: &str, policy: &RetryPolicy) -> bool {
        policy.retryable_errors.iter().any(|pattern| error_msg.contains(pattern))
    }

    fn calculate_delay(&self, attempt: usize, policy: &RetryPolicy) -> Duration {
        let base_delay = Duration::from_millis(policy.base_delay_ms);
        let multiplier = policy.backoff_multiplier.powi((attempt - 1) as i32);
        let delay_ms = (policy.base_delay_ms as f64 * multiplier) as u64;
        let capped_delay = delay_ms.min(policy.max_delay_ms);
        
        let final_delay = if policy.jitter_enabled {
            let jitter = (rand::random::<f64>() * 0.1 + 0.95) * capped_delay as f64;
            jitter as u64
        } else {
            capped_delay
        };

        Duration::from_millis(final_delay)
    }

    fn classify_error_severity(&self, error_msg: &str) -> ErrorSeverity {
        if error_msg.contains("Critical") || error_msg.contains("Fatal") {
            ErrorSeverity::Critical
        } else if error_msg.contains("Corruption") || error_msg.contains("Security") {
            ErrorSeverity::High
        } else if error_msg.contains("Connection") || error_msg.contains("Timeout") {
            ErrorSeverity::Medium
        } else {
            ErrorSeverity::Low
        }
    }

    fn add_error_event(&mut self, event: ErrorEvent) {
        self.error_history.push(event);
        
        // Trim history if too large
        if self.error_history.len() > self.max_history_size {
            self.error_history.remove(0);
        }
    }

    async fn attempt_recovery(&mut self, error_event: &ErrorEvent) -> Option<RecoveryResult> {
        let mut applicable_strategies: Vec<_> = self.recovery_strategies
            .values()
            .filter(|strategy| strategy.can_handle(error_event))
            .collect();

        // Sort by priority (highest first)
        applicable_strategies.sort_by(|a, b| b.priority().cmp(&a.priority()));

        for strategy in applicable_strategies {
            info!("Attempting recovery with strategy: {}", strategy.name());
            
            match strategy.recover(error_event).await {
                Ok(result) => {
                    if result.success {
                        // Mark error as recovered
                        if let Some(event) = self.error_history.iter_mut().find(|e| e.id == error_event.id) {
                            event.recovered = true;
                            event.recovery_strategy = Some(result.strategy_used.clone());
                        }
                        
                        return Some(result);
                    }
                }
                Err(e) => {
                    warn!("Recovery strategy {} failed: {}", strategy.name(), e);
                }
            }
        }

        None
    }

    fn is_circuit_breaker_closed(&mut self, key: &str) -> bool {
        let breaker = self.circuit_breakers.entry(key.to_string())
            .or_insert_with(|| CircuitBreaker {
                name: key.to_string(),
                failure_threshold: 5,
                recovery_timeout: Duration::from_secs(60),
                state: CircuitBreakerState::Closed,
                failure_count: 0,
                last_failure: None,
                last_success: None,
            });

        match breaker.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                // Check if we should transition to half-open
                if let Some(last_failure) = breaker.last_failure {
                    if last_failure.elapsed() > breaker.recovery_timeout {
                        breaker.state = CircuitBreakerState::HalfOpen;
                        info!("Circuit breaker {} transitioning to half-open", key);
                        return true;
                    }
                }
                false
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }

    fn record_success(&mut self, key: &str) {
        if let Some(breaker) = self.circuit_breakers.get_mut(key) {
            breaker.failure_count = 0;
            breaker.last_success = Some(Instant::now());
            breaker.state = CircuitBreakerState::Closed;
        }
    }

    fn record_failure(&mut self, key: &str) {
        if let Some(breaker) = self.circuit_breakers.get_mut(key) {
            breaker.failure_count += 1;
            breaker.last_failure = Some(Instant::now());

            if breaker.failure_count >= breaker.failure_threshold {
                breaker.state = CircuitBreakerState::Open;
                warn!("Circuit breaker {} opened after {} failures", key, breaker.failure_count);
            }
        }
    }

    fn get_circuit_breaker_status(&self) -> HashMap<String, String> {
        self.circuit_breakers
            .iter()
            .map(|(name, breaker)| {
                let status = match breaker.state {
                    CircuitBreakerState::Closed => format!("Closed ({})", breaker.failure_count),
                    CircuitBreakerState::Open => "Open".to_string(),
                    CircuitBreakerState::HalfOpen => "Half-Open".to_string(),
                };
                (name.clone(), status)
            })
            .collect()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorStatistics {
    pub total_errors: usize,
    pub errors_last_hour: usize,
    pub errors_last_day: usize,
    pub error_types: HashMap<String, usize>,
    pub component_errors: HashMap<String, usize>,
    pub recovery_success_rate: f64,
    pub circuit_breakers_status: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthStatus {
    pub level: HealthLevel,
    pub message: String,
    pub last_updated: DateTime<Utc>,
    pub recovery_rate: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum HealthLevel {
    Healthy,
    Warning,
    Critical,
}

// Recovery Strategy Implementations

struct ConnectionRecoveryStrategy;

#[async_trait::async_trait]
impl RecoveryStrategy for ConnectionRecoveryStrategy {
    async fn recover(&self, error_event: &ErrorEvent) -> Result<RecoveryResult> {
        let start_time = Instant::now();
        let mut actions = Vec::new();

        // Wait for network to stabilize
        sleep(Duration::from_secs(5)).await;
        actions.push("Waited for network stabilization".to_string());

        // TODO: Add connection-specific recovery logic
        // - Reset connections
        // - Switch to backup endpoints
        // - Validate network connectivity

        Ok(RecoveryResult {
            success: true, // Placeholder
            strategy_used: self.name(),
            recovery_time_seconds: start_time.elapsed().as_secs_f64(),
            actions_taken: actions,
            remaining_issues: Vec::new(),
        })
    }

    fn can_handle(&self, error_event: &ErrorEvent) -> bool {
        error_event.message.contains("Connection") || 
        error_event.message.contains("Network") ||
        error_event.message.contains("Timeout")
    }

    fn priority(&self) -> u32 { 100 }

    fn name(&self) -> String { "ConnectionRecovery".to_string() }
}

struct DiskSpaceRecoveryStrategy;

#[async_trait::async_trait]
impl RecoveryStrategy for DiskSpaceRecoveryStrategy {
    async fn recover(&self, _error_event: &ErrorEvent) -> Result<RecoveryResult> {
        let start_time = Instant::now();
        let mut actions = Vec::new();

        // TODO: Implement disk space recovery
        // - Clean temporary files
        // - Compress old backups
        // - Move data to alternative storage

        Ok(RecoveryResult {
            success: false, // Placeholder
            strategy_used: self.name(),
            recovery_time_seconds: start_time.elapsed().as_secs_f64(),
            actions_taken: actions,
            remaining_issues: vec!["Manual disk space cleanup required".to_string()],
        })
    }

    fn can_handle(&self, error_event: &ErrorEvent) -> bool {
        error_event.message.contains("No space") || 
        error_event.message.contains("Disk full")
    }

    fn priority(&self) -> u32 { 80 }

    fn name(&self) -> String { "DiskSpaceRecovery".to_string() }
}

struct CorruptionRecoveryStrategy;

#[async_trait::async_trait]
impl RecoveryStrategy for CorruptionRecoveryStrategy {
    async fn recover(&self, _error_event: &ErrorEvent) -> Result<RecoveryResult> {
        let start_time = Instant::now();
        let mut actions = Vec::new();

        // TODO: Implement corruption recovery
        // - Verify backup integrity
        // - Restore from redundant copies
        // - Repair corrupted blocks

        Ok(RecoveryResult {
            success: false, // Placeholder
            strategy_used: self.name(),
            recovery_time_seconds: start_time.elapsed().as_secs_f64(),
            actions_taken: actions,
            remaining_issues: vec!["Manual data verification required".to_string()],
        })
    }

    fn can_handle(&self, error_event: &ErrorEvent) -> bool {
        error_event.message.contains("Corruption") || 
        error_event.message.contains("Checksum") ||
        error_event.message.contains("Integrity")
    }

    fn priority(&self) -> u32 { 90 }

    fn name(&self) -> String { "CorruptionRecovery".to_string() }
}

struct PermissionRecoveryStrategy;

#[async_trait::async_trait]
impl RecoveryStrategy for PermissionRecoveryStrategy {
    async fn recover(&self, _error_event: &ErrorEvent) -> Result<RecoveryResult> {
        let start_time = Instant::now();
        let mut actions = Vec::new();

        // TODO: Implement permission recovery
        // - Check file/directory permissions
        // - Attempt to fix common permission issues
        // - Suggest manual fixes

        Ok(RecoveryResult {
            success: false, // Placeholder
            strategy_used: self.name(),
            recovery_time_seconds: start_time.elapsed().as_secs_f64(),
            actions_taken: actions,
            remaining_issues: vec!["Manual permission adjustment required".to_string()],
        })
    }

    fn can_handle(&self, error_event: &ErrorEvent) -> bool {
        error_event.message.contains("Permission") || 
        error_event.message.contains("Access denied") ||
        error_event.message.contains("Forbidden")
    }

    fn priority(&self) -> u32 { 70 }

    fn name(&self) -> String { "PermissionRecovery".to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_error_handler_retry_logic() {
        let mut handler = ErrorHandler::new();
        
        let mut attempt_count = 0;
        let operation = || {
            attempt_count += 1;
            Box::pin(async move {
                if attempt_count < 3 {
                    Err(anyhow::anyhow!("Connection failed"))
                } else {
                    Ok("Success".to_string())
                }
            })
        };

        let result: Result<String, anyhow::Error> = handler
            .execute_with_retry("test_operation", "test_component", operation)
            .await;

        assert!(result.is_ok());
        assert_eq!(attempt_count, 3);
    }
}