//! Audit Event Definitions
//!
//! Defines the types of security events that can be audited.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

/// Severity level of an audit event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventSeverity {
    /// Informational events (logins, normal operations)
    Info,
    /// Warning events (failed attempts, unusual patterns)
    Warning,
    /// Error events (failures, violations)
    Error,
    /// Critical events (security breaches, data loss)
    Critical,
}

impl EventSeverity {
    /// Get numeric value for comparison
    pub fn level(&self) -> u8 {
        match self {
            EventSeverity::Info => 0,
            EventSeverity::Warning => 1,
            EventSeverity::Error => 2,
            EventSeverity::Critical => 3,
        }
    }
}

impl std::fmt::Display for EventSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventSeverity::Info => write!(f, "INFO"),
            EventSeverity::Warning => write!(f, "WARNING"),
            EventSeverity::Error => write!(f, "ERROR"),
            EventSeverity::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Outcome of an audit event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EventOutcome {
    /// Operation succeeded
    Success,
    /// Operation failed
    Failure,
    /// Operation was blocked/denied
    Denied,
    /// Outcome is unknown or not applicable
    Unknown,
}

impl std::fmt::Display for EventOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventOutcome::Success => write!(f, "SUCCESS"),
            EventOutcome::Failure => write!(f, "FAILURE"),
            EventOutcome::Denied => write!(f, "DENIED"),
            EventOutcome::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

/// Type of audit event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuditEventType {
    // Authentication events
    /// User login attempt
    LoginAttempt {
        username: String,
        method: String,
    },
    /// User logout
    Logout {
        username: String,
    },
    /// Password change
    PasswordChange {
        username: String,
    },
    /// Session timeout
    SessionTimeout {
        session_id: String,
    },
    
    // Authorization events
    /// Access to a resource
    ResourceAccess {
        resource_type: String,
        resource_id: String,
        action: String,
    },
    /// Permission change
    PermissionChange {
        target_user: String,
        permission: String,
        granted: bool,
    },
    
    // Key management events
    /// Encryption key access
    KeyAccess {
        key_id: String,
        operation: String,
    },
    /// Key generation
    KeyGeneration {
        key_id: String,
        key_type: String,
    },
    /// Key rotation
    KeyRotation {
        old_key_id: String,
        new_key_id: String,
    },
    /// Key deletion
    KeyDeletion {
        key_id: String,
    },
    
    // Data events
    /// Backup creation
    BackupCreated {
        backup_id: String,
        file_count: usize,
        total_size: u64,
    },
    /// Backup restoration
    BackupRestored {
        backup_id: String,
        file_count: usize,
    },
    /// Backup deletion
    BackupDeleted {
        backup_id: String,
    },
    /// File encryption
    FileEncrypted {
        file_path: String,
        file_size: u64,
    },
    /// File decryption
    FileDecrypted {
        file_path: String,
        file_size: u64,
    },
    
    // Network events
    /// Network connection
    ConnectionEstablished {
        protocol: String,
        remote_host: String,
    },
    /// Network connection failed
    ConnectionFailed {
        protocol: String,
        remote_host: String,
        error: String,
    },
    /// Certificate validation
    CertificateValidation {
        host: String,
        result: String,
    },
    
    // Security events
    /// Rate limit triggered
    RateLimitTriggered {
        identifier: String,
        limit_type: String,
    },
    /// Account lockout
    AccountLockout {
        username: String,
        duration_secs: u64,
    },
    /// Security policy violation
    PolicyViolation {
        policy: String,
        description: String,
    },
    /// Intrusion detection alert
    IntrusionDetected {
        alert_type: String,
        details: String,
    },
    
    // Configuration events
    /// Configuration change
    ConfigurationChange {
        setting: String,
        old_value: Option<String>,
        new_value: String,
    },
    
    // System events
    /// Service start
    ServiceStart {
        service: String,
        version: String,
    },
    /// Service stop
    ServiceStop {
        service: String,
        reason: String,
    },
    /// Error occurred
    SystemError {
        component: String,
        error: String,
    },
    
    // Custom event
    /// Custom audit event
    Custom {
        event_name: String,
        details: HashMap<String, String>,
    },
}

