//! Audit Logger Implementation
//!
//! Provides async, non-blocking audit logging with configurable backends.

use super::events::{AuditEvent, EventSeverity};
use super::storage::AuditStorage;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// Configuration for the audit logger
#[derive(Debug, Clone)]
pub struct AuditLoggerConfig {
    /// Minimum severity level to log
    pub min_severity: EventSeverity,
    /// Buffer size for async logging
    pub buffer_size: usize,
    /// Whether to log to tracing as well
    pub also_trace: bool,
    /// Categories to include (empty = all)
    pub include_categories: Vec<String>,
    /// Categories to exclude
    pub exclude_categories: Vec<String>,
}

impl Default for AuditLoggerConfig {
    fn default() -> Self {
        Self {
            min_severity: EventSeverity::Info,
            buffer_size: 1000,
            also_trace: true,
            include_categories: Vec::new(),
            exclude_categories: Vec::new(),
        }
    }
}

impl AuditLoggerConfig {
    /// Create config that only logs warnings and above
    pub fn warnings_only() -> Self {
        Self {
            min_severity: EventSeverity::Warning,
            ..Default::default()
        }
    }
    
    /// Create config for security-focused logging
    pub fn security_focused() -> Self {
        Self {
            min_severity: EventSeverity::Info,
            include_categories: vec![
                "authentication".to_string(),
                "authorization".to_string(),
                "key_management".to_string(),
                "security".to_string(),
            ],
            ..Default::default()
        }
    }
}

/// Audit logger with async buffering
pub struct AuditLogger {
    config: AuditLoggerConfig,
    sender: mpsc::Sender<AuditEvent>,
    /// Handle to the background task (kept alive)
    _task_handle: tokio::task::JoinHandle<()>,
}

impl AuditLogger {
    /// Create a new audit logger with the given storage backend
    pub fn new<S: AuditStorage + Send + Sync + 'static>(
        config: AuditLoggerConfig,
        storage: S,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(config.buffer_size);
        
        let task_handle = tokio::spawn(Self::background_writer(
            receiver,
            Arc::new(storage),
            config.also_trace,
        ));
        
