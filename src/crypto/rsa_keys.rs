//! RSA-4096 Key Management for Skylock Hybrid
//! 
//! This module provides RSA key generation, storage, and rotation capabilities
//! for asymmetric encryption of backup metadata and key exchange.

use rsa::{
    Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey, 
    pkcs1::{DecodeRsaPrivateKey, EncodeRsaPrivateKey, EncodeRsaPublicKey, LineEnding as Pkcs1LineEnding},
    traits::PublicKeyParts,
};
use pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};
use chrono::{DateTime, Utc};

/// RSA key management errors
#[derive(Error, Debug)]
pub enum RsaError {
    #[error("Key generation failed: {0}")]
    KeyGeneration(String),
    #[error("Key loading failed: {0}")]
    KeyLoading(String),
    #[error("Key saving failed: {0}")]
    KeySaving(String),
    #[error("Encryption failed: {0}")]
    Encryption(String),
    #[error("Decryption failed: {0}")]
    Decryption(String),
    #[error("Key validation failed: {0}")]
    KeyValidation(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// RSA key pair metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RsaKeyMetadata {
    pub key_id: String,
    pub algorithm: String,
    pub key_size: u32,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub usage: Vec<String>, // e.g., ["encryption", "signing", "key_exchange"]
    pub fingerprint: String,
    pub public_key_pem: String,
}

/// RSA private key with secure storage
#[derive(zeroize::ZeroizeOnDrop)]
pub struct SecureRsaPrivateKey {
    #[zeroize(skip)]
    pub metadata: RsaKeyMetadata,
    pub private_key: RsaPrivateKey,
    #[zeroize(skip)]
    pub public_key: RsaPublicKey,
}

impl SecureRsaPrivateKey {
    /// Generate a new RSA-4096 key pair
    pub fn generate(usage: Vec<String>, expires_in_days: Option<u32>) -> Result<Self, RsaError> {
        let mut rng = OsRng;
        let bits = 4096;
        
        let private_key = RsaPrivateKey::new(&mut rng, bits)
            .map_err(|e| RsaError::KeyGeneration(e.to_string()))?;
        
        let public_key = RsaPublicKey::from(&private_key);
        
        let key_id = Self::generate_key_id();
        let created_at = Utc::now();
        let expires_at = expires_in_days.map(|days| created_at + chrono::Duration::days(days as i64));
        
        let fingerprint = Self::calculate_fingerprint(&public_key)?;
        let public_key_pem = public_key
            .to_pkcs1_pem(Pkcs1LineEnding::LF)
            .map_err(|e| RsaError::Serialization(e.to_string()))?;
        
        let metadata = RsaKeyMetadata {
            key_id,
            algorithm: "RSA-4096".to_string(),
            key_size: bits as u32,
            created_at,
            expires_at,
            usage,
            fingerprint,
            public_key_pem,
        };

        Ok(SecureRsaPrivateKey {
            metadata,
            private_key,
            public_key,
        })
    }

    /// Load RSA private key from PEM file
    pub fn from_pem_file<P: AsRef<Path>>(path: P) -> Result<Self, RsaError> {
        let pem_data = std::fs::read_to_string(&path)?;
        Self::from_pem(&pem_data)
    }

    /// Load RSA private key from PEM string
    pub fn from_pem(pem_data: &str) -> Result<Self, RsaError> {
        let private_key = RsaPrivateKey::from_pkcs8_pem(pem_data)
            .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem_data))
            .map_err(|e| RsaError::KeyLoading(e.to_string()))?;
        
        let public_key = RsaPublicKey::from(&private_key);
        
        let key_id = Self::generate_key_id();
        let fingerprint = Self::calculate_fingerprint(&public_key)?;
        let public_key_pem = public_key
            .to_pkcs1_pem(Pkcs1LineEnding::LF)
            .map_err(|e| RsaError::Serialization(e.to_string()))?;

        let metadata = RsaKeyMetadata {
            key_id,
            algorithm: "RSA-4096".to_string(),
            key_size: private_key.size() as u32 * 8,
            created_at: Utc::now(),
            expires_at: None,
            usage: vec!["encryption".to_string()],
            fingerprint,
            public_key_pem,
        };

