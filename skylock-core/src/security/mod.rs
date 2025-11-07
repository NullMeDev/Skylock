mod types;
pub mod backup;
pub mod cloud_storage;
pub mod error;
pub mod hsm;
pub mod key_manager;
pub mod monitoring;
pub mod versioning;

// Re-export common types
pub use backup::BackupManager;
pub use cloud_storage::CloudStorageProvider;
#[cfg(feature = "aws-storage")]
pub use cloud_storage::S3StorageProvider;
pub use error::{SecurityError, SecurityErrorType, ErrorSeverity};
pub use hsm::{HsmProvider, SoftwareHsm};
pub use key_manager::{KeyManager, KeyMetadata, KeyRotationPolicy, KeyStatus};
pub use monitoring::{KeyManagerHealth, KeyManagerMetrics, MetricsCollector, StorageStatus};
pub use types::{KeyType, SecureKey, EncryptionEngine};
pub use versioning::{VersionedKey, KeyVersion, KeyVersioningPolicy, VersionMetadata};