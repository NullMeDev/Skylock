//! Automated Key Rotation Module
//!
//! Provides automated encryption key rotation with:
//! - Configurable rotation intervals (time-based)
//! - Usage-based rotation (max encryptions per key)
//! - Grace periods for re-encryption of existing data
//! - Key versioning and tracking
//! - Backward compatibility with old keys
//!
//! Security benefits:
//! - Limits exposure window if a key is compromised
//! - Reduces amount of data encrypted under any single key
//! - Enables cryptographic agility for future algorithm updates

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc, Duration as ChronoDuration};
use sha2::{Sha256, Digest};
use zeroize::{Zeroize, ZeroizeOnDrop};
use std::sync::Arc;
use parking_lot::RwLock;

use crate::error::{Result, SkylockError};

/// Key rotation policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRotationPolicy {
    /// Maximum age of a key before rotation (in days)
    pub max_key_age_days: u32,
    /// Maximum number of encryptions before rotation
    pub max_encryptions_per_key: u64,
    /// Grace period for re-encryption after rotation (in days)
    pub grace_period_days: u32,
    /// Whether to automatically re-encrypt data during grace period
    pub auto_reencrypt: bool,
    /// Minimum time between rotations (prevents rapid rotation)
    pub min_rotation_interval_hours: u32,
    /// Whether rotation is enabled
    pub enabled: bool,
}

impl Default for KeyRotationPolicy {
    fn default() -> Self {
        Self {
            max_key_age_days: 90,           // Rotate every 90 days
            max_encryptions_per_key: 1_000_000_000, // 1 billion encryptions
            grace_period_days: 30,           // 30 days to re-encrypt old data
            auto_reencrypt: false,           // Manual re-encryption by default
            min_rotation_interval_hours: 24, // At least 24 hours between rotations
            enabled: true,
        }
    }
}

impl KeyRotationPolicy {
    /// Create a conservative policy (more frequent rotation)
    pub fn conservative() -> Self {
        Self {
            max_key_age_days: 30,
            max_encryptions_per_key: 100_000_000,
            grace_period_days: 14,
            auto_reencrypt: true,
            min_rotation_interval_hours: 1,
            enabled: true,
        }
    }
    
    /// Create an aggressive policy (very frequent rotation)
    pub fn aggressive() -> Self {
        Self {
            max_key_age_days: 7,
            max_encryptions_per_key: 10_000_000,
            grace_period_days: 7,
            auto_reencrypt: true,
            min_rotation_interval_hours: 1,
            enabled: true,
        }
    }
    
    /// Create a relaxed policy (less frequent rotation)
    pub fn relaxed() -> Self {
        Self {
            max_key_age_days: 365,
            max_encryptions_per_key: 10_000_000_000,
            grace_period_days: 90,
            auto_reencrypt: false,
            min_rotation_interval_hours: 168, // 1 week
            enabled: true,
        }
    }
    
    /// Disable key rotation
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }
}

/// Key version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyVersion {
    /// Unique version ID (monotonically increasing)
    pub version: u64,
    /// Key fingerprint (SHA-256 hash of derived key, truncated)
    pub fingerprint: String,
    /// When this key version was created
    pub created_at: DateTime<Utc>,
    /// When this key version expires (based on policy)
    pub expires_at: DateTime<Utc>,
    /// When grace period ends (can still decrypt, but shouldn't encrypt)
    pub grace_ends_at: DateTime<Utc>,
    /// Number of encryptions performed with this key
    pub encryption_count: u64,
    /// Whether this key is currently active for encryption
    pub is_active: bool,
    /// Whether this key can still be used for decryption
    pub can_decrypt: bool,
    /// Salt used for key derivation
    pub salt: String,
    /// Algorithm used
    pub algorithm: String,
}

impl KeyVersion {
    /// Check if the key needs rotation based on policy
    pub fn needs_rotation(&self, policy: &KeyRotationPolicy) -> bool {
        if !policy.enabled {
            return false;
        }
        
        let now = Utc::now();
        
        // Check expiration
        if now >= self.expires_at {
            return true;
        }
        
        // Check encryption count
        if self.encryption_count >= policy.max_encryptions_per_key {
            return true;
        }
        
        false
    }
    
    /// Check if the key is in grace period
    pub fn in_grace_period(&self) -> bool {
        let now = Utc::now();
        now >= self.expires_at && now < self.grace_ends_at
    }
    
    /// Check if the key is still valid for decryption
    pub fn is_valid_for_decryption(&self) -> bool {
        self.can_decrypt
    }
    
    /// Check if the key is valid for encryption
    pub fn is_valid_for_encryption(&self) -> bool {
        self.is_active && Utc::now() < self.expires_at
    }
}