        Self {
            config,
            sender,
            _task_handle: task_handle,
        }
    }
    
    /// Log an audit event
    pub async fn log(&self, event: AuditEvent) {
        // Filter by severity
        if event.severity.level() < self.config.min_severity.level() {
            return;
        }
        
        // Filter by category
        if !self.config.include_categories.is_empty() 
            && !self.config.include_categories.contains(&event.category().to_string()) 
        {
            return;
        }
        
        if self.config.exclude_categories.contains(&event.category().to_string()) {
            return;
        }
        
        // Send to background writer
        if let Err(e) = self.sender.send(event).await {
            error!("Failed to send audit event to logger: {}", e);
        }
    }
    
    /// Log an event without waiting (fire and forget)
    pub fn log_sync(&self, event: AuditEvent) {
        // Filter by severity
        if event.severity.level() < self.config.min_severity.level() {
            return;
        }
        
        // Filter by category
        if !self.config.include_categories.is_empty() 
            && !self.config.include_categories.contains(&event.category().to_string()) 
        {
            return;
        }
        
        if self.config.exclude_categories.contains(&event.category().to_string()) {
            return;
        }
        
        // Try to send without blocking
        if let Err(e) = self.sender.try_send(event) {
            error!("Failed to send audit event (sync): {}", e);
        }
    }
    
    /// Background task that writes events to storage
    async fn background_writer<S: AuditStorage + Send + Sync>(
        mut receiver: mpsc::Receiver<AuditEvent>,
        storage: Arc<S>,
        also_trace: bool,
    ) {
        debug!("Audit logger background writer started");
        
        while let Some(event) = receiver.recv().await {
            // Also log to tracing if configured
            if also_trace {
                Self::trace_event(&event);
            }
            
            // Write to storage
            if let Err(e) = storage.write(&event).await {
                error!("Failed to write audit event to storage: {}", e);
            }
        }
        
        debug!("Audit logger background writer stopped");
    }
    
    /// Log event using tracing
    fn trace_event(event: &AuditEvent) {
        let json = event.to_json().unwrap_or_else(|_| "serialization_error".to_string());
        
        match event.severity {
            EventSeverity::Info => {
                info!(
                    target: "audit",
                    category = event.category(),
                    event_id = %event.id,
                    outcome = %event.outcome,
                    "{}",
                    json
                );
            }
            EventSeverity::Warning => {
                tracing::warn!(
                    target: "audit",
                    category = event.category(),
                    event_id = %event.id,
                    outcome = %event.outcome,
                    "{}",
                    json
                );
            }
            EventSeverity::Error => {
                error!(
                    target: "audit",
                    category = event.category(),
                    event_id = %event.id,
                    outcome = %event.outcome,
                    "{}",
                    json
                );
            }
            EventSeverity::Critical => {
                error!(
                    target: "audit",
                    category = event.category(),
                    event_id = %event.id,
                    outcome = %event.outcome,
                    severity = "CRITICAL",
                    "{}",
                    json
                );
            }
        }
    }
    
    /// Flush any buffered events
    pub async fn flush(&self) {
        // The channel will be drained by the background task
        // We just need to wait a bit for it to process
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

/// Builder for creating audit loggers
pub struct AuditLoggerBuilder {
    config: AuditLoggerConfig,
}

impl AuditLoggerBuilder {
    /// Create a new builder with default config
    pub fn new() -> Self {
        Self {
            config: AuditLoggerConfig::default(),
        }
    }
    
    /// Set minimum severity
    pub fn min_severity(mut self, severity: EventSeverity) -> Self {
        self.config.min_severity = severity;
        self
    }
    
    /// Set buffer size
    pub fn buffer_size(mut self, size: usize) -> Self {
        self.config.buffer_size = size;
        self
    }
    
    /// Enable/disable tracing output
    pub fn also_trace(mut self, enabled: bool) -> Self {
        self.config.also_trace = enabled;
        self
    }
    
    /// Include specific categories
    pub fn include_categories(mut self, categories: Vec<String>) -> Self {
        self.config.include_categories = categories;
        self
    }
    
    /// Exclude specific categories
    pub fn exclude_categories(mut self, categories: Vec<String>) -> Self {
        self.config.exclude_categories = categories;
        self
    }
    
    /// Build the logger with the given storage backend
    pub fn build<S: AuditStorage + Send + Sync + 'static>(self, storage: S) -> AuditLogger {
        AuditLogger::new(self.config, storage)
    }
}

impl Default for AuditLoggerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::events::{AuditEventType, AuditActor, EventOutcome};
    use super::super::storage::MemoryAuditStorage;
    
    #[tokio::test]
    async fn test_logger_creation() {
        let storage = MemoryAuditStorage::new();
        let _logger = AuditLoggerBuilder::new()
            .min_severity(EventSeverity::Info)
            .build(storage);
    }
    
    #[tokio::test]
    async fn test_event_logging() {
        let storage = MemoryAuditStorage::new();
        let storage_clone = storage.clone();
        
        let logger = AuditLoggerBuilder::new()
            .min_severity(EventSeverity::Info)
            .also_trace(false)
            .build(storage);
        
        let event = AuditEvent::new(
            AuditEventType::LoginAttempt {
                username: "testuser".to_string(),
                method: "password".to_string(),
            },
            AuditActor::user("testuser"),
            EventOutcome::Success,
        );
        
        logger.log(event).await;
        logger.flush().await;
        
        let events = storage_clone.get_events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].category(), "authentication");
    }
    
    #[tokio::test]
    async fn test_severity_filtering() {
        let storage = MemoryAuditStorage::new();
        let storage_clone = storage.clone();
        
        let logger = AuditLoggerBuilder::new()
            .min_severity(EventSeverity::Error)
            .also_trace(false)
            .build(storage);
        
        // This should be filtered out (Info < Error)
        let info_event = AuditEvent::new(
            AuditEventType::ServiceStart {
                service: "test".to_string(),
                version: "1.0".to_string(),
            },
            AuditActor::system(),
            EventOutcome::Success,
        );
        
        // This should be logged (Critical >= Error)
        let critical_event = AuditEvent::new(
            AuditEventType::IntrusionDetected {
                alert_type: "test".to_string(),
                details: "test".to_string(),
            },
            AuditActor::system(),
            EventOutcome::Success,
        );
        
        logger.log(info_event).await;
        logger.log(critical_event).await;
        logger.flush().await;
        
        let events = storage_clone.get_events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].category(), "security");
    }
}
