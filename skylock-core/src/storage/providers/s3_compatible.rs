//! S3-Compatible Storage Providers
//!
//! Provides pre-configured settings for popular S3-compatible storage services.
//! All providers use the AWS S3 SDK with custom endpoints.
//!
//! Supported providers:
//! - MinIO (self-hosted)
//! - Wasabi
//! - DigitalOcean Spaces
//! - Linode Object Storage
//! - Cloudflare R2
//! - Scaleway Object Storage
//! - Vultr Object Storage
//! - OVH Object Storage (S3 interface)

use crate::storage::{StorageConfig, StorageProviderType};
use crate::{Result, SkylockError};
use crate::error_types::StorageErrorType;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Pre-configured S3-compatible provider
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum S3CompatibleProvider {
    /// MinIO - self-hosted S3-compatible storage
    MinIO,
    /// Wasabi - hot cloud storage
    Wasabi,
    /// DigitalOcean Spaces
    DigitalOcean,
    /// Linode Object Storage
    Linode,
    /// Cloudflare R2
    CloudflareR2,
    /// Scaleway Object Storage
    Scaleway,
    /// Vultr Object Storage
    Vultr,
    /// OVH Object Storage
    OVH,
    /// Custom provider with manual endpoint configuration
    Custom,
}

impl Default for S3CompatibleProvider {
    fn default() -> Self {
        S3CompatibleProvider::Custom
    }
}

/// S3-compatible provider configuration
#[derive(Debug, Clone)]
pub struct S3CompatibleConfig {
    /// The provider type
    pub provider: S3CompatibleProvider,
    /// Region code (provider-specific)
    pub region: String,
    /// Bucket name
    pub bucket_name: String,
    /// Access key ID
    pub access_key_id: String,
    /// Secret access key
    pub secret_access_key: String,
    /// Custom endpoint URL (for MinIO or custom providers)
    pub custom_endpoint: Option<String>,
    /// Account ID (for Cloudflare R2)
    pub account_id: Option<String>,
}