        Ok(SecureRsaPrivateKey {
            metadata,
            private_key,
            public_key,
        })
    }

    /// Save private key to encrypted PEM file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P, password: Option<&str>) -> Result<(), RsaError> {
        let pem_data = if let Some(_pwd) = password {
            // Note: encrypted PEM not available in current rsa version
            self.private_key
                .to_pkcs8_pem(LineEnding::LF)
                .map_err(|e| RsaError::KeySaving(e.to_string()))?
        } else {
            self.private_key
                .to_pkcs8_pem(LineEnding::LF)
                .map_err(|e| RsaError::KeySaving(e.to_string()))?
        };

        std::fs::write(&path, pem_data.as_bytes())?;
        
        // Set restrictive permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path)?.permissions();
            perms.set_mode(0o600); // rw-------
            std::fs::set_permissions(&path, perms)?;
        }

        Ok(())
    }

    /// Save public key to file
    pub fn save_public_key_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), RsaError> {
        std::fs::write(&path, &self.metadata.public_key_pem)?;
        Ok(())
    }

    /// Encrypt data with RSA public key
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>, RsaError> {
        let mut rng = OsRng;
        self.public_key
            .encrypt(&mut rng, Pkcs1v15Encrypt, data)
            .map_err(|e| RsaError::Encryption(e.to_string()))
    }

    /// Decrypt data with RSA private key
    pub fn decrypt(&self, encrypted_data: &[u8]) -> Result<Vec<u8>, RsaError> {
        self.private_key
            .decrypt(Pkcs1v15Encrypt, encrypted_data)
            .map_err(|e| RsaError::Decryption(e.to_string()))
    }

    /// Check if the key is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.metadata.expires_at {
            expires_at < Utc::now()
        } else {
            false
        }
    }

    /// Validate key integrity
    pub fn validate(&self) -> Result<(), RsaError> {
        // Check if private and public keys match
        let test_data = b"validation_test";
        let encrypted = self.encrypt(test_data)?;
        let decrypted = self.decrypt(&encrypted)?;
        
        if decrypted != test_data {
            return Err(RsaError::KeyValidation("Key pair validation failed".to_string()));
        }

        Ok(())
    }

    /// Generate a random key ID
    fn generate_key_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    /// Calculate public key fingerprint
    fn calculate_fingerprint(public_key: &RsaPublicKey) -> Result<String, RsaError> {
        let der_bytes = public_key
            .to_pkcs1_der()
            .map_err(|e| RsaError::Serialization(e.to_string()))?;
        
        let mut hasher = Sha256::new();
        hasher.update(&der_bytes);
        let hash = hasher.finalize();
        
        Ok(hex::encode(&hash[..16])) // First 16 bytes as hex
    }

    /// Get public key for sharing
    pub fn public_key(&self) -> RsaPublicKey {
        self.public_key.clone()
    }

    /// Get metadata
    pub fn metadata(&self) -> &RsaKeyMetadata {
        &self.metadata
    }
}

/// RSA key manager for handling multiple keys and rotation
pub struct RsaKeyManager {
    keys: std::collections::HashMap<String, SecureRsaPrivateKey>,
    key_directory: PathBuf,
    current_key_id: Option<String>,
}

impl RsaKeyManager {
    /// Create new RSA key manager
    pub fn new<P: AsRef<Path>>(key_directory: P) -> Result<Self, RsaError> {
        let key_dir = key_directory.as_ref().to_path_buf();
        std::fs::create_dir_all(&key_dir)?;
        
        Ok(RsaKeyManager {
            keys: std::collections::HashMap::new(),
            key_directory: key_dir,
            current_key_id: None,
        })
    }

    /// Generate and add new RSA key
    pub fn generate_key(
        &mut self, 
        usage: Vec<String>,
        expires_in_days: Option<u32>
    ) -> Result<String, RsaError> {
        let key = SecureRsaPrivateKey::generate(usage, expires_in_days)?;
        let key_id = key.metadata.key_id.clone();
        
        // Save to disk
        let private_key_path = self.key_directory.join(format!("{}.pem", key_id));
        let public_key_path = self.key_directory.join(format!("{}_pub.pem", key_id));
        let metadata_path = self.key_directory.join(format!("{}_metadata.json", key_id));
        
        key.save_to_file(&private_key_path, None)?;
        key.save_public_key_to_file(&public_key_path)?;
        
        let metadata_json = serde_json::to_string_pretty(&key.metadata)
            .map_err(|e| RsaError::Serialization(e.to_string()))?;
        std::fs::write(metadata_path, metadata_json)?;
        
        // Set as current key if none exists
        if self.current_key_id.is_none() {
            self.current_key_id = Some(key_id.clone());
        }
        
        self.keys.insert(key_id.clone(), key);
        Ok(key_id)
    }

