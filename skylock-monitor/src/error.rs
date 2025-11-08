use skylock_core::{SkylockError, StorageErrorType, SystemErrorType, NetworkErrorType};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Watch error: {0}")]
    Watch(#[from] notify::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Storage error: {0:?}")]
    Storage(StorageErrorType),
    #[error("System error: {0:?}")]
    System(SystemErrorType),
    #[error("Network error: {0:?}")]
    Network(NetworkErrorType),
    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;



impl From<skylock_core::SkylockError> for Error {
    fn from(err: SkylockError) -> Self {
        match err {
            SkylockError::Storage(e) => Error::Storage(e),
            SkylockError::System(e) => Error::System(e),
            SkylockError::Network(e) => Error::Network(e),
            SkylockError::Io(e) => Error::Io(e),
            _ => Error::Other(format!("{:?}", err))
        }
    }
}

impl From<skylock_sync::Error> for Error {
    fn from(e: skylock_sync::Error) -> Self {
        // Map skylock_sync::Error to our Error type
        Error::Other(format!("Sync error: {}", e))
    }
}

impl From<Error> for SkylockError {
    fn from(err: Error) -> Self {
        match err {
            Error::Watch(e) => SkylockError::Other(format!("Watch error: {}", e)),
            Error::Storage(e) => SkylockError::Storage(e),
            Error::System(e) => SkylockError::System(e),
            Error::Network(e) => SkylockError::Network(e),
            Error::Io(e) => SkylockError::Io(e),
            Error::Other(s) => SkylockError::Other(s),
        }
    }
}
