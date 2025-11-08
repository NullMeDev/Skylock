use thiserror::Error;
use skylock_core::{SkylockError, StorageErrorType, NetworkErrorType, SystemErrorType};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Service failure: {0}")]
    ServiceFailure(String),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Network error: {0:?}")]
    Network(NetworkErrorType),
    
    #[error("Storage error: {0:?}")]
    Storage(StorageErrorType),
    
    #[error("System error: {0:?}")]
    System(SystemErrorType),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),
    
    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<skylock_core::SkylockError> for Error {
    fn from(err: skylock_core::SkylockError) -> Self {
        match err {
            SkylockError::Storage(e) => Error::Storage(e),
            SkylockError::System(e) => Error::System(e),
            SkylockError::Network(e) => Error::Network(e),
            SkylockError::Io(e) => Error::Io(e),
            _ => Error::Other(format!("{:?}", err))
        }
    }
}

impl From<Error> for SkylockError {
    fn from(err: Error) -> Self {
        match err {
            Error::ServiceFailure(msg) => SkylockError::Generic(format!("Service failure: {}", msg)),
            Error::InvalidConfig(msg) => SkylockError::Config(msg),
            Error::Network(e) => SkylockError::Network(e),
            Error::Storage(e) => SkylockError::Storage(e),
            Error::System(e) => SkylockError::System(e),
            Error::Io(e) => SkylockError::Io(e),
            Error::UrlParse(e) => SkylockError::Config(format!("URL parse error: {}", e)),
            Error::Other(msg) => SkylockError::Other(msg),
        }
    }
}