impl S3CompatibleConfig {
    /// Create configuration for MinIO
    ///
    /// # Arguments
    /// * `endpoint` - MinIO server URL (e.g., "http://localhost:9000")
    /// * `bucket_name` - Target bucket name
    /// * `access_key` - MinIO access key
    /// * `secret_key` - MinIO secret key
    pub fn minio(
        endpoint: &str,
        bucket_name: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Self {
        Self {
            provider: S3CompatibleProvider::MinIO,
            region: "us-east-1".to_string(), // MinIO doesn't use regions
            bucket_name: bucket_name.to_string(),
            access_key_id: access_key.to_string(),
            secret_access_key: secret_key.to_string(),
            custom_endpoint: Some(endpoint.to_string()),
            account_id: None,
        }
    }

    /// Create configuration for Wasabi
    ///
    /// # Arguments
    /// * `region` - Wasabi region (e.g., "us-east-1", "us-west-1", "eu-central-1", "ap-northeast-1")
    /// * `bucket_name` - Target bucket name
    /// * `access_key` - Wasabi access key
    /// * `secret_key` - Wasabi secret key
    pub fn wasabi(
        region: &str,
        bucket_name: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Self {
        Self {
            provider: S3CompatibleProvider::Wasabi,
            region: region.to_string(),
            bucket_name: bucket_name.to_string(),
            access_key_id: access_key.to_string(),
            secret_access_key: secret_key.to_string(),
            custom_endpoint: None,
            account_id: None,
        }
    }

    /// Create configuration for DigitalOcean Spaces
    ///
    /// # Arguments
    /// * `region` - DO region (e.g., "nyc3", "sfo3", "ams3", "sgp1", "fra1")
    /// * `bucket_name` - Space name
    /// * `access_key` - Spaces access key
    /// * `secret_key` - Spaces secret key
    pub fn digitalocean(
        region: &str,
        bucket_name: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Self {
        Self {
            provider: S3CompatibleProvider::DigitalOcean,
            region: region.to_string(),
            bucket_name: bucket_name.to_string(),
            access_key_id: access_key.to_string(),
            secret_access_key: secret_key.to_string(),
            custom_endpoint: None,
            account_id: None,
        }
    }

    /// Create configuration for Linode Object Storage
    ///
    /// # Arguments
    /// * `region` - Linode region (e.g., "us-east-1", "eu-central-1", "ap-south-1")
    /// * `bucket_name` - Bucket name
    /// * `access_key` - Linode access key
    /// * `secret_key` - Linode secret key
    pub fn linode(
        region: &str,
        bucket_name: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Self {
        Self {
            provider: S3CompatibleProvider::Linode,
            region: region.to_string(),
            bucket_name: bucket_name.to_string(),
            access_key_id: access_key.to_string(),
            secret_access_key: secret_key.to_string(),
            custom_endpoint: None,
            account_id: None,
        }
    }

    /// Create configuration for Cloudflare R2
    ///
    /// # Arguments
    /// * `account_id` - Cloudflare account ID
    /// * `bucket_name` - R2 bucket name
    /// * `access_key` - R2 access key ID
    /// * `secret_key` - R2 secret access key
    pub fn cloudflare_r2(
        account_id: &str,
        bucket_name: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Self {
        Self {
            provider: S3CompatibleProvider::CloudflareR2,
            region: "auto".to_string(), // R2 auto-selects region
            bucket_name: bucket_name.to_string(),
            access_key_id: access_key.to_string(),
            secret_access_key: secret_key.to_string(),
            custom_endpoint: None,
            account_id: Some(account_id.to_string()),
        }
    }

    /// Create configuration for Scaleway Object Storage
    ///
    /// # Arguments
    /// * `region` - Scaleway region (e.g., "fr-par", "nl-ams", "pl-waw")
    /// * `bucket_name` - Bucket name
    /// * `access_key` - Scaleway access key
    /// * `secret_key` - Scaleway secret key
    pub fn scaleway(
        region: &str,
        bucket_name: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Self {
        Self {
            provider: S3CompatibleProvider::Scaleway,
            region: region.to_string(),
            bucket_name: bucket_name.to_string(),
            access_key_id: access_key.to_string(),
            secret_access_key: secret_key.to_string(),
            custom_endpoint: None,
            account_id: None,
        }
    }

    /// Create configuration for Vultr Object Storage
    ///
    /// # Arguments
    /// * `region` - Vultr region (e.g., "ewr1", "sjc1", "ams1", "sgp1")
    /// * `bucket_name` - Bucket name
    /// * `access_key` - Vultr access key
    /// * `secret_key` - Vultr secret key
    pub fn vultr(
        region: &str,
        bucket_name: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Self {
        Self {
            provider: S3CompatibleProvider::Vultr,
            region: region.to_string(),
            bucket_name: bucket_name.to_string(),
            access_key_id: access_key.to_string(),
            secret_access_key: secret_key.to_string(),
            custom_endpoint: None,
            account_id: None,
        }
    }

    /// Create configuration for a custom S3-compatible provider
    ///
    /// # Arguments
    /// * `endpoint` - Custom endpoint URL
    /// * `region` - Region string (can be arbitrary for some providers)
    /// * `bucket_name` - Bucket name
    /// * `access_key` - Access key
    /// * `secret_key` - Secret key
    pub fn custom(
        endpoint: &str,
        region: &str,
        bucket_name: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Self {
        Self {
            provider: S3CompatibleProvider::Custom,
            region: region.to_string(),
            bucket_name: bucket_name.to_string(),
            access_key_id: access_key.to_string(),
            secret_access_key: secret_key.to_string(),
            custom_endpoint: Some(endpoint.to_string()),
            account_id: None,
        }
    }

    /// Get the endpoint URL for this provider
    pub fn endpoint_url(&self) -> Result<String> {
        match &self.provider {
            S3CompatibleProvider::MinIO => {
                self.custom_endpoint.clone()
                    .ok_or_else(|| SkylockError::Storage(StorageErrorType::ConfigError))
            }
            S3CompatibleProvider::Wasabi => {
                Ok(format!("https://s3.{}.wasabisys.com", self.region))
            }
            S3CompatibleProvider::DigitalOcean => {
                Ok(format!("https://{}.digitaloceanspaces.com", self.region))
            }
            S3CompatibleProvider::Linode => {
                Ok(format!("https://{}.linodeobjects.com", self.region))
            }
            S3CompatibleProvider::CloudflareR2 => {
                let account_id = self.account_id.as_ref()
                    .ok_or_else(|| SkylockError::Storage(StorageErrorType::ConfigError))?;
                Ok(format!("https://{}.r2.cloudflarestorage.com", account_id))
            }
            S3CompatibleProvider::Scaleway => {
                Ok(format!("https://s3.{}.scw.cloud", self.region))
            }
            S3CompatibleProvider::Vultr => {
                Ok(format!("https://{}.vultrobjects.com", self.region))
            }
            S3CompatibleProvider::OVH => {
                Ok(format!("https://s3.{}.cloud.ovh.net", self.region))
            }
            S3CompatibleProvider::Custom => {
                self.custom_endpoint.clone()
                    .ok_or_else(|| SkylockError::Storage(StorageErrorType::ConfigError))
            }
        }
    }

    /// Convert to standard StorageConfig for use with AWSStorageProvider
    pub fn to_storage_config(&self) -> Result<StorageConfig> {
        let endpoint = self.endpoint_url()?;
        
        info!(
            "Creating S3-compatible config: provider={:?}, region={}, endpoint={}",
            self.provider, self.region, endpoint
        );

        Ok(StorageConfig {
            provider: StorageProviderType::S3Compatible,
            bucket_name: Some(self.bucket_name.clone()),
            region: Some(self.region.clone()),
            endpoint: Some(endpoint),
            access_key_id: Some(self.access_key_id.clone()),
            secret_access_key: Some(self.secret_access_key.clone()),
            account_id: self.account_id.clone(),
            ..Default::default()
        })
    }

    /// Get provider-specific notes and limitations
    pub fn notes(&self) -> &'static str {
        match self.provider {
            S3CompatibleProvider::MinIO => {
                "MinIO: Self-hosted, fully S3-compatible. Supports versioning, lifecycle policies."
            }
            S3CompatibleProvider::Wasabi => {
                "Wasabi: No egress fees, no API request fees. 90-day minimum storage duration."
            }
            S3CompatibleProvider::DigitalOcean => {
                "DigitalOcean Spaces: Includes CDN. 250GB outbound transfer/month included."
            }
            S3CompatibleProvider::Linode => {
                "Linode: Object Storage is S3-compatible. Outbound transfer billed separately."
            }
            S3CompatibleProvider::CloudflareR2 => {
                "Cloudflare R2: Zero egress fees. Automatic multi-region replication."
            }
            S3CompatibleProvider::Scaleway => {
                "Scaleway: European cloud provider. GDPR-compliant data centers."
            }
            S3CompatibleProvider::Vultr => {
                "Vultr: Global presence. S3-compatible object storage."
            }
            S3CompatibleProvider::OVH => {
                "OVH: European cloud provider. S3 interface available."
            }
            S3CompatibleProvider::Custom => {
                "Custom: Configure your own S3-compatible endpoint."
            }
        }
    }
}

/// Provider information for display
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub website: &'static str,
    pub regions: &'static [&'static str],
}

/// Get information about all supported S3-compatible providers
pub fn list_providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo {
            name: "MinIO",
            description: "High-performance, S3-compatible object storage (self-hosted)",
            website: "https://min.io",
            regions: &["N/A (self-hosted)"],
        },
        ProviderInfo {
            name: "Wasabi",
            description: "Hot cloud storage with no egress fees",
            website: "https://wasabi.com",
            regions: &["us-east-1", "us-east-2", "us-west-1", "us-central-1", "eu-central-1", "eu-central-2", "eu-west-1", "eu-west-2", "ap-northeast-1", "ap-northeast-2", "ap-southeast-1", "ap-southeast-2"],
        },
        ProviderInfo {
            name: "DigitalOcean Spaces",
            description: "S3-compatible object storage with CDN",
            website: "https://www.digitalocean.com/products/spaces",
            regions: &["nyc3", "sfo3", "ams3", "sgp1", "fra1", "syd1"],
        },
        ProviderInfo {
            name: "Linode Object Storage",
            description: "S3-compatible object storage by Akamai",
            website: "https://www.linode.com/products/object-storage",
            regions: &["us-east-1", "eu-central-1", "ap-south-1", "us-southeast-1"],
        },
        ProviderInfo {
            name: "Cloudflare R2",
            description: "Zero egress fee object storage",
            website: "https://www.cloudflare.com/products/r2",
            regions: &["auto (global)"],
        },
        ProviderInfo {
            name: "Scaleway Object Storage",
            description: "European S3-compatible storage",
            website: "https://www.scaleway.com/en/object-storage",
            regions: &["fr-par", "nl-ams", "pl-waw"],
        },
        ProviderInfo {
            name: "Vultr Object Storage",
            description: "S3-compatible cloud storage",
            website: "https://www.vultr.com/products/object-storage",
            regions: &["ewr1", "sjc1", "ams1", "sgp1", "blr1"],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasabi_endpoint() {
        let config = S3CompatibleConfig::wasabi(
            "us-east-1",
            "my-bucket",
            "access_key",
            "secret_key",
        );
        assert_eq!(config.endpoint_url().unwrap(), "https://s3.us-east-1.wasabisys.com");
    }

    #[test]
    fn test_digitalocean_endpoint() {
        let config = S3CompatibleConfig::digitalocean(
            "nyc3",
            "my-space",
            "access_key",
            "secret_key",
        );
        assert_eq!(config.endpoint_url().unwrap(), "https://nyc3.digitaloceanspaces.com");
    }

    #[test]
    fn test_cloudflare_r2_endpoint() {
        let config = S3CompatibleConfig::cloudflare_r2(
            "abc123",
            "my-bucket",
            "access_key",
            "secret_key",
        );
        assert_eq!(config.endpoint_url().unwrap(), "https://abc123.r2.cloudflarestorage.com");
    }

    #[test]
    fn test_minio_custom_endpoint() {
        let config = S3CompatibleConfig::minio(
            "http://localhost:9000",
            "my-bucket",
            "minioadmin",
            "minioadmin",
        );
        assert_eq!(config.endpoint_url().unwrap(), "http://localhost:9000");
    }

    #[test]
    fn test_to_storage_config() {
        let config = S3CompatibleConfig::wasabi(
            "us-east-1",
            "my-bucket",
            "access_key",
            "secret_key",
        );
        let storage_config = config.to_storage_config().unwrap();
        
        assert_eq!(storage_config.bucket_name, Some("my-bucket".to_string()));
        assert_eq!(storage_config.region, Some("us-east-1".to_string()));
        assert_eq!(storage_config.endpoint, Some("https://s3.us-east-1.wasabisys.com".to_string()));
    }

    #[test]
    fn test_list_providers() {
        let providers = list_providers();
        assert!(providers.len() >= 7);
        assert!(providers.iter().any(|p| p.name == "Wasabi"));
        assert!(providers.iter().any(|p| p.name == "Cloudflare R2"));
    }
}