/// Key chain managing multiple key versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyChain {
    /// All key versions (newest first)
    pub versions: Vec<KeyVersion>,
    /// Current active version for encryption
    pub active_version: u64,
    /// When the chain was created
    pub created_at: DateTime<Utc>,
    /// Last rotation time
    pub last_rotation: Option<DateTime<Utc>>,
    /// Rotation policy
    pub policy: KeyRotationPolicy,
}

impl KeyChain {
    /// Create a new key chain with initial key
    pub fn new(initial_fingerprint: &str, salt: &str, policy: KeyRotationPolicy) -> Self {
        let now = Utc::now();
        let expires_at = now + ChronoDuration::days(policy.max_key_age_days as i64);
        let grace_ends_at = expires_at + ChronoDuration::days(policy.grace_period_days as i64);
        
        let initial_version = KeyVersion {
            version: 1,
            fingerprint: initial_fingerprint.to_string(),
            created_at: now,
            expires_at,
            grace_ends_at,
            encryption_count: 0,
            is_active: true,
            can_decrypt: true,
            salt: salt.to_string(),
            algorithm: "AES-256-GCM".to_string(),
        };
        
        Self {
            versions: vec![initial_version],
            active_version: 1,
            created_at: now,
            last_rotation: None,
            policy,
        }
    }
    
    /// Get the active key version
    pub fn active(&self) -> Option<&KeyVersion> {
        self.versions.iter().find(|v| v.version == self.active_version)
    }
    
    /// Get a specific key version
    pub fn get_version(&self, version: u64) -> Option<&KeyVersion> {
        self.versions.iter().find(|v| v.version == version)
    }
    
    /// Get all versions valid for decryption
    pub fn decryption_versions(&self) -> Vec<&KeyVersion> {
        self.versions.iter().filter(|v| v.can_decrypt).collect()
    }
    
    /// Check if rotation is needed
    pub fn needs_rotation(&self) -> bool {
        if !self.policy.enabled {
            return false;
        }
        
        if let Some(active) = self.active() {
            return active.needs_rotation(&self.policy);
        }
        
        true // No active key, need rotation
    }
    
    /// Check if enough time has passed since last rotation
    pub fn can_rotate(&self) -> bool {
        if let Some(last) = self.last_rotation {
            let min_interval = ChronoDuration::hours(self.policy.min_rotation_interval_hours as i64);
            return Utc::now() >= last + min_interval;
        }
        true
    }
    
    /// Rotate to a new key version
    pub fn rotate(&mut self, new_fingerprint: &str, new_salt: &str) -> Result<u64> {
        if !self.can_rotate() {
            return Err(SkylockError::Encryption(
                "Minimum rotation interval not met".to_string()
            ));
        }
        
        let now = Utc::now();
        let new_version = self.versions.iter().map(|v| v.version).max().unwrap_or(0) + 1;
        
        let expires_at = now + ChronoDuration::days(self.policy.max_key_age_days as i64);
        let grace_ends_at = expires_at + ChronoDuration::days(self.policy.grace_period_days as i64);
        
        // Deactivate current active key
        if let Some(active) = self.versions.iter_mut().find(|v| v.version == self.active_version) {
            active.is_active = false;
            // Keep can_decrypt true during grace period
        }
        
        // Add new version
        let new_key_version = KeyVersion {
            version: new_version,
            fingerprint: new_fingerprint.to_string(),
            created_at: now,
            expires_at,
            grace_ends_at,
            encryption_count: 0,
            is_active: true,
            can_decrypt: true,
            salt: new_salt.to_string(),
            algorithm: "AES-256-GCM".to_string(),
        };
        
        self.versions.insert(0, new_key_version);
        self.active_version = new_version;
        self.last_rotation = Some(now);
        
        // Clean up very old versions (past grace period and not active)
        self.cleanup_expired_versions();
        
        Ok(new_version)
    }
    
    /// Remove versions that are past their grace period
    fn cleanup_expired_versions(&mut self) {
        let now = Utc::now();
        self.versions.retain(|v| {
            // Keep active version
            if v.version == self.active_version {
                return true;
            }
            // Keep versions still in grace period
            if now < v.grace_ends_at {
                return true;
            }
            // Remove expired versions
            false
        });
        
        // Disable decryption for versions past grace
        for version in &mut self.versions {
            if now >= version.grace_ends_at && version.version != self.active_version {
                version.can_decrypt = false;
            }
        }
    }
    
    /// Record an encryption with the active key
    pub fn record_encryption(&mut self) -> Result<()> {
        if let Some(active) = self.versions.iter_mut()
            .find(|v| v.version == self.active_version)
        {
            active.encryption_count += 1;
            Ok(())
        } else {
            Err(SkylockError::Encryption("No active key version".to_string()))
        }
    }
}

