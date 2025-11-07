#[cfg(feature = "aws-storage")]
mod aws;
#[cfg(feature = "azure-storage")]
mod azure;
#[cfg(feature = "gcp-storage")]
mod gcp;
#[cfg(feature = "backblaze-storage")]
mod backblaze;

mod local;
mod hetzner;

pub use local::LocalStorageProvider;
pub use hetzner::HetznerStorageProvider;

#[cfg(feature = "aws-storage")]
pub use aws::AWSStorageProvider;
#[cfg(feature = "azure-storage")]
pub use azure::AzureStorageProvider;
#[cfg(feature = "gcp-storage")]
pub use gcp::GCPStorageProvider;
#[cfg(feature = "backblaze-storage")]
pub use backblaze::BackblazeStorageProvider;