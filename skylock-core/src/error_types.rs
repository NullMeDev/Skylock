use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc, Duration};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct SystemError {
    pub code: i32,
    pub message: String,
    pub source: String,
    pub timestamp: DateTime<Utc>,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub category: ErrorCategory,
    pub severity: ErrorSeverity,
    pub message: String,
    pub source: String,
    pub timestamp: DateTime<Utc>,
    pub id: Uuid,
    pub status: ErrorStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorStatus {
    New,
    InProgress,
    Handled,
    Failed,
}

impl Error {
    pub fn new(category: ErrorCategory, severity: ErrorSeverity, message: String, source: String) -> Self {
        Self {
            category,
            severity,
            message,
            source,
            timestamp: Utc::now(),
            id: Uuid::new_v4(),
            status: ErrorStatus::New,
        }
    }
    
    pub fn system(error_type: SystemErrorType, message: String, source: String) -> Self {
        Self::new(
            ErrorCategory::System(error_type),
            ErrorSeverity::High,
            message,
            source
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ErrorPattern {
    pub category: ErrorCategory,
    pub threshold: u32,
    pub time_window: Duration,
    pub action: RecoveryAction,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryAction {
    RestartComponent(String),
    RetryOperation(u32),
    SwitchBackupService,
    NotifyAdmin,
    TerminateProcess,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCategory {
    Network(NetworkErrorType),
    Storage(StorageErrorType),
    System(SystemErrorType),
    Security(SecurityErrorType),
    Sync(SyncErrorType),
    Resource(ResourceErrorType),
    Config(ConfigErrorType),
    Backup(BackupErrorType),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncErrorType {
    FileConflict,
    SyncFailed,
    MergeConflict,
    UnauthorizedAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StorageErrorType {
    SpaceExhausted,
    PathNotFound,
    PermissionDenied,
    WriteError,
    ReadError,
    DeleteError,
    QuotaExceeded,
    NetworkTimeout,
    AccessDenied,
    StorageBoxUnavailable,
    RateLimitExceeded,
    ReplicationError(String),
    FileNotFound,
    IOError(String),
    AuthenticationFailed,
    ConnectionFailed(String),
    /// Configuration error for storage provider
    ConfigError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkErrorType {
    ConnectionFailed,
    TimeoutError,
    InvalidResponse,
    ServerError,
    SSLError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SecurityErrorType {
    AuthenticationFailed,
    AccessDenied,
    TokenExpired,
    InvalidCredentials,
    EncryptionFailed,
    DecryptionFailed,
    RateLimitExceeded,
    IntegrityCheckFailed,
    KeyNotFound,
    KeyGenerationFailed,
    StorageError(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResourceErrorType {
    CPUOverload,
    MemoryExhausted,
    DiskSpaceLow,
    NetworkCongestion,
    ThreadPoolExhausted,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigErrorType {
    InvalidSetting,
    MissingRequired,
    ParseError,
    ValidationFailed,
    VersionMismatch,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BackupErrorType {
    BackupFailed,
    RestoreFailed,
    VerificationFailed,
    IncompleteBackup,
    CorruptBackup,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SystemErrorType {
    InternalError,
    InitializationError,
    ResourceExhausted,
    InvalidState,
    UnexpectedError,
    InvalidConfiguration,
}

impl std::fmt::Display for SystemErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SystemErrorType::InternalError => write!(f, "Internal system error"),
            SystemErrorType::InitializationError => write!(f, "System initialization error"),
            SystemErrorType::ResourceExhausted => write!(f, "System resources exhausted"),
            SystemErrorType::InvalidState => write!(f, "System in invalid state"),
            SystemErrorType::UnexpectedError => write!(f, "Unexpected system error"),
            SystemErrorType::InvalidConfiguration => write!(f, "Invalid system configuration"),
        }
    }
}

impl std::fmt::Display for StorageErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageErrorType::SpaceExhausted => write!(f, "Storage space exhausted"),
            StorageErrorType::PathNotFound => write!(f, "Path not found"),
            StorageErrorType::PermissionDenied => write!(f, "Permission denied"),
            StorageErrorType::WriteError => write!(f, "Write error"),
            StorageErrorType::ReadError => write!(f, "Read error"),
            StorageErrorType::DeleteError => write!(f, "Delete error"),
            StorageErrorType::QuotaExceeded => write!(f, "Quota exceeded"),
            StorageErrorType::NetworkTimeout => write!(f, "Network timeout"),
            StorageErrorType::AccessDenied => write!(f, "Access denied"),
            StorageErrorType::StorageBoxUnavailable => write!(f, "Storage box unavailable"),
            StorageErrorType::RateLimitExceeded => write!(f, "Rate limit exceeded"),
            StorageErrorType::ReplicationError(msg) => write!(f, "Replication error: {}", msg),
            StorageErrorType::FileNotFound => write!(f, "File not found"),
            StorageErrorType::IOError(msg) => write!(f, "IO error: {}", msg),
            StorageErrorType::AuthenticationFailed => write!(f, "Authentication failed"),
            StorageErrorType::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            StorageErrorType::ConfigError => write!(f, "Storage configuration error"),
        }
    }
}

impl std::fmt::Display for NetworkErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkErrorType::ConnectionFailed => write!(f, "Connection failed"),
            NetworkErrorType::TimeoutError => write!(f, "Timeout error"),
            NetworkErrorType::InvalidResponse => write!(f, "Invalid response"),
            NetworkErrorType::ServerError => write!(f, "Server error"),
            NetworkErrorType::SSLError => write!(f, "SSL error"),
        }
    }
}

impl std::fmt::Display for SecurityErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecurityErrorType::AuthenticationFailed => write!(f, "Authentication failed"),
            SecurityErrorType::AccessDenied => write!(f, "Access denied"),
            SecurityErrorType::TokenExpired => write!(f, "Token expired"),
            SecurityErrorType::InvalidCredentials => write!(f, "Invalid credentials"),
            SecurityErrorType::EncryptionFailed => write!(f, "Encryption failed"),
            SecurityErrorType::DecryptionFailed => write!(f, "Decryption failed"),
            SecurityErrorType::RateLimitExceeded => write!(f, "Rate limit exceeded"),
            SecurityErrorType::IntegrityCheckFailed => write!(f, "Integrity check failed"),
            SecurityErrorType::KeyNotFound => write!(f, "Key not found"),
            SecurityErrorType::KeyGenerationFailed => write!(f, "Key generation failed"),
            SecurityErrorType::StorageError(msg) => write!(f, "Storage error: {}", msg),
        }
    }
}

impl std::fmt::Display for SyncErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncErrorType::FileConflict => write!(f, "File conflict"),
            SyncErrorType::SyncFailed => write!(f, "Sync failed"),
            SyncErrorType::MergeConflict => write!(f, "Merge conflict"),
            SyncErrorType::UnauthorizedAccess => write!(f, "Unauthorized access"),
        }
    }
}

impl std::fmt::Display for BackupErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackupErrorType::BackupFailed => write!(f, "Backup failed"),
            BackupErrorType::RestoreFailed => write!(f, "Restore failed"),
            BackupErrorType::VerificationFailed => write!(f, "Verification failed"),
            BackupErrorType::IncompleteBackup => write!(f, "Incomplete backup"),
            BackupErrorType::CorruptBackup => write!(f, "Corrupt backup"),
        }
    }
}
