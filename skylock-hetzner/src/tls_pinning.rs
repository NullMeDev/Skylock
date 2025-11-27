//! TLS Certificate Pinning for WebDAV
//! 
//! Provides optional SPKI (Subject Public Key Info) pinning to prevent MITM attacks
//! even if an attacker compromises a Certificate Authority.
//!
//! ## Security Features
//! - SPKI pinning with SHA-256 hashes
//! - Backup pin support for key rotation
//! - Pin expiration and reporting
//! - TLS 1.3 enforcement option
//! - Constant-time hash comparison

use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use anyhow::{Result, anyhow};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{warn, info, error};

/// Pin validation result
#[derive(Debug, Clone, PartialEq)]
pub enum PinValidationResult {
    /// Pin matched successfully
    Valid,
    /// Pin matched a backup pin (primary may need rotation)
    ValidBackup { matched_pin: String },
    /// No pin configured, validation skipped
    NotConfigured,
    /// Pin mismatch - potential MITM attack
    Invalid { expected: Vec<String>, actual: String },
    /// Pin expired
    Expired { pin: String, expired_at: chrono::DateTime<chrono::Utc> },
}

/// Certificate pin with metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CertificatePin {
    /// Base64-encoded SHA-256 hash of SPKI
    pub hash: String,
    /// Human-readable label (e.g., "Hetzner Primary 2025")
    pub label: Option<String>,
    /// When this pin expires (None = never)
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Whether this is a backup pin
    pub is_backup: bool,
}

impl CertificatePin {
    /// Create a new primary pin
    pub fn primary(hash: String) -> Self {
        Self {
            hash,
            label: None,
            expires_at: None,
            is_backup: false,
        }
    }
    
    /// Create a new backup pin
    pub fn backup(hash: String) -> Self {
        Self {
            hash,
            label: None,
            expires_at: None,
            is_backup: true,
        }
    }
    
    /// Set a label for this pin
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
    
    /// Set expiration for this pin
    pub fn with_expiration(mut self, expires_at: chrono::DateTime<chrono::Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }
    
    /// Check if this pin is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| chrono::Utc::now() > exp).unwrap_or(false)
    }
}

/// TLS Pinning configuration with enhanced security features
#[derive(Debug, Clone)]
pub struct TlsPinningConfig {
    /// List of acceptable certificate pins (primary + backups)
    pub pins: Vec<CertificatePin>,
    /// Whether to enforce TLS 1.3 only
    pub tls_13_only: bool,
    /// Whether pinning is strict (fail if no match) or advisory (warn only)
    pub strict_mode: bool,
    /// Report URI for pin validation failures (for monitoring)
    pub report_uri: Option<String>,
    /// Maximum age for cached validation results
    pub cache_duration: Duration,
}

impl Default for TlsPinningConfig {
    fn default() -> Self {
        Self {
            pins: Vec::new(),
            tls_13_only: true,  // Default to TLS 1.3 for security
            strict_mode: false,  // Default to advisory mode to avoid breaking existing setups
            report_uri: None,
            cache_duration: Duration::from_secs(3600), // 1 hour cache
        }
    }
}

impl TlsPinningConfig {
    /// Create a new pinning config with strict mode enabled
    pub fn strict(spki_hash: String) -> Self {
        Self {
            pins: vec![CertificatePin::primary(spki_hash)],
            tls_13_only: true,
            strict_mode: true,
            report_uri: None,
            cache_duration: Duration::from_secs(3600),
        }
    }
    
    /// Create a new pinning config in advisory mode (warns but doesn't fail)
    pub fn advisory(spki_hash: String) -> Self {
        Self {
            pins: vec![CertificatePin::primary(spki_hash)],
            tls_13_only: true,
            strict_mode: false,
            report_uri: None,
            cache_duration: Duration::from_secs(3600),
        }
    }
    
    /// Add a backup pin for key rotation
    pub fn with_backup_pin(mut self, hash: String) -> Self {
        self.pins.push(CertificatePin::backup(hash));
        self
    }
    
    /// Add a report URI for pin failures
    pub fn with_report_uri(mut self, uri: String) -> Self {
        self.report_uri = Some(uri);
        self
    }
    
    /// Get the primary SPKI hash (for backward compatibility)
    pub fn spki_hash(&self) -> Option<&str> {
        self.pins.first().map(|p| p.hash.as_str())
    }
    
