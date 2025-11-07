//! AES-256-GCM Encryption Engine for Skylock Hybrid
//! 
//! This module provides secure encryption and decryption capabilities using
//! AES-256-GCM with AEAD (Authenticated Encryption with Associated Data).

use skylock_core::{
    Error, 
    ErrorCategory, 
    ErrorSeverity,
    error_types::SecurityErrorType,
};

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce, Key,
};
use argon2::{
    password_hash::{rand_core::RngCore, PasswordHash, PasswordHasher, SaltString},
    Argon2, PasswordVerifier,
};
use sha2::{Digest, Sha256};
use std::fmt;
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Encryption errors
#[derive(Error, Debug)]
pub enum EncryptionError {
    #[error("Key derivation failed: {0}")]
    KeyDerivation(String),
    #[error("Encryption failed: {0}")]
    Encryption(String),
    #[error("Decryption failed: {0}")]
    Decryption(String),
    #[error("Invalid key format: {0}")]
    InvalidKey(String),
    #[error("Invalid nonce size")]
    InvalidNonce,
    #[error("Authentication failed")]
    AuthenticationFailed,
}

/// Secure key material that gets zeroized on drop
#[derive(Clone, zeroize::ZeroizeOnDrop)]
pub struct SecureKey {
    #[zeroize(skip)]
    pub algorithm: String,
    pub key_data: Vec<u8>,
    pub salt: Vec<u8>,
    pub iterations: u32,
}

impl SecureKey {
    /// Generate a new secure key from password using Argon2
    pub fn from_password(password: &str, salt: Option<&[u8]>) -> Result<Self, EncryptionError> {
        let salt = match salt {
            Some(s) => s.to_vec(),
            None => {
                let mut salt_bytes = [0u8; 32];
                OsRng.fill_bytes(&mut salt_bytes);
                salt_bytes.to_vec()
            }
        };

        let salt_string = SaltString::encode_b64(&salt)
            .map_err(|e| EncryptionError::KeyDerivation(e.to_string()))?;
        
        let argon2 = Argon2::default();
        let mut key_data = vec![0u8; 32]; // 256 bits for AES-256
        argon2
            .hash_password_into(
                password.as_bytes(),
                salt_string.as_str().as_bytes(),
                &mut key_data,
            )
            .map_err(|e| EncryptionError::KeyDerivation(e.to_string()))?;
        
        Ok(SecureKey {
            algorithm: "AES-256-GCM".to_string(),
            key_data,
            salt,
            iterations: 600_000, // OWASP recommended minimum
        })
    }

    /// Generate a random 256-bit key
    pub fn generate_random() -> Self {
        let mut key_data = vec![0u8; 32]; // 256 bits
        OsRng.fill_bytes(&mut key_data);
        
        let mut salt = vec![0u8; 32];
        OsRng.fill_bytes(&mut salt);

        SecureKey {
            algorithm: "AES-256-GCM".to_string(),
            key_data,
            salt,
            iterations: 0, // No password derivation
        }
    }

    /// Get the raw key bytes for AES
    pub fn as_aes_key(&self) -> Result<&Key<Aes256Gcm>, EncryptionError> {
        if self.key_data.len() != 32 {
            return Err(EncryptionError::InvalidKey("Key must be 32 bytes".to_string()));
        }
        
        let key = Key::<Aes256Gcm>::from_slice(&self.key_data);
        Ok(key)
    }

    /// Derive key fingerprint for identification
    pub fn fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(&self.key_data);
        hasher.update(&self.salt);
        let hash = hasher.finalize();
        hex::encode(&hash[..8]) // First 8 bytes as hex
    }
}

impl fmt::Debug for SecureKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecureKey")
            .field("algorithm", &self.algorithm)
            .field("salt", &hex::encode(&self.salt))
            .field("iterations", &self.iterations)
            .field("fingerprint", &self.fingerprint())
            .finish()
    }
}

/// Encrypted data container
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EncryptedData {
    pub algorithm: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub key_fingerprint: String,
    pub metadata: Option<Vec<u8>>, // Additional authenticated data
}

/// AES-256-GCM encryption engine
pub struct EncryptionEngine {
    key: SecureKey,
    cipher: Aes256Gcm,
}

