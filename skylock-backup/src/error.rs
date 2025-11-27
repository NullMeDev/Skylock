use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkylockError {
    #[error("Encryption error: {0}")]
    Encryption(String),
    #[error("Backup error: {0}")]
    Backup(String),
    
    #[error("Cryptography error: {0}")]
    Crypto(String),
    
    #[error("Compression error: {0}")]
    Compression(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Core error: {0}")]
    Core(#[from] skylock_core::SkylockError),

    #[error("Strip prefix error: {0}")]
    StripPrefix(#[from] std::path::StripPrefixError),

    #[cfg(windows)]
    #[error("Windows API error: {0}")]
    Windows(#[from] windows::core::Error),

    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, SkylockError>;