    /// Legacy compatibility getter
    #[deprecated(note = "Use pins field directly")]
    pub fn get_spki_hash(&self) -> Option<String> {
        self.pins.first().map(|p| p.hash.clone())
    }
}

/// Certificate pinning validator with caching
pub struct CertificatePinner {
    config: TlsPinningConfig,
    /// Cache of validated hashes with timestamps
    validation_cache: Arc<RwLock<std::collections::HashMap<String, (PinValidationResult, Instant)>>>,
    /// Set of reported failures (to avoid duplicate reports)
    reported_failures: Arc<RwLock<HashSet<String>>>,
}

impl CertificatePinner {
    /// Create a new certificate pinner with the given configuration
    pub fn new(config: TlsPinningConfig) -> Self {
        Self {
            config,
            validation_cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
            reported_failures: Arc::new(RwLock::new(HashSet::new())),
        }
    }
    
    /// Validate a certificate's SPKI against pinned values
    pub async fn validate(&self, spki_der: &[u8]) -> Result<PinValidationResult> {
        // If no pins configured, skip validation
        if self.config.pins.is_empty() {
            return Ok(PinValidationResult::NotConfigured);
        }
        
        let actual_hash = compute_spki_hash(spki_der);
        
        // Check cache first
        {
            let cache = self.validation_cache.read().await;
            if let Some((result, timestamp)) = cache.get(&actual_hash) {
                if timestamp.elapsed() < self.config.cache_duration {
                    return Ok(result.clone());
                }
            }
        }
        
        // Perform validation
        let result = self.validate_hash(&actual_hash);
        
        // Cache the result
        {
            let mut cache = self.validation_cache.write().await;
            cache.insert(actual_hash.clone(), (result.clone(), Instant::now()));
        }
        
        // Handle validation result
        match &result {
            PinValidationResult::Valid => {
                info!("Certificate pin validation successful");
            }
            PinValidationResult::ValidBackup { matched_pin } => {
                warn!("Certificate matched backup pin '{}' - consider rotating primary pin", matched_pin);
            }
            PinValidationResult::Invalid { expected, actual } => {
                error!("Certificate pin mismatch! Expected one of {:?}, got {}", expected, actual);
                self.report_failure(&actual_hash, "pin_mismatch").await;
                
                if self.config.strict_mode {
                    return Err(anyhow!("Certificate pinning validation failed: MITM attack possible"));
                }
            }
            PinValidationResult::Expired { pin, expired_at } => {
                error!("Certificate pin '{}' expired at {}", pin, expired_at);
                self.report_failure(&actual_hash, "pin_expired").await;
                
                if self.config.strict_mode {
                    return Err(anyhow!("Certificate pin expired"));
                }
            }
            PinValidationResult::NotConfigured => {}
        }
        
        Ok(result)
    }
    
    /// Validate a hash against configured pins
    fn validate_hash(&self, actual_hash: &str) -> PinValidationResult {
        for pin in &self.config.pins {
            // Use constant-time comparison to prevent timing attacks
            if constant_time_compare(&pin.hash, actual_hash) {
                // Check if pin is expired
                if pin.is_expired() {
                    return PinValidationResult::Expired {
                        pin: pin.label.clone().unwrap_or_else(|| pin.hash.clone()),
                        expired_at: pin.expires_at.unwrap(),
                    };
                }
                
                // Check if this is a backup pin
                if pin.is_backup {
                    return PinValidationResult::ValidBackup {
                        matched_pin: pin.label.clone().unwrap_or_else(|| pin.hash.clone()),
                    };
                }
                
                return PinValidationResult::Valid;
            }
        }
        
        PinValidationResult::Invalid {
            expected: self.config.pins.iter().map(|p| p.hash.clone()).collect(),
            actual: actual_hash.to_string(),
        }
    }
    