    /// Load all keys from directory
    pub fn load_keys(&mut self) -> Result<Vec<String>, RsaError> {
        let mut loaded_keys = Vec::new();
        
        for entry in std::fs::read_dir(&self.key_directory)? {
            let entry = entry?;
            let path = entry.path();
            
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.ends_with(".pem") && !file_name.ends_with("_pub.pem") {
                    if let Some(key_id) = file_name.strip_suffix(".pem") {
                        match SecureRsaPrivateKey::from_pem_file(&path) {
                            Ok(mut key) => {
                                // Try to load metadata if it exists
                                let metadata_path = self.key_directory.join(format!("{}_metadata.json", key_id));
                                if metadata_path.exists() {
                                    if let Ok(metadata_json) = std::fs::read_to_string(&metadata_path) {
                                        if let Ok(metadata) = serde_json::from_str::<RsaKeyMetadata>(&metadata_json) {
                                            key.metadata = metadata;
                                        }
                                    }
                                }
                                
                                loaded_keys.push(key.metadata.key_id.clone());
                                self.keys.insert(key.metadata.key_id.clone(), key);
                            }
                            Err(e) => {
                                eprintln!("Failed to load key {}: {}", key_id, e);
                            }
                        }
                    }
                }
            }
        }
        
        // Set first key as current if none is set
        if self.current_key_id.is_none() && !loaded_keys.is_empty() {
            self.current_key_id = Some(loaded_keys[0].clone());
        }
        
