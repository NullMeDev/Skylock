//! AES-256-GCM encryption for backup archives
//!
//! This module provides authenticated encryption using AES-256-GCM with:
//! - 256-bit keys derived from user password via Argon2id (RFC 9106)
//! - Random 96-bit nonces for each encryption operation
//! - Authentication tags to verify data integrity
//! - Associated authenticated data (AAD) binding for metadata

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng, Payload},
    Aes256Gcm, Nonce,
};
use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{SaltString, PasswordHasher},
};
use rand::RngCore;
use serde::{Serialize, Deserialize};
use zeroize::Zeroizing;
use crate::error::{Result, SkylockError};

/// Argon2id KDF parameters (RFC 9106 compliant)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KdfParams {
    /// Algorithm identifier (always "Argon2id")
    pub algorithm: String,
    /// Memory cost in KiB (minimum 65536 = 64 MiB)
    pub memory_cost: u32,
    /// Time cost (iterations, minimum 3)
    pub time_cost: u32,
    /// Parallelism factor
    pub parallelism: u32,
    /// Salt (base64 encoded)
    pub salt: String,
    /// Version
    pub version: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            algorithm: "Argon2id".to_string(),
            memory_cost: 65536,  // 64 MiB (NIST minimum)
            time_cost: 3,         // 3 iterations (RFC 9106)
            parallelism: 1,       // Single-threaded
            salt: String::new(),
            version: 0x13,        // Argon2 v1.3
        }
    }
}

impl KdfParams {
    /// Create "paranoid" parameters for highly sensitive data
    pub fn paranoid() -> Self {
        Self {
            algorithm: "Argon2id".to_string(),
            memory_cost: 262144,  // 256 MiB
            time_cost: 5,         // 5 iterations
            parallelism: 4,       // 4 threads
            salt: String::new(),
            version: 0x13,
        }
    }
}

pub struct EncryptionManager {
    cipher: Aes256Gcm,
    kdf_params: KdfParams,
}

impl EncryptionManager {
    /// Create a new encryption manager from a password using Argon2id
    /// 
    /// This uses RFC 9106 compliant parameters:
    /// - Memory: 64 MiB minimum (NIST SP 800-175B)
    /// - Iterations: 3 minimum
    /// - Algorithm: Argon2id (resistant to side-channel and GPU attacks)
    pub fn new(password: &str) -> Result<Self> {
        Self::new_with_params(password, KdfParams::default())
    }
    
    /// Create encryption manager with custom KDF parameters
    pub fn new_with_params(password: &str, mut params: KdfParams) -> Result<Self> {
        // Generate cryptographically secure salt
        let salt = SaltString::generate(&mut OsRng);
        params.salt = salt.to_string();
        
        Self::from_password_and_params(password, &params)
    }
    
    /// Restore encryption manager from existing KDF parameters (for decryption)
    pub fn from_password_and_params(password: &str, params: &KdfParams) -> Result<Self> {
        // Validate parameters meet minimum security requirements
        if params.memory_cost < 65536 {
            return Err(SkylockError::Encryption(
                format!("Insecure memory cost: {} KiB (minimum 65536)", params.memory_cost)
            ));
        }
        if params.time_cost < 3 {
            return Err(SkylockError::Encryption(
                format!("Insecure iteration count: {} (minimum 3)", params.time_cost)
            ));
        }
        
        // Parse salt
        let salt = SaltString::from_b64(&params.salt)
            .map_err(|e| SkylockError::Encryption(format!("Invalid salt: {}", e)))?;
        
        // Configure Argon2id
        let argon2_params = Params::new(
            params.memory_cost,
            params.time_cost,
            params.parallelism,
            Some(32), // 256-bit output
        ).map_err(|e| SkylockError::Encryption(format!("Invalid Argon2 params: {}", e)))?;
        
        let argon2 = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            argon2_params,
        );
        
        // Derive key using Argon2id (secure against GPU attacks)
        let mut key_bytes = Zeroizing::new([0u8; 32]);
        argon2
            .hash_password_into(password.as_bytes(), salt.as_str().as_bytes(), &mut *key_bytes)
            .map_err(|e| SkylockError::Encryption(format!("Key derivation failed: {}", e)))?;
        
        // Create cipher
        let cipher = Aes256Gcm::new_from_slice(&*key_bytes)
            .map_err(|e| SkylockError::Encryption(format!("Failed to create cipher: {}", e)))?;
        
        // Key bytes automatically zeroized when dropped
        