    /// Report a pin validation failure
    async fn report_failure(&self, hash: &str, reason: &str) {
        // Check if already reported
        {
            let reported = self.reported_failures.read().await;
            if reported.contains(hash) {
                return;
            }
        }
        
        // Mark as reported
        {
            let mut reported = self.reported_failures.write().await;
            reported.insert(hash.to_string());
        }
        
        // Report to URI if configured
        if let Some(ref uri) = self.config.report_uri {
            let report = serde_json::json!({
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "reason": reason,
                "certificate_hash": hash,
                "expected_pins": self.config.pins.iter().map(|p| &p.hash).collect::<Vec<_>>(),
            });
            
            // Fire-and-forget report (don't block on network)
            let uri = uri.clone();
            let report_str = report.to_string();
            tokio::spawn(async move {
                if let Err(e) = send_pin_report(&uri, &report_str).await {
                    warn!("Failed to send pin validation report: {}", e);
                }
            });
        }
    }
    
    /// Check if pinning is enabled
    pub fn is_enabled(&self) -> bool {
        !self.config.pins.is_empty()
    }
    
    /// Check if strict mode is enabled
    pub fn is_strict(&self) -> bool {
        self.config.strict_mode
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
    Ok(constant_time_compare(&computed, expected))
}

/// Constant-time string comparison to prevent timing attacks
fn constant_time_compare(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    
    let mut result = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        result |= x ^ y;
    }
    result == 0
}

/// Send a pin validation report (async helper)
async fn send_pin_report(uri: &str, report: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    
    client.post(uri)
        .header("Content-Type", "application/json")
        .body(report.to_string())
        .send()
        .await?;
    
    Ok(())
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
        assert!(config.spki_hash().is_none());
        assert!(config.tls_13_only);
        assert!(!config.strict_mode);
    }
    
    #[test]
    fn test_strict_config() {
        let config = TlsPinningConfig::strict("test_hash".to_string());
        assert_eq!(config.spki_hash().unwrap(), "test_hash");
        assert!(config.strict_mode);
    }
    
    #[test]
    fn test_constant_time_compare() {
        assert!(constant_time_compare("hello", "hello"));
        assert!(!constant_time_compare("hello", "world"));
        assert!(!constant_time_compare("hello", "hell"));
        assert!(!constant_time_compare("", "a"));
        assert!(constant_time_compare("", ""));
    }
    
    #[test]
    fn test_certificate_pin_creation() {
        let pin = CertificatePin::primary("abc123".to_string())
            .with_label("Test Pin");
        
        assert_eq!(pin.hash, "abc123");
        assert_eq!(pin.label.as_ref().unwrap(), "Test Pin");
        assert!(!pin.is_backup);
        assert!(!pin.is_expired());
    }
    
    #[test]
    fn test_backup_pin() {
        let config = TlsPinningConfig::strict("primary".to_string())
            .with_backup_pin("backup".to_string());
        
        assert_eq!(config.pins.len(), 2);
        assert!(!config.pins[0].is_backup);
        assert!(config.pins[1].is_backup);
    }
    
    #[tokio::test]
    async fn test_pinner_validation() {
        let test_spki = b"test_subject_public_key_info";
        let hash = compute_spki_hash(test_spki);
        
        let config = TlsPinningConfig::strict(hash.clone());
        let pinner = CertificatePinner::new(config);
        
        // Valid pin should pass
        let result = pinner.validate(test_spki).await.unwrap();
        assert_eq!(result, PinValidationResult::Valid);
        
        // Invalid pin should fail
        let invalid_spki = b"wrong_spki";
        let result = pinner.validate(invalid_spki).await;
        assert!(result.is_err()); // Strict mode returns error
    }
    
    #[tokio::test]
    async fn test_pinner_not_configured() {
        let config = TlsPinningConfig::default();
        let pinner = CertificatePinner::new(config);
        
        let test_spki = b"any_spki";
        let result = pinner.validate(test_spki).await.unwrap();
        assert_eq!(result, PinValidationResult::NotConfigured);
    }
    
    #[tokio::test]
    async fn test_backup_pin_validation() {
        let primary_spki = b"primary_spki";
        let backup_spki = b"backup_spki";
        let primary_hash = compute_spki_hash(primary_spki);
        let backup_hash = compute_spki_hash(backup_spki);
        
        let config = TlsPinningConfig::advisory(primary_hash)
            .with_backup_pin(backup_hash);
        let pinner = CertificatePinner::new(config);
        
        // Backup pin should return ValidBackup
        let result = pinner.validate(backup_spki).await.unwrap();
        match result {
            PinValidationResult::ValidBackup { .. } => {},
            _ => panic!("Expected ValidBackup result"),
        }
    }
}