impl AuditEventType {
    /// Get the category of this event type
    pub fn category(&self) -> &'static str {
        match self {
            AuditEventType::LoginAttempt { .. } |
            AuditEventType::Logout { .. } |
            AuditEventType::PasswordChange { .. } |
            AuditEventType::SessionTimeout { .. } => "authentication",
            
            AuditEventType::ResourceAccess { .. } |
            AuditEventType::PermissionChange { .. } => "authorization",
            
            AuditEventType::KeyAccess { .. } |
            AuditEventType::KeyGeneration { .. } |
            AuditEventType::KeyRotation { .. } |
            AuditEventType::KeyDeletion { .. } => "key_management",
            
            AuditEventType::BackupCreated { .. } |
            AuditEventType::BackupRestored { .. } |
            AuditEventType::BackupDeleted { .. } |
            AuditEventType::FileEncrypted { .. } |
            AuditEventType::FileDecrypted { .. } => "data",
            
            AuditEventType::ConnectionEstablished { .. } |
            AuditEventType::ConnectionFailed { .. } |
            AuditEventType::CertificateValidation { .. } => "network",
            
            AuditEventType::RateLimitTriggered { .. } |
            AuditEventType::AccountLockout { .. } |
            AuditEventType::PolicyViolation { .. } |
            AuditEventType::IntrusionDetected { .. } => "security",
            
            AuditEventType::ConfigurationChange { .. } => "configuration",
            
            AuditEventType::ServiceStart { .. } |
            AuditEventType::ServiceStop { .. } |
            AuditEventType::SystemError { .. } => "system",
            
            AuditEventType::Custom { .. } => "custom",
        }
    }
    
    /// Get the default severity for this event type
    pub fn default_severity(&self) -> EventSeverity {
        match self {
            // Informational events
            AuditEventType::Logout { .. } |
            AuditEventType::ServiceStart { .. } |
            AuditEventType::ServiceStop { .. } |
            AuditEventType::BackupCreated { .. } |
            AuditEventType::BackupRestored { .. } |
            AuditEventType::FileEncrypted { .. } |
            AuditEventType::FileDecrypted { .. } |
            AuditEventType::ConnectionEstablished { .. } |
            AuditEventType::Custom { .. } => EventSeverity::Info,
            
            // Warning events
            AuditEventType::LoginAttempt { .. } |
            AuditEventType::SessionTimeout { .. } |
            AuditEventType::ResourceAccess { .. } |
            AuditEventType::KeyAccess { .. } |
            AuditEventType::ConfigurationChange { .. } |
            AuditEventType::CertificateValidation { .. } => EventSeverity::Warning,
            
            // Error events
            AuditEventType::PasswordChange { .. } |
            AuditEventType::PermissionChange { .. } |
            AuditEventType::KeyGeneration { .. } |
            AuditEventType::KeyRotation { .. } |
            AuditEventType::BackupDeleted { .. } |
            AuditEventType::ConnectionFailed { .. } |
            AuditEventType::RateLimitTriggered { .. } |
            AuditEventType::SystemError { .. } => EventSeverity::Error,
            
            // Critical events
            AuditEventType::KeyDeletion { .. } |
            AuditEventType::AccountLockout { .. } |
            AuditEventType::PolicyViolation { .. } |
            AuditEventType::IntrusionDetected { .. } => EventSeverity::Critical,
        }
    }
}

/// Actor who performed the action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditActor {
    /// Actor type (user, service, system)
    pub actor_type: String,
    /// Actor identifier (username, service name)
    pub id: String,
    /// Additional attributes
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, String>,
}

impl AuditActor {
    /// Create a user actor
    pub fn user(username: impl Into<String>) -> Self {
        Self {
            actor_type: "user".to_string(),
            id: username.into(),
            attributes: HashMap::new(),
        }
    }
    
