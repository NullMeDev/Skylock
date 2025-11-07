use thiserror::Error;
use std::fmt;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum SecurityErrorType {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),
    
    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),
    
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    
    #[error("Invalid key: {0}")]
    InvalidKey(String),
    
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),
    
    #[error("Data corruption detected: {0}")]
    DataCorruption(String),
    
    #[error("Backup operation failed: {0}")]
    BackupFailed(String),
    
    #[error("Restore operation failed: {0}")]
    RestoreFailed(String),
    
    #[error("HSM operation failed: {0}")]
    HsmError(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
}

impl SecurityErrorType {
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::DataCorruption(_) => ErrorSeverity::Critical,
            Self::DecryptionFailed(_) => ErrorSeverity::High,
            Self::EncryptionFailed(_) => ErrorSeverity::High,
            Self::KeyNotFound(_) => ErrorSeverity::High,
            Self::InvalidKey(_) => ErrorSeverity::High,
            Self::HsmError(_) => ErrorSeverity::High,
            Self::BackupFailed(_) => ErrorSeverity::Medium,
            Self::RestoreFailed(_) => ErrorSeverity::Medium,
            Self::RateLimitExceeded(_) => ErrorSeverity::Low,
            Self::StorageError(_) => ErrorSeverity::Medium,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorSeverity {
    Critical,
    High,
    Medium,
    Low,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorSeverity::Critical => write!(f, "CRITICAL"),
            ErrorSeverity::High => write!(f, "HIGH"),
            ErrorSeverity::Medium => write!(f, "MEDIUM"),
            ErrorSeverity::Low => write!(f, "LOW"),
        }
    }
}

#[derive(Debug, Error)]
#[error("{severity} Security Error: {error_type} - {message} (Component: {component})")]
pub struct SecurityError {
    pub error_type: SecurityErrorType,
    pub severity: ErrorSeverity,
    pub message: String,
    pub component: String,
}