/// Key rotation manager
pub struct KeyRotationManager {
    /// Key chain state
    key_chain: Arc<RwLock<KeyChain>>,
    /// Path to persist key chain state
    state_path: PathBuf,
    /// Derived key cache (version -> key bytes)
    key_cache: RwLock<HashMap<u64, [u8; 32]>>,
}

impl KeyRotationManager {
    /// Create a new key rotation manager
    pub fn new(
        state_path: PathBuf,
        initial_key: &[u8; 32],
        policy: KeyRotationPolicy,
    ) -> Result<Self> {
        // Calculate fingerprint
        let fingerprint = Self::calculate_fingerprint(initial_key);
        
        // Generate salt
        let salt = Self::generate_salt();
        
        // Create key chain
        let key_chain = KeyChain::new(&fingerprint, &salt, policy);
        
        let mut key_cache = HashMap::new();
        key_cache.insert(1, *initial_key);
        
        Ok(Self {
            key_chain: Arc::new(RwLock::new(key_chain)),
            state_path,
            key_cache: RwLock::new(key_cache),
        })
    }
    
    /// Load existing key chain from disk
    pub fn load(state_path: PathBuf) -> Result<Self> {
        let data = std::fs::read_to_string(&state_path)
            .map_err(|e| SkylockError::Backup(format!("Failed to load key chain: {}", e)))?;
        
        let key_chain: KeyChain = serde_json::from_str(&data)
            .map_err(|e| SkylockError::Backup(format!("Failed to parse key chain: {}", e)))?;
        
        Ok(Self {
            key_chain: Arc::new(RwLock::new(key_chain)),
            state_path,
            key_cache: RwLock::new(HashMap::new()),
        })
    }
    
    /// Save key chain to disk
    pub fn save(&self) -> Result<()> {
        let chain = self.key_chain.read();
        let data = serde_json::to_string_pretty(&*chain)
            .map_err(|e| SkylockError::Backup(format!("Failed to serialize key chain: {}", e)))?;
        
        std::fs::write(&self.state_path, data)
            .map_err(|e| SkylockError::Backup(format!("Failed to save key chain: {}", e)))?;
        
        Ok(())
    }
    
    /// Calculate key fingerprint (first 16 chars of SHA-256)
    fn calculate_fingerprint(key: &[u8; 32]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key);
        let hash = hasher.finalize();
        hex::encode(&hash[..8]) // First 8 bytes = 16 hex chars
    }
    
    /// Generate a random salt
    fn generate_salt() -> String {
        use rand::RngCore;
        let mut salt = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &salt)
    }
    
    /// Check if rotation is needed
    pub fn needs_rotation(&self) -> bool {
        self.key_chain.read().needs_rotation()
    }
    
    /// Perform key rotation
    pub fn rotate(&self, new_key: &[u8; 32]) -> Result<u64> {
        let fingerprint = Self::calculate_fingerprint(new_key);
        let salt = Self::generate_salt();
        
        let new_version = self.key_chain.write().rotate(&fingerprint, &salt)?;
        
        // Cache the new key
        self.key_cache.write().insert(new_version, *new_key);
        
        // Save state
        self.save()?;
        
        Ok(new_version)
    }
    
    /// Get the active key version number
    pub fn active_version(&self) -> u64 {
        self.key_chain.read().active_version
    }
    
    /// Get key for a specific version (from cache or derive)
    pub fn get_key(&self, version: u64) -> Result<[u8; 32]> {
        // Check cache first
        if let Some(key) = self.key_cache.read().get(&version) {
            return Ok(*key);
        }
        
        // Key not in cache - this is an error in normal operation
        // because we should have cached all keys we've used
        Err(SkylockError::Encryption(
            format!("Key version {} not found in cache", version)
        ))
    }
    
    /// Cache a key for a specific version (used during decryption setup)
    pub fn cache_key(&self, version: u64, key: [u8; 32]) {
        self.key_cache.write().insert(version, key);
    }
    
    /// Record an encryption
    pub fn record_encryption(&self) -> Result<()> {
        self.key_chain.write().record_encryption()
    }
    
    /// Get key chain info
    pub fn info(&self) -> KeyChainInfo {
        let chain = self.key_chain.read();
        KeyChainInfo {
            active_version: chain.active_version,
            total_versions: chain.versions.len(),
            needs_rotation: chain.needs_rotation(),
            policy: chain.policy.clone(),
            last_rotation: chain.last_rotation,
        }
    }
    
    /// Get all key versions
    pub fn versions(&self) -> Vec<KeyVersion> {
        self.key_chain.read().versions.clone()
    }
}