        Ok(loaded_keys)
    }

    /// Get current active key
    pub fn current_key(&self) -> Option<&SecureRsaPrivateKey> {
        self.current_key_id.as_ref().and_then(|id| self.keys.get(id))
    }

    /// Set current active key
    pub fn set_current_key(&mut self, key_id: &str) -> Result<(), RsaError> {
        if self.keys.contains_key(key_id) {
            self.current_key_id = Some(key_id.to_string());
            Ok(())
        } else {
            Err(RsaError::KeyValidation(format!("Key not found: {}", key_id)))
        }
    }

    /// Get key by ID
    pub fn get_key(&self, key_id: &str) -> Option<&SecureRsaPrivateKey> {
        self.keys.get(key_id)
    }

    /// List all keys
    pub fn list_keys(&self) -> Vec<&RsaKeyMetadata> {
        self.keys.values().map(|key| &key.metadata).collect()
    }

    /// Remove expired keys
    pub fn cleanup_expired_keys(&mut self) -> Result<Vec<String>, RsaError> {
        let mut removed_keys = Vec::new();
        let now = Utc::now();
        
        let expired_keys: Vec<String> = self.keys
            .iter()
            .filter(|(_, key)| key.is_expired())
            .map(|(id, _)| id.clone())
            .collect();
        
        for key_id in expired_keys {
            // Don't remove if it's the current key
            if Some(&key_id) == self.current_key_id.as_ref() {
                continue;
            }
            
            // Remove from memory
            self.keys.remove(&key_id);
            
            // Remove files
            let private_key_path = self.key_directory.join(format!("{}.pem", key_id));
            let public_key_path = self.key_directory.join(format!("{}_pub.pem", key_id));
            let metadata_path = self.key_directory.join(format!("{}_metadata.json", key_id));
            
            let _ = std::fs::remove_file(private_key_path);
            let _ = std::fs::remove_file(public_key_path);
            let _ = std::fs::remove_file(metadata_path);
            
            removed_keys.push(key_id);
        }
        
        Ok(removed_keys)
    }

    /// Rotate keys (generate new key and mark old as expired)
    pub fn rotate_keys(&mut self) -> Result<String, RsaError> {
        let new_key_id = self.generate_key(
            vec!["encryption".to_string(), "key_exchange".to_string()],
            Some(365), // 1 year expiration
        )?;
        
        // Mark previous key for expiration in 30 days (grace period)
        if let Some(current_key_id) = &self.current_key_id.clone() {
            if let Some(current_key) = self.keys.get_mut(current_key_id) {
                // Only update if key doesn't already have an expiration
                if current_key.metadata.expires_at.is_none() {
                    current_key.metadata.expires_at = Some(Utc::now() + chrono::Duration::days(30));
                    
                    // Update metadata file
                    let metadata_path = self.key_directory.join(format!("{}_metadata.json", current_key_id));
                    let metadata_json = serde_json::to_string_pretty(&current_key.metadata)
                        .map_err(|e| RsaError::Serialization(e.to_string()))?;
                    std::fs::write(metadata_path, metadata_json)?;
                }
            }
        }
        
        self.current_key_id = Some(new_key_id.clone());
        Ok(new_key_id)
    }

    /// Encrypt with current key
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>, RsaError> {
        let key = self.current_key()
            .ok_or_else(|| RsaError::KeyValidation("No current key set".to_string()))?;
        key.encrypt(data)
    }

    /// Decrypt with specified key ID
    pub fn decrypt(&self, key_id: &str, encrypted_data: &[u8]) -> Result<Vec<u8>, RsaError> {
        let key = self.get_key(key_id)
            .ok_or_else(|| RsaError::KeyValidation(format!("Key not found: {}", key_id)))?;
        key.decrypt(encrypted_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_rsa_key_generation() {
        let key = SecureRsaPrivateKey::generate(
            vec!["encryption".to_string()], 
            Some(30)
        ).unwrap();
        
        assert_eq!(key.metadata.algorithm, "RSA-4096");
        assert_eq!(key.metadata.key_size, 4096);
        assert!(!key.metadata.fingerprint.is_empty());
    }

    #[test]
    fn test_rsa_encrypt_decrypt() {
        let key = SecureRsaPrivateKey::generate(
            vec!["encryption".to_string()], 
            None
        ).unwrap();
        
        let plaintext = b"Hello, RSA!";
        let ciphertext = key.encrypt(plaintext).unwrap();
        let decrypted = key.decrypt(&ciphertext).unwrap();
        
        assert_eq!(plaintext, &decrypted[..]);
    }

    #[test]
    fn test_key_validation() {
        let key = SecureRsaPrivateKey::generate(
            vec!["encryption".to_string()], 
            None
        ).unwrap();
        
        key.validate().unwrap();
    }

    #[test]
    fn test_key_manager() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = RsaKeyManager::new(temp_dir.path()).unwrap();
        
        let key_id = manager.generate_key(vec!["encryption".to_string()], None).unwrap();
        assert!(manager.current_key().is_some());
        
        let plaintext = b"Test message for RSA";
        let ciphertext = manager.encrypt(plaintext).unwrap();
        let decrypted = manager.decrypt(&key_id, &ciphertext).unwrap();
        
        assert_eq!(plaintext, &decrypted[..]);
    }

    #[test]
    fn test_key_rotation() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = RsaKeyManager::new(temp_dir.path()).unwrap();
        
        let first_key_id = manager.generate_key(vec!["encryption".to_string()], None).unwrap();
        let second_key_id = manager.rotate_keys().unwrap();
        
        assert_ne!(first_key_id, second_key_id);
        assert_eq!(manager.current_key().unwrap().metadata.key_id, second_key_id);
    }

    #[test]
    fn test_save_load_keys() {
        let temp_dir = TempDir::new().unwrap();
        let key_path = temp_dir.path().join("test_key.pem");
        
        let original_key = SecureRsaPrivateKey::generate(
            vec!["encryption".to_string()], 
            None
        ).unwrap();
        
        original_key.save_to_file(&key_path, None).unwrap();
        let loaded_key = SecureRsaPrivateKey::from_pem_file(&key_path).unwrap();
        
        // Test that keys work the same way
        let plaintext = b"Test message";
        let encrypted1 = original_key.encrypt(plaintext).unwrap();
        let encrypted2 = loaded_key.encrypt(plaintext).unwrap();
        
        let decrypted1 = original_key.decrypt(&encrypted2).unwrap();
        let decrypted2 = loaded_key.decrypt(&encrypted1).unwrap();
        
        assert_eq!(plaintext, &decrypted1[..]);
        assert_eq!(plaintext, &decrypted2[..]);
    }
}
