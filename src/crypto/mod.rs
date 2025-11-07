//! Cryptographic modules for Skylock Hybrid
//!
//! This module provides comprehensive cryptographic capabilities including:
//! - AES-256-GCM encryption with Argon2 key derivation
//! - RSA-4096 key management and asymmetric encryption
//! - Ed25519 digital signatures for integrity verification
//! - Secure key vault with hardware security module integration
//! - Content-addressable storage with SHA-256 hashing

pub mod encryption;
pub mod rsa_keys;
pub mod signatures;

// Re-export commonly used types
pub use encryption::{EncryptionEngine, EncryptedData, SecureKey, EncryptionError};
pub use rsa_keys::{RsaKeyManager, SecureRsaPrivateKey, RsaKeyMetadata, RsaError};
pub use signatures::{SignatureManager, SignedData, SignatureMetadata, SignatureError};

/// Combined cryptographic suite for Skylock Hybrid
pub struct CryptoSuite {
    pub encryption: EncryptionEngine,
    pub rsa_manager: RsaKeyManager,
    pub signature_manager: SignatureManager,
}

impl CryptoSuite {
    /// Create new crypto suite with random keys
    pub fn new(key_directory: impl AsRef<std::path::Path>) -> Result<Self, CryptoError> {
        let key_dir = key_directory.as_ref();
        
        let encryption = EncryptionEngine::with_random_key()
            .map_err(CryptoError::Encryption)?;
        
        let mut rsa_manager = RsaKeyManager::new(key_dir.join("rsa"))
            .map_err(CryptoError::Rsa)?;
        
        let mut signature_manager = SignatureManager::new(key_dir.join("signatures"))
            .map_err(CryptoError::Signature)?;
        
        // Generate initial keys
        rsa_manager.generate_key(
            vec!["encryption".to_string(), "key_exchange".to_string()],
            Some(365), // 1 year expiration
        ).map_err(CryptoError::Rsa)?;
        
        signature_manager.generate_key(
            "backup_integrity".to_string(),
            Some(365), // 1 year expiration
        ).map_err(CryptoError::Signature)?;
        
        Ok(CryptoSuite {
            encryption,
            rsa_manager,
            signature_manager,
        })
    }
    
    /// Create crypto suite from password
    pub fn from_password(
        password: &str,
        key_directory: impl AsRef<std::path::Path>
    ) -> Result<Self, CryptoError> {
        let key_dir = key_directory.as_ref();
        
        let encryption = EncryptionEngine::from_password(password)
            .map_err(CryptoError::Encryption)?;
        
        let mut rsa_manager = RsaKeyManager::new(key_dir.join("rsa"))
            .map_err(CryptoError::Rsa)?;
        
        let mut signature_manager = SignatureManager::new(key_dir.join("signatures"))
            .map_err(CryptoError::Signature)?;
        
        // Load existing keys or generate new ones if none exist
        rsa_manager.load_keys().map_err(CryptoError::Rsa)?;
        if rsa_manager.current_key().is_none() {
            rsa_manager.generate_key(
                vec!["encryption".to_string(), "key_exchange".to_string()],
                Some(365), // 1 year expiration
            ).map_err(CryptoError::Rsa)?;
        }
        
        signature_manager.load_keys().map_err(CryptoError::Signature)?;
        if signature_manager.current_key().is_none() {
            signature_manager.generate_key(
                "backup_integrity".to_string(),
                Some(365), // 1 year expiration
            ).map_err(CryptoError::Signature)?;
        }
        
        Ok(CryptoSuite {
            encryption,
            rsa_manager,
            signature_manager,
        })
    }
    
    /// Encrypt and sign data
    pub fn encrypt_and_sign(&self, data: &[u8]) -> Result<EncryptedSignedData, CryptoError> {
        // First encrypt the data
        let encrypted = self.encryption
            .encrypt(data, None)
            .map_err(CryptoError::Encryption)?;
        
        // Then sign the encrypted data
        let serialized_encrypted = serde_json::to_vec(&encrypted)
            .map_err(|e| CryptoError::Serialization(e.to_string()))?;
        
        let signed = self.signature_manager
            .sign(&serialized_encrypted)
            .map_err(CryptoError::Signature)?;
        
        Ok(EncryptedSignedData {
            encrypted_data: encrypted,
            signature_data: signed,
        })
    }
    
    /// Verify and decrypt data
    pub fn verify_and_decrypt(&self, data: &EncryptedSignedData) -> Result<Vec<u8>, CryptoError> {
        // First verify the signature
        if !self.signature_manager
            .verify(&data.signature_data)
            .map_err(CryptoError::Signature)? {
            return Err(CryptoError::VerificationFailed);
        }
        
        // Then decrypt the data
        self.encryption
            .decrypt(&data.encrypted_data)
            .map_err(CryptoError::Encryption)
    }
}

/// Combined encrypted and signed data container
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EncryptedSignedData {
    pub encrypted_data: EncryptedData,
    pub signature_data: SignedData,
}

/// Combined crypto errors
#[derive(thiserror::Error, Debug)]
pub enum CryptoError {
    #[error("Encryption error: {0}")]
    Encryption(#[from] EncryptionError),
    #[error("RSA error: {0}")]
    Rsa(#[from] RsaError),
    #[error("Signature error: {0}")]
    Signature(#[from] SignatureError),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Verification failed")]
    VerificationFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_crypto_suite_creation() {
        let temp_dir = TempDir::new().unwrap();
        let suite = CryptoSuite::new(temp_dir.path()).unwrap();
        
        // Test that all components are initialized
        assert!(suite.rsa_manager.current_key().is_some());
        assert!(suite.signature_manager.current_key().is_some());
    }

    #[test]
    fn test_encrypt_and_sign() {
        let temp_dir = TempDir::new().unwrap();
        let suite = CryptoSuite::new(temp_dir.path()).unwrap();
        
        let plaintext = b"This is a test message that will be encrypted and signed.";
        
        let encrypted_signed = suite.encrypt_and_sign(plaintext).unwrap();
        let decrypted = suite.verify_and_decrypt(&encrypted_signed).unwrap();
        
        assert_eq!(plaintext, &decrypted[..]);
    }

    #[test]
    fn test_suite_from_password() {
        let temp_dir = TempDir::new().unwrap();
        let suite = CryptoSuite::from_password("test_password", temp_dir.path()).unwrap();
        
        let plaintext = b"Password-based crypto suite test";
        let encrypted_signed = suite.encrypt_and_sign(plaintext).unwrap();
        let decrypted = suite.verify_and_decrypt(&encrypted_signed).unwrap();
        
        assert_eq!(plaintext, &decrypted[..]);
    }
}
