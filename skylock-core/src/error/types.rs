use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorStatus {
    New,
    InProgress,
    Retrying,
    Resolved,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCategory {
    Security,
    System,
    Network,
    Storage,
    Backup,
    Sync,
    Config,
    Resource,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorRetryState {
    NotRetried,
    Retrying(u32),
    MaxRetriesReached,
    PermanentFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityErrorType {
    AuthenticationFailed,
    AuthorizationFailed,
    EncryptionFailed,
    DecryptionFailed,
    InvalidCredentials,
    InvalidToken,
    Unauthorized,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemErrorType {
    OutOfMemory,
    OutOfDisk,
    PermissionDenied,
    ProcessFailed,
    SystemUnavailable,
    InvalidConfiguration,
    InternalError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkErrorType {
    ConnectionFailed,
    Timeout,
    DnsResolutionFailed,
    InvalidUrl,
    ProxyError,
    SslError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageErrorType {
    FileNotFound,
    DirectoryNotFound,
    PermissionDenied,
    InsufficientSpace,
    CorruptedData,
    ReadError,
    WriteError,
    // Additional variants for Hetzner SFTP
    ConnectionFailed(String),
    StorageBoxUnavailable,
    AuthenticationFailed,
    AccessDenied,
    IOError,
    NetworkTimeout,
    RateLimitExceeded,
    QuotaExceeded,
    ReplicationError(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackupErrorType {
    SnapshotFailed,
    CompressionFailed,
    EncryptionFailed,
    UploadFailed,
    ValidationFailed,
    RestoreFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncErrorType {
    ConflictDetected,
    MergeError,
    LockTimeout,
    VersionMismatch,
    IndexCorruption,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigErrorType {
    InvalidFormat,
    MissingField,
    InvalidValue,
    FileNotFound,
    ParseError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceErrorType {
    NotFound,
    AlreadyExists,
    InUse,
    Locked,
    Corrupted,
}

/// Main error type for the Skylock system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    pub category: ErrorCategory,
    pub severity: ErrorSeverity,
    pub status: ErrorStatus,
    pub retry_state: ErrorRetryState,
    pub retry_count: u32,
    pub message: String,
    pub details: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub source: Option<String>,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.category, self.message)
    }
}

impl std::error::Error for Error {}

impl Error {
    pub fn new(category: ErrorCategory, severity: ErrorSeverity, message: String, source: String) -> Self {
        Self {
            category,
            severity,
            status: ErrorStatus::New,
            retry_state: ErrorRetryState::NotRetried,
            retry_count: 0,
            message,
            details: None,
            timestamp: chrono::Utc::now(),
            source: Some(source),
        }
    }

    pub fn with_details(mut self, details: String) -> Self {
        self.details = Some(details);
        self
    }

    pub fn is_retryable(&self) -> bool {
        !matches!(self.retry_state, ErrorRetryState::PermanentFailure | ErrorRetryState::MaxRetriesReached)
    }

    pub fn increment_retry_count(&mut self) {
        self.retry_count += 1;
        self.retry_state = ErrorRetryState::Retrying(self.retry_count);
    }

    pub fn mark_non_retryable(&mut self) {
        self.retry_state = ErrorRetryState::PermanentFailure;
    }

    pub fn security(error_type: SecurityErrorType, message: String) -> Self {
        Self::new(ErrorCategory::Security, ErrorSeverity::High, format!("{:?}: {}", error_type, message), "security".to_string())
    }

    pub fn system(error_type: SystemErrorType, message: String) -> Self {
        Self::new(ErrorCategory::System, ErrorSeverity::Medium, format!("{:?}: {}", error_type, message), "system".to_string())
    }

    pub fn network(error_type: NetworkErrorType, message: String) -> Self {
        Self::new(ErrorCategory::Network, ErrorSeverity::Medium, format!("{:?}: {}", error_type, message), "network".to_string())
    }

    pub fn storage(error_type: StorageErrorType, message: String) -> Self {
        Self::new(ErrorCategory::Storage, ErrorSeverity::Medium, format!("{:?}: {}", error_type, message), "storage".to_string())
    }

    pub fn backup(error_type: BackupErrorType, message: String) -> Self {
        Self::new(ErrorCategory::Backup, ErrorSeverity::High, format!("{:?}: {}", error_type, message), "backup".to_string())
    }

    pub fn sync(error_type: SyncErrorType, message: String) -> Self {
        Self::new(ErrorCategory::Sync, ErrorSeverity::Medium, format!("{:?}: {}", error_type, message), "sync".to_string())
    }

    pub fn config(error_type: ConfigErrorType, message: String) -> Self {
        Self::new(ErrorCategory::Config, ErrorSeverity::High, format!("{:?}: {}", error_type, message), "config".to_string())
    }

    pub fn resource(error_type: ResourceErrorType, message: String) -> Self {
        Self::new(ErrorCategory::Resource, ErrorSeverity::Medium, format!("{:?}: {}", error_type, message), "resource".to_string())
    }
}

// Display implementations for error types used in thiserror derive macros
impl fmt::Display for SecurityErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for SystemErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for NetworkErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for StorageErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for BackupErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for SyncErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for ConfigErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl fmt::Display for ResourceErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