/// Key chain information summary
#[derive(Debug, Clone, Serialize)]
pub struct KeyChainInfo {
    pub active_version: u64,
    pub total_versions: usize,
    pub needs_rotation: bool,
    pub policy: KeyRotationPolicy,
    pub last_rotation: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_policy_defaults() {
        let policy = KeyRotationPolicy::default();
        assert!(policy.enabled);
        assert_eq!(policy.max_key_age_days, 90);
        assert_eq!(policy.grace_period_days, 30);
    }
    
    #[test]
    fn test_policy_variants() {
        let conservative = KeyRotationPolicy::conservative();
        assert_eq!(conservative.max_key_age_days, 30);
        
        let aggressive = KeyRotationPolicy::aggressive();
        assert_eq!(aggressive.max_key_age_days, 7);
        
        let relaxed = KeyRotationPolicy::relaxed();
        assert_eq!(relaxed.max_key_age_days, 365);
        
        let disabled = KeyRotationPolicy::disabled();
        assert!(!disabled.enabled);
    }
    
    #[test]
    fn test_key_chain_creation() {
        let policy = KeyRotationPolicy::default();
        let chain = KeyChain::new("fingerprint123", "salt456", policy);
        
        assert_eq!(chain.active_version, 1);
        assert_eq!(chain.versions.len(), 1);
        
        let active = chain.active().unwrap();
        assert_eq!(active.version, 1);
        assert!(active.is_active);
        assert!(active.can_decrypt);
    }
    
    #[test]
    fn test_key_rotation() {
        let mut policy = KeyRotationPolicy::default();
        policy.min_rotation_interval_hours = 0; // Allow immediate rotation for test
        
        let mut chain = KeyChain::new("fp1", "salt1", policy);
        
        // Initial state
        assert_eq!(chain.active_version, 1);
        
        // Rotate
        let new_version = chain.rotate("fp2", "salt2").unwrap();
        assert_eq!(new_version, 2);
        assert_eq!(chain.active_version, 2);
        assert_eq!(chain.versions.len(), 2);
        
        // Old version should still be decryptable
        let v1 = chain.get_version(1).unwrap();
        assert!(!v1.is_active);
        assert!(v1.can_decrypt);
        
        // New version should be active
        let v2 = chain.get_version(2).unwrap();
        assert!(v2.is_active);
        assert!(v2.can_decrypt);
    }
    
    #[test]
    fn test_key_rotation_manager() {
        let dir = tempdir().unwrap();
        let state_path = dir.path().join("keychain.json");
        
        let initial_key = [0x42u8; 32];
        let policy = KeyRotationPolicy::default();
        
        let manager = KeyRotationManager::new(state_path.clone(), &initial_key, policy).unwrap();
        
        assert_eq!(manager.active_version(), 1);
        
        // Key should be in cache
        let key = manager.get_key(1).unwrap();
        assert_eq!(key, initial_key);
        
        // Save and verify file exists
        manager.save().unwrap();
        assert!(state_path.exists());
    }
    
    #[test]
    fn test_encryption_counting() {
        let mut policy = KeyRotationPolicy::default();
        policy.max_encryptions_per_key = 10;
        
        let mut chain = KeyChain::new("fp", "salt", policy);
        
        // Record encryptions
        for _ in 0..5 {
            chain.record_encryption().unwrap();
        }
        
        let active = chain.active().unwrap();
        assert_eq!(active.encryption_count, 5);
        assert!(!chain.needs_rotation());
        
        // Exceed threshold
        for _ in 0..6 {
            chain.record_encryption().unwrap();
        }
        
        assert!(chain.needs_rotation());
    }
    
    #[test]
    fn test_key_version_validity() {
        let policy = KeyRotationPolicy::default();
        let chain = KeyChain::new("fp", "salt", policy);
        
        let active = chain.active().unwrap();
        
        assert!(active.is_valid_for_encryption());
        assert!(active.is_valid_for_decryption());
        assert!(!active.in_grace_period());
    }
    
    #[test]
    fn test_fingerprint_calculation() {
        let key1 = [0x42u8; 32];
        let key2 = [0x43u8; 32];
        
        let fp1 = KeyRotationManager::calculate_fingerprint(&key1);
        let fp2 = KeyRotationManager::calculate_fingerprint(&key2);
        
        // Fingerprints should be different
        assert_ne!(fp1, fp2);
        
        // Fingerprint should be 16 hex chars
        assert_eq!(fp1.len(), 16);
        
        // Same key should give same fingerprint
        let fp1_again = KeyRotationManager::calculate_fingerprint(&key1);
        assert_eq!(fp1, fp1_again);
    }
}
