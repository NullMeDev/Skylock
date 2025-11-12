//! TLS Certificate Pinning for WebDAV
//! 
//! Provides optional SPKI (Subject Public Key Info) pinning to prevent MITM attacks
//! even if an attacker compromises a Certificate Authority.

use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use anyhow::{Result, anyhow};

/// TLS Pinning configuration
#[derive(Debug, Clone)]
pub struct TlsPinningConfig {
    /// Base64-encoded SHA-256 hash of expected server SPKI
    pub spki_hash: Option<String>,
    /// Whether to enforce TLS 1.3 only
    pub tls_13_only: bool,
    /// Whether pinning is strict (fail if no match) or advisory (warn only)
    pub strict_mode: bool,
}

impl Default for TlsPinningConfig {
    fn default() -> Self {
        Self {
            spki_hash: None,
            tls_13_only: true,  // Default to TLS 1.3 for security
            strict_mode: false,  // Default to advisory mode to avoid breaking existing setups
        }
    }
}

impl TlsPinningConfig {
    /// Create a new pinning config with strict mode enabled
    pub fn strict(spki_hash: String) -> Self {
        Self {
            spki_hash: Some(spki_hash),
            tls_13_only: true,
            strict_mode: true,
        }
    }
    
    /// Create a new pinning config in advisory mode (warns but doesn't fail)
    pub fn advisory(spki_hash: String) -> Self {
        Self {
            spki_hash: Some(spki_hash),
            tls_13_only: true,
            strict_mode: false,
        }
    }
}

/// Compute SHA-256 hash of SPKI and return base64-encoded string
pub fn compute_spki_hash(spki_der: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(spki_der);
    STANDARD.encode(hasher.finalize())
}

/// Verify SPKI hash matches expected value
pub fn verify_spki_hash(spki_der: &[u8], expected: &str) -> Result<bool> {
    let computed = compute_spki_hash(spki_der);
    Ok(computed == expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_spki_hash_computation() {
        let test_spki = b"test_subject_public_key_info";
        let hash = compute_spki_hash(test_spki);
        
        // Should be base64-encoded SHA-256 (44 chars)
        assert_eq!(hash.len(), 44);
        
        // Same input should produce same hash
        let hash2 = compute_spki_hash(test_spki);
        assert_eq!(hash, hash2);
    }
    
    #[test]
    fn test_spki_verification() {
        let test_spki = b"test_subject_public_key_info";
        let expected = compute_spki_hash(test_spki);
        
        assert!(verify_spki_hash(test_spki, &expected).unwrap());
        assert!(!verify_spki_hash(test_spki, "wrong_hash").unwrap());
    }
    
    #[test]
    fn test_default_config() {
        let config = TlsPinningConfig::default();
        assert!(config.spki_hash.is_none());
        assert!(config.tls_13_only);
        assert!(!config.strict_mode);
    }
    
    #[test]
    fn test_strict_config() {
        let config = TlsPinningConfig::strict("test_hash".to_string());
        assert_eq!(config.spki_hash.unwrap(), "test_hash");
        assert!(config.strict_mode);
    }
}