impl EncryptionEngine {
    /// Create new encryption engine with the given key
    pub fn new(key: SecureKey) -> Result<Self, EncryptionError> {
        let aes_key = key.as_aes_key()?;
        let cipher = Aes256Gcm::new(aes_key);
        
        Ok(EncryptionEngine { key, cipher })
    }

    /// Create encryption engine from password
    pub fn from_password(password: &str) -> Result<Self, EncryptionError> {
        let key = SecureKey::from_password(password, None)?;
        Self::new(key)
    }

    /// Create encryption engine with random key
    pub fn with_random_key() -> Result<Self, EncryptionError> {
        let key = SecureKey::generate_random();
        Self::new(key)
    }

    /// Encrypt data with optional additional authenticated data (AAD)
    pub fn encrypt(&self, data: &[u8], aad: Option<&[u8]>) -> Result<EncryptedData, EncryptionError> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        
        let ciphertext = if let Some(additional_data) = aad {
            self.cipher
                .encrypt(&nonce, aes_gcm::aead::Payload { msg: data, aad: additional_data })
                .map_err(|e| EncryptionError::Encryption(e.to_string()))?
        } else {
            self.cipher
                .encrypt(&nonce, data)
                .map_err(|e| EncryptionError::Encryption(e.to_string()))?
        };