    /// Create a service actor
    pub fn service(service_name: impl Into<String>) -> Self {
        Self {
            actor_type: "service".to_string(),
            id: service_name.into(),
            attributes: HashMap::new(),
        }
    }
    
    /// Create a system actor
    pub fn system() -> Self {
        Self {
            actor_type: "system".to_string(),
            id: "skylock".to_string(),
            attributes: HashMap::new(),
        }
    }
    
    /// Add an attribute
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

/// Source information for the event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSource {
    /// IP address of the source
    pub ip: Option<IpAddr>,
    /// Port number
    pub port: Option<u16>,
    /// User agent or client identifier
    pub user_agent: Option<String>,
    /// Session ID if applicable
    pub session_id: Option<String>,
    /// Geographic location (if known)
    pub location: Option<String>,
}

impl AuditSource {
    /// Create an empty source
    pub fn empty() -> Self {
        Self {
            ip: None,
            port: None,
            user_agent: None,
            session_id: None,
            location: None,
        }
    }
    
    /// Create a source with IP address
    pub fn with_ip(ip: IpAddr) -> Self {
        Self {
            ip: Some(ip),
            ..Self::empty()
        }
    }
    
    /// Set the session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }
}

/// A complete audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unique event ID
    pub id: String,
    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,
    /// Event type and details
    pub event: AuditEventType,
    /// Severity of the event
    pub severity: EventSeverity,
    /// Outcome of the event
    pub outcome: EventOutcome,
    /// Actor who performed the action
    pub actor: AuditActor,
    /// Source of the event
    pub source: AuditSource,
    /// Additional context
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub context: HashMap<String, String>,
    /// Error message if outcome is failure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Duration of the operation in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

impl AuditEvent {
    /// Create a new audit event
    pub fn new(event: AuditEventType, actor: AuditActor, outcome: EventOutcome) -> Self {
        let severity = event.default_severity();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event,
            severity,
            outcome,
            actor,
            source: AuditSource::empty(),
            context: HashMap::new(),
            error_message: None,
            duration_ms: None,
        }
    }
    
    /// Set the severity
    pub fn with_severity(mut self, severity: EventSeverity) -> Self {
        self.severity = severity;
        self
    }
    
    /// Set the source
    pub fn with_source(mut self, source: AuditSource) -> Self {
        self.source = source;
        self
    }
    
    /// Add context
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }
    
    /// Set error message
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error_message = Some(error.into());
        self
    }
    
    /// Set duration
    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }
    
    /// Get the event category
    pub fn category(&self) -> &'static str {
        self.event.category()
    }
    
    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
    
    /// Convert to pretty JSON string
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_event_creation() {
        let event = AuditEvent::new(
            AuditEventType::LoginAttempt {
                username: "testuser".to_string(),
                method: "password".to_string(),
            },
            AuditActor::user("testuser"),
            EventOutcome::Success,
        );
        
        assert_eq!(event.category(), "authentication");
        assert!(!event.id.is_empty());
    }
    
    #[test]
    fn test_event_serialization() {
        let event = AuditEvent::new(
            AuditEventType::BackupCreated {
                backup_id: "backup_123".to_string(),
                file_count: 100,
                total_size: 1024 * 1024,
            },
            AuditActor::system(),
            EventOutcome::Success,
        );
        
        let json = event.to_json().unwrap();
        assert!(json.contains("backup_123"));
        assert!(json.contains("backup_created"));
    }
    
    #[test]
    fn test_severity_levels() {
        assert!(EventSeverity::Critical.level() > EventSeverity::Error.level());
        assert!(EventSeverity::Error.level() > EventSeverity::Warning.level());
        assert!(EventSeverity::Warning.level() > EventSeverity::Info.level());
    }
    
    #[test]
    fn test_actor_types() {
        let user = AuditActor::user("alice");
        assert_eq!(user.actor_type, "user");
        assert_eq!(user.id, "alice");
        
        let service = AuditActor::service("backup-daemon");
        assert_eq!(service.actor_type, "service");
        
        let system = AuditActor::system();
        assert_eq!(system.actor_type, "system");
    }
}