        Ok(Self { 
            cipher,
            kdf_params: params.clone(),
        })
    }
    
    /// Get the KDF parameters (needed for decryption)
    pub fn kdf_params(&self) -> &KdfParams {
        &self.kdf_params
    }
    
    /// Encrypt data with AES-256-GCM using Associated Authenticated Data (AAD)
    /// 
    /// AAD binds metadata to the ciphertext, preventing:
    /// - Ciphertext transplant between backups
    /// - File replay attacks
    /// - Path manipulation
    /// 
    /// Returns: [12-byte nonce][encrypted data with auth tag]
    pub fn encrypt_with_aad(
        &self,
        plaintext: &[u8],
        backup_id: &str,
        file_path: &str,
    ) -> Result<Vec<u8>> {
        // Generate a random 96-bit (12-byte) nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        // Construct AAD: backup_id|algorithm|version|file_path
        let aad = format!("{}|AES-256-GCM|v2|{}", backup_id, file_path);
        
        // Create payload with AAD
        let payload = Payload {
            msg: plaintext,
            aad: aad.as_bytes(),
        };
        
        // Encrypt with AAD binding
        let ciphertext = self.cipher
            .encrypt(nonce, payload)
            .map_err(|e| SkylockError::Encryption(format!("Encryption failed: {}", e)))?;
        
        // Prepend nonce to ciphertext: [nonce][ciphertext+tag]
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        
        Ok(result)
    }
    
    /// Decrypt data encrypted with AES-256-GCM and AAD
    /// 
    /// Expects: [12-byte nonce][encrypted data with auth tag]
    /// The same AAD used during encryption must be provided
    pub fn decrypt_with_aad(
        &self,
        ciphertext: &[u8],
        backup_id: &str,
        file_path: &str,
    ) -> Result<Vec<u8>> {
        if ciphertext.len() < 12 {
            return Err(SkylockError::Encryption(
                "Ciphertext too short (missing nonce)".to_string()
            ));
        }
        
        // Extract nonce (first 12 bytes)
        let (nonce_bytes, encrypted_data) = ciphertext.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        
        // Reconstruct AAD (must match encryption)
        let aad = format!("{}|AES-256-GCM|v2|{}", backup_id, file_path);
        
        // Create payload with AAD
        let payload = Payload {
            msg: encrypted_data,
            aad: aad.as_bytes(),
        };
        
        // Decrypt and verify authentication tag + AAD
        let plaintext = self.cipher
            .decrypt(nonce, payload)
            .map_err(|e| SkylockError::Encryption(
                format!("Decryption failed (wrong key, corrupted data, or AAD mismatch): {}", e)
            ))?;
        
        Ok(plaintext)
    }
    
    /// Legacy encrypt without AAD (for backward compatibility)
    /// 
    /// ⚠️  DEPRECATED: Use encrypt_with_aad() for new backups
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let ciphertext = self.cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| SkylockError::Encryption(format!("Encryption failed: {}", e)))?;
        
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        
        Ok(result)
    }
    
    /// Legacy decrypt without AAD (for backward compatibility)
    /// 
    /// ⚠️  DEPRECATED: Use decrypt_with_aad() for new backups
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < 12 {
            return Err(SkylockError::Encryption(
                "Ciphertext too short (missing nonce)".to_string()
            ));
        }
        
        let (nonce_bytes, encrypted_data) = ciphertext.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        
        let plaintext = self.cipher
            .decrypt(nonce, encrypted_data)
            .map_err(|e| SkylockError::Encryption(format!("Decryption failed (wrong key or corrupted data): {}", e)))?;
        
        Ok(plaintext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_encrypt_decrypt() {
        let manager = EncryptionManager::new("test_password_123").unwrap();
        let plaintext = b"Hello, this is a secret message!";
        
        let encrypted = manager.encrypt(plaintext).unwrap();
        assert_ne!(encrypted.as_slice(), plaintext);
        assert!(encrypted.len() > plaintext.len()); // Has nonce + tag
        
        let decrypted = manager.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted.as_slice(), plaintext);
    }
    
    #[test]
    fn test_different_nonces() {
        let manager = EncryptionManager::new("test_password_123").unwrap();
        let plaintext = b"Same message";
        
        let encrypted1 = manager.encrypt(plaintext).unwrap();
        let encrypted2 = manager.encrypt(plaintext).unwrap();
        
        // Same plaintext should produce different ciphertext (different nonces)
        assert_ne!(encrypted1, encrypted2);
        
        // Both should decrypt to same plaintext
        assert_eq!(manager.decrypt(&encrypted1).unwrap(), plaintext);
        assert_eq!(manager.decrypt(&encrypted2).unwrap(), plaintext);
    }
    
    #[test]
    fn test_wrong_password() {
        let manager1 = EncryptionManager::new("password1").unwrap();
        let manager2 = EncryptionManager::new("password2").unwrap();
        
        let encrypted = manager1.encrypt(b"secret").unwrap();
        
        // Wrong password should fail to decrypt
        assert!(manager2.decrypt(&encrypted).is_err());
    }
}