        Ok(EncryptedData {
            algorithm: self.key.algorithm.clone(),
            nonce: nonce.to_vec(),
            ciphertext,
            key_fingerprint: self.key.fingerprint(),
            metadata: aad.map(|a| a.to_vec()),
        })
    }

    /// Decrypt encrypted data
    pub fn decrypt(&self, encrypted: &EncryptedData) -> Result<Vec<u8>, EncryptionError> {
        // Verify key fingerprint matches
        if encrypted.key_fingerprint != self.key.fingerprint() {
            return Err(EncryptionError::AuthenticationFailed);
        }

        // Verify algorithm
        if encrypted.algorithm != self.key.algorithm {
            return Err(EncryptionError::InvalidKey(
                format!("Algorithm mismatch: expected {}, got {}", 
                       self.key.algorithm, encrypted.algorithm)
            ));
        }

        let nonce = Nonce::from_slice(&encrypted.nonce);
        
        let plaintext = if let Some(ref aad) = encrypted.metadata {
            self.cipher
                .decrypt(nonce, aes_gcm::aead::Payload { 
                    msg: &encrypted.ciphertext, 
                    aad 
                })
                .map_err(|e| EncryptionError::Decryption(e.to_string()))?
        } else {
            self.cipher
                .decrypt(nonce, encrypted.ciphertext.as_slice())
                .map_err(|e| EncryptionError::Decryption(e.to_string()))?
        };

        Ok(plaintext)
    }

    /// Get key information (without exposing key material)
    pub fn key_info(&self) -> (String, String, u32) {
        (
            self.key.algorithm.clone(),
            self.key.fingerprint(),
            self.key.iterations,
        )
    }

    /// Encrypt large data streams in chunks
    pub fn encrypt_stream<R: std::io::Read, W: std::io::Write>(
        &self,
        mut reader: R,
        mut writer: W,
        chunk_size: usize,
    ) -> Result<u64, EncryptionError> {
        let mut total_bytes = 0u64;
        let mut chunk_buffer = vec![0u8; chunk_size];
        let mut chunk_index = 0u64;

        loop {
            let bytes_read = reader
                .read(&mut chunk_buffer)
                .map_err(|e| EncryptionError::Encryption(format!("Read error: {}", e)))?;

            if bytes_read == 0 {
                break; // End of stream
            }

            let chunk_data = &chunk_buffer[..bytes_read];
            
            // Use chunk index as additional authenticated data for ordering
            let aad = chunk_index.to_le_bytes();
            let encrypted_chunk = self.encrypt(chunk_data, Some(&aad))?;
            
            // Write chunk size as u32 little-endian
            let serialized = bincode::serialize(&encrypted_chunk)
                .map_err(|e| EncryptionError::Encryption(format!("Serialization error: {}", e)))?;
            
            writer.write_all(&(serialized.len() as u32).to_le_bytes())
                .map_err(|e| EncryptionError::Encryption(format!("Write error: {}", e)))?;
            
            writer.write_all(&serialized)
                .map_err(|e| EncryptionError::Encryption(format!("Write error: {}", e)))?;

            total_bytes += bytes_read as u64;
            chunk_index += 1;
        }

        Ok(total_bytes)
    }

    /// Decrypt large data streams in chunks
    pub fn decrypt_stream<R: std::io::Read, W: std::io::Write>(
        &self,
        mut reader: R,
        mut writer: W,
    ) -> Result<u64, EncryptionError> {
        let mut total_bytes = 0u64;
        let mut chunk_index = 0u64;

        loop {
            // Read chunk size
            let mut size_bytes = [0u8; 4];
            match reader.read_exact(&mut size_bytes) {
                Ok(_) => {},
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(EncryptionError::Decryption(format!("Read error: {}", e))),
            }

            let chunk_size = u32::from_le_bytes(size_bytes) as usize;
            let mut chunk_buffer = vec![0u8; chunk_size];
            
            reader.read_exact(&mut chunk_buffer)
                .map_err(|e| EncryptionError::Decryption(format!("Read error: {}", e)))?;

            let encrypted_chunk: EncryptedData = bincode::deserialize(&chunk_buffer)
                .map_err(|e| EncryptionError::Decryption(format!("Deserialization error: {}", e)))?;

            let decrypted_data = self.decrypt(&encrypted_chunk)?;
            
            writer.write_all(&decrypted_data)
                .map_err(|e| EncryptionError::Decryption(format!("Write error: {}", e)))?;

            total_bytes += decrypted_data.len() as u64;
            chunk_index += 1;
        }

        Ok(total_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation() {
        let key = SecureKey::generate_random();
        assert_eq!(key.key_data.len(), 32);
        assert_eq!(key.algorithm, "AES-256-GCM");
    }

    #[test]
    fn test_password_key_derivation() {
        let key = SecureKey::from_password("test_password", None).unwrap();
        assert_eq!(key.algorithm, "AES-256-GCM");
        assert!(!key.salt.is_empty());
        assert_eq!(key.iterations, 600_000);
        
        // Same password should produce same key with same salt
        let key2 = SecureKey::from_password("test_password", Some(&key.salt)).unwrap();
        assert_eq!(key.fingerprint(), key2.fingerprint());
    }

    #[test]
    fn test_basic_encryption_decryption() {
        let engine = EncryptionEngine::with_random_key().unwrap();
        let plaintext = b"Hello, World! This is a test message.";
        
        let encrypted = engine.encrypt(plaintext, None).unwrap();
        let decrypted = engine.decrypt(&encrypted).unwrap();
        
        assert_eq!(plaintext, &decrypted[..]);
    }

    #[test]
    fn test_encryption_with_aad() {
        let engine = EncryptionEngine::with_random_key().unwrap();
        let plaintext = b"Secret message";
        let aad = b"additional authenticated data";
        
        let encrypted = engine.encrypt(plaintext, Some(aad)).unwrap();
        let decrypted = engine.decrypt(&encrypted).unwrap();
        
        assert_eq!(plaintext, &decrypted[..]);
        assert_eq!(encrypted.metadata, Some(aad.to_vec()));
    }

    #[test]
    fn test_wrong_key_fails() {
        let engine1 = EncryptionEngine::with_random_key().unwrap();
        let engine2 = EncryptionEngine::with_random_key().unwrap();
        
        let encrypted = engine1.encrypt(b"test", None).unwrap();
        let result = engine2.decrypt(&encrypted);
        
        assert!(matches!(result, Err(EncryptionError::AuthenticationFailed)));
    }

    #[test]
    fn test_stream_encryption() {
        let engine = EncryptionEngine::with_random_key().unwrap();
        let data = b"This is a longer message that will be encrypted in chunks to test stream encryption functionality.";
        
        let mut encrypted_buffer = Vec::new();
        let mut cursor = std::io::Cursor::new(data);
        let bytes_encrypted = engine.encrypt_stream(&mut cursor, &mut encrypted_buffer, 32).unwrap();
        
        assert_eq!(bytes_encrypted, data.len() as u64);
        assert!(!encrypted_buffer.is_empty());
        
        // Decrypt the stream
        let mut decrypted_buffer = Vec::new();
        let mut encrypted_cursor = std::io::Cursor::new(&encrypted_buffer);
        let bytes_decrypted = engine.decrypt_stream(&mut encrypted_cursor, &mut decrypted_buffer).unwrap();
        
        assert_eq!(bytes_decrypted, data.len() as u64);
        assert_eq!(&decrypted_buffer[..], data);
    }
}
