//! AES-256-GCM encryption for backup archives
//!
//! This module provides authenticated encryption using AES-256-GCM with:
//! - 256-bit keys derived from user password via SHA-256
//! - Random 96-bit nonces for each encryption operation
//! - Authentication tags to verify data integrity

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use crate::error::{Result, SkylockError};

pub struct EncryptionManager {
    cipher: Aes256Gcm,
}

impl EncryptionManager {
    /// Create a new encryption manager from a password/key
    /// The key is hashed with SHA-256 to produce a 256-bit encryption key
    pub fn new(password: &str) -> Result<Self> {
        // Derive a 256-bit key from the password using SHA-256
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        let key_bytes = hasher.finalize();
        
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| SkylockError::Encryption(format!("Failed to create cipher: {}", e)))?;
        
        Ok(Self { cipher })
    }
    
    /// Encrypt data with AES-256-GCM
    /// Returns: [12-byte nonce][encrypted data with auth tag]
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        // Generate a random 96-bit (12-byte) nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        // Encrypt the data
        let ciphertext = self.cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| SkylockError::Encryption(format!("Encryption failed: {}", e)))?;
        
        // Prepend nonce to ciphertext: [nonce][ciphertext+tag]
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);
        
        Ok(result)
    }
    
    /// Decrypt data encrypted with AES-256-GCM
    /// Expects: [12-byte nonce][encrypted data with auth tag]
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < 12 {
            return Err(SkylockError::Encryption(
                "Ciphertext too short (missing nonce)".to_string()
            ));
        }
        
        // Extract nonce (first 12 bytes)
        let (nonce_bytes, encrypted_data) = ciphertext.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        
        // Decrypt and verify authentication tag
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
