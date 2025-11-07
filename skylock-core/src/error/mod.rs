pub mod types;
pub use types::{
    Error,
    ErrorRetryState,
    SecurityErrorType,
    SystemErrorType,
    NetworkErrorType,
    StorageErrorType,
    BackupErrorType,
    SyncErrorType,
    ConfigErrorType,
    ResourceErrorType,
    ErrorSeverity,
    ErrorStatus,
    ErrorCategory,
};

pub mod handler;
pub use handler::{
    CanHandle,
    HandleError,
    ErrorHandlerRegistry,
    RetryHandler,
};

#[cfg(test)]
mod tests;

/// Type alias for Result with our Error type
pub type Result<T> = std::result::Result<T, Error>;
