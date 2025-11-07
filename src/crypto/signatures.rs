//! Ed25519 Digital Signatures for Skylock Hybrid
//! 
//! This module provides Ed25519 digital signature capabilities for backup
//! integrity verification and authenticity checks.

use ed25519_dalek::{
    Signature, Signer, SigningKey, Verifier, VerifyingKey,
};
use pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;
use zeroize::{Zeroize, ZeroizeOnDrop};
use chrono::{DateTime, Utc};

/// Digital signature errors
#[derive(Error, Debug)]
pub enum SignatureError {
    #[error("Key generation failed: {0}")]
    KeyGeneration(String),
    #[error("Key loading failed: {0}")]
    KeyLoading(String),
    #[error("Key saving failed: {0}")]
    KeySaving(String),
    #[error("Signing failed: {0}")]
    Signing(String),
    #[error("Verification failed: {0}")]
    Verification(String),
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Digital signature metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SignatureMetadata {
    pub key_id: String,
    pub algorithm: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub purpose: String, // e.g., "backup_integrity", "metadata_auth", "chain_of_trust"
    pub public_key_hex: String,
    pub fingerprint: String,
}

/// Signed data container
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SignedData {
    pub data: Vec<u8>,
    pub signature: Vec<u8>,
    pub metadata: SignatureMetadata,
    pub timestamp: DateTime<Utc>,
    pub content_hash: String, // SHA-256 hash of original data
}

/// Ed25519 signing key with secure storage
#[derive(zeroize::ZeroizeOnDrop)]
pub struct SecureSigningKey {
    #[zeroize(skip)]
    pub metadata: SignatureMetadata,
    pub signing_key: SigningKey,
    #[zeroize(skip)]
    pub verifying_key: VerifyingKey,
}

impl SecureSigningKey {
    /// Generate a new Ed25519 key pair
    pub fn generate(purpose: String, expires_in_days: Option<u32>) -> Result<Self, SignatureError> {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        
        let key_id = Self::generate_key_id();
        let created_at = Utc::now();
        let expires_at = expires_in_days.map(|days| created_at + chrono::Duration::days(days as i64));
        
        let public_key_hex = hex::encode(verifying_key.as_bytes());
        let fingerprint = Self::calculate_fingerprint(&verifying_key);

        let metadata = SignatureMetadata {
            key_id,
            algorithm: "Ed25519".to_string(),
            created_at,
            expires_at,
            purpose,
            public_key_hex,
            fingerprint,
        };

        Ok(SecureSigningKey {
            metadata,
            signing_key,
            verifying_key,
        })
    }

    /// Load signing key from PEM file
    pub fn from_pem_file<P: AsRef<Path>>(path: P) -> Result<Self, SignatureError> {
        let pem_data = std::fs::read_to_string(&path)?;
        Self::from_pem(&pem_data)
    }

    /// Load signing key from PEM string
    pub fn from_pem(pem_data: &str) -> Result<Self, SignatureError> {
        let signing_key = SigningKey::from_pkcs8_pem(pem_data)
            .map_err(|e| SignatureError::KeyLoading(e.to_string()))?;
        
        let verifying_key = signing_key.verifying_key();
        let key_id = Self::generate_key_id();
        let public_key_hex = hex::encode(verifying_key.as_bytes());
        let fingerprint = Self::calculate_fingerprint(&verifying_key);

        let metadata = SignatureMetadata {
            key_id,
            algorithm: "Ed25519".to_string(),
            created_at: Utc::now(),
            expires_at: None,
            purpose: "general".to_string(),
            public_key_hex,
            fingerprint,
        };

        Ok(SecureSigningKey {
            metadata,
            signing_key,
            verifying_key,
        })
    }

    /// Save signing key to PEM file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), SignatureError> {
        let pem_data = self.signing_key
            .to_pkcs8_pem(LineEnding::LF)
            .map_err(|e| SignatureError::KeySaving(e.to_string()))?;

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
    pub fn save_public_key_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), SignatureError> {
        let pem_data = self.verifying_key
            .to_public_key_pem(LineEnding::LF)
            .map_err(|e| SignatureError::KeySaving(e.to_string()))?;
        
        std::fs::write(&path, pem_data.as_bytes())?;
        Ok(())
    }

    /// Sign data and create signed container
    pub fn sign(&self, data: &[u8]) -> Result<SignedData, SignatureError> {
        let signature = self.signing_key
            .try_sign(data)
            .map_err(|e| SignatureError::Signing(e.to_string()))?;

        let content_hash = {
            let mut hasher = Sha256::new();
            hasher.update(data);
            hex::encode(hasher.finalize())
        };

        Ok(SignedData {
            data: data.to_vec(),
            signature: signature.to_bytes().to_vec(),
            metadata: self.metadata.clone(),
            timestamp: Utc::now(),
            content_hash,
        })
    }

    /// Sign only the hash of data (for large files)
    pub fn sign_hash(&self, hash: &[u8]) -> Result<Vec<u8>, SignatureError> {
        let signature = self.signing_key
            .try_sign(hash)
            .map_err(|e| SignatureError::Signing(e.to_string()))?;
        
        Ok(signature.to_bytes().to_vec())
    }

    /// Check if the key is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.metadata.expires_at {
            expires_at < Utc::now()
        } else {
            false
        }
    }

    /// Get public key for sharing
    pub fn public_key(&self) -> VerifyingKey {
        self.verifying_key
    }

    /// Get metadata
    pub fn metadata(&self) -> &SignatureMetadata {
        &self.metadata
    }

    /// Generate a random key ID
    fn generate_key_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    /// Calculate public key fingerprint
    fn calculate_fingerprint(public_key: &VerifyingKey) -> String {
        let mut hasher = Sha256::new();
        hasher.update(public_key.as_bytes());
        let hash = hasher.finalize();
        hex::encode(&hash[..8]) // First 8 bytes as hex
    }
}

/// Public key for signature verification
pub struct PublicSignatureKey {
    pub metadata: SignatureMetadata,
    pub verifying_key: VerifyingKey,
}

impl PublicSignatureKey {
    /// Load public key from hex string
    pub fn from_hex(hex_data: &str, metadata: SignatureMetadata) -> Result<Self, SignatureError> {
        let key_bytes = hex::decode(hex_data)
            .map_err(|e| SignatureError::KeyLoading(format!("Invalid hex: {}", e)))?;
        
        if key_bytes.len() != 32 {
            return Err(SignatureError::KeyLoading("Invalid key length".to_string()));
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);
        
        let verifying_key = VerifyingKey::from_bytes(&key_array)
            .map_err(|e| SignatureError::KeyLoading(e.to_string()))?;

        Ok(PublicSignatureKey {
            metadata,
            verifying_key,
        })
    }

    /// Load public key from PEM file
    pub fn from_pem_file<P: AsRef<Path>>(path: P) -> Result<Self, SignatureError> {
        let pem_data = std::fs::read_to_string(&path)?;
        Self::from_pem(&pem_data)
    }

    /// Load public key from PEM string
    pub fn from_pem(pem_data: &str) -> Result<Self, SignatureError> {
        let verifying_key = VerifyingKey::from_public_key_pem(pem_data)
            .map_err(|e| SignatureError::KeyLoading(e.to_string()))?;

        let key_id = SecureSigningKey::generate_key_id();
        let public_key_hex = hex::encode(verifying_key.as_bytes());
        let fingerprint = SecureSigningKey::calculate_fingerprint(&verifying_key);

        let metadata = SignatureMetadata {
            key_id,
            algorithm: "Ed25519".to_string(),
            created_at: Utc::now(),
            expires_at: None,
            purpose: "verification".to_string(),
            public_key_hex,
            fingerprint,
        };

        Ok(PublicSignatureKey {
            metadata,
            verifying_key,
        })
    }

    /// Verify signed data
    pub fn verify(&self, signed_data: &SignedData) -> Result<bool, SignatureError> {
        // Check if signatures match expected key
        if signed_data.metadata.fingerprint != self.metadata.fingerprint {
            return Err(SignatureError::Verification(
                "Key fingerprint mismatch".to_string()
            ));
        }

        // Verify content hash
        let mut hasher = Sha256::new();
        hasher.update(&signed_data.data);
        let calculated_hash = hex::encode(hasher.finalize());
        
        if calculated_hash != signed_data.content_hash {
            return Err(SignatureError::Verification(
                "Content hash mismatch".to_string()
            ));
        }

        // Verify signature
        if signed_data.signature.len() != 64 {
            return Err(SignatureError::InvalidSignature);
        }

        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(&signed_data.signature);
        
        let signature = Signature::from_bytes(&sig_bytes);
        
        match self.verifying_key.verify(&signed_data.data, &signature) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Verify signature of data hash
    pub fn verify_hash(&self, hash: &[u8], signature_bytes: &[u8]) -> Result<bool, SignatureError> {
        if signature_bytes.len() != 64 {
            return Err(SignatureError::InvalidSignature);
        }

        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(signature_bytes);
        
        let signature = Signature::from_bytes(&sig_bytes);
        
        match self.verifying_key.verify(hash, &signature) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

/// Signature manager for handling multiple signing keys
pub struct SignatureManager {
    signing_keys: std::collections::HashMap<String, SecureSigningKey>,
    public_keys: std::collections::HashMap<String, PublicSignatureKey>,
    key_directory: PathBuf,
    current_key_id: Option<String>,
}

impl SignatureManager {
    /// Create new signature manager
    pub fn new<P: AsRef<Path>>(key_directory: P) -> Result<Self, SignatureError> {
        let key_dir = key_directory.as_ref().to_path_buf();
        std::fs::create_dir_all(&key_dir)?;
        
        Ok(SignatureManager {
            signing_keys: std::collections::HashMap::new(),
            public_keys: std::collections::HashMap::new(),
            key_directory: key_dir,
            current_key_id: None,
        })
    }

    /// Generate new signing key
    pub fn generate_key(
        &mut self,
        purpose: String,
        expires_in_days: Option<u32>
    ) -> Result<String, SignatureError> {
        let key = SecureSigningKey::generate(purpose, expires_in_days)?;
        let key_id = key.metadata.key_id.clone();
        
        // Save to disk
        let private_key_path = self.key_directory.join(format!("{}_sign.pem", key_id));
        let public_key_path = self.key_directory.join(format!("{}_verify.pem", key_id));
        let metadata_path = self.key_directory.join(format!("{}_metadata.json", key_id));
        
        key.save_to_file(&private_key_path)?;
        key.save_public_key_to_file(&public_key_path)?;
        
        let metadata_json = serde_json::to_string_pretty(&key.metadata)
            .map_err(|e| SignatureError::Serialization(e.to_string()))?;
        std::fs::write(metadata_path, metadata_json)?;
        
        // Set as current key if none exists
        if self.current_key_id.is_none() {
            self.current_key_id = Some(key_id.clone());
        }
        
        self.signing_keys.insert(key_id.clone(), key);
        Ok(key_id)
    }

    /// Load all keys from directory
    pub fn load_keys(&mut self) -> Result<Vec<String>, SignatureError> {
        let mut loaded_keys = Vec::new();
        
        for entry in std::fs::read_dir(&self.key_directory)? {
            let entry = entry?;
            let path = entry.path();
            
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.ends_with("_sign.pem") {
                    if let Some(key_id) = file_name.strip_suffix("_sign.pem") {
                        match SecureSigningKey::from_pem_file(&path) {
                            Ok(mut key) => {
                                // Try to load metadata if it exists
                                let metadata_path = self.key_directory.join(format!("{}_metadata.json", key_id));
                                if metadata_path.exists() {
                                    if let Ok(metadata_json) = std::fs::read_to_string(&metadata_path) {
                                        if let Ok(metadata) = serde_json::from_str::<SignatureMetadata>(&metadata_json) {
                                            key.metadata = metadata;
                                        }
                                    }
                                }
                                
                                loaded_keys.push(key.metadata.key_id.clone());
                                self.signing_keys.insert(key.metadata.key_id.clone(), key);
                            }
                            Err(e) => {
                                eprintln!("Failed to load signing key {}: {}", key_id, e);
                            }
                        }
                    }
                } else if file_name.ends_with("_verify.pem") {
                    if let Some(key_id) = file_name.strip_suffix("_verify.pem") {
                        match PublicSignatureKey::from_pem_file(&path) {
                            Ok(mut key) => {
                                // Try to load metadata if it exists
                                let metadata_path = self.key_directory.join(format!("{}_metadata.json", key_id));
                                if metadata_path.exists() {
                                    if let Ok(metadata_json) = std::fs::read_to_string(&metadata_path) {
                                        if let Ok(metadata) = serde_json::from_str::<SignatureMetadata>(&metadata_json) {
                                            key.metadata = metadata;
                                        }
                                    }
                                }
                                
                                self.public_keys.insert(key.metadata.key_id.clone(), key);
                            }
                            Err(e) => {
                                eprintln!("Failed to load public key {}: {}", key_id, e);
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

    /// Sign data with current key
    pub fn sign(&self, data: &[u8]) -> Result<SignedData, SignatureError> {
        let key = self.current_key()
            .ok_or_else(|| SignatureError::Verification("No current key set".to_string()))?;
        key.sign(data)
    }

    /// Sign data with specific key
    pub fn sign_with_key(&self, key_id: &str, data: &[u8]) -> Result<SignedData, SignatureError> {
        let key = self.signing_keys.get(key_id)
            .ok_or_else(|| SignatureError::Verification(format!("Key not found: {}", key_id)))?;
        key.sign(data)
    }

    /// Verify signed data
    pub fn verify(&self, signed_data: &SignedData) -> Result<bool, SignatureError> {
        // Try to find the public key by fingerprint
        if let Some(public_key) = self.public_keys.values()
            .find(|key| key.metadata.fingerprint == signed_data.metadata.fingerprint) {
            return public_key.verify(signed_data);
        }
        
        // Fallback: try signing keys
        if let Some(signing_key) = self.signing_keys.values()
            .find(|key| key.metadata.fingerprint == signed_data.metadata.fingerprint) {
            // Create temporary public key wrapper for verification
            let temp_public_key = PublicSignatureKey {
                metadata: signing_key.metadata.clone(),
                verifying_key: signing_key.verifying_key,
            };
            return temp_public_key.verify(signed_data);
        }
        
        Err(SignatureError::Verification(
            "No matching key found for verification".to_string()
        ))
    }

    /// Get current signing key
    pub fn current_key(&self) -> Option<&SecureSigningKey> {
        self.current_key_id.as_ref().and_then(|id| self.signing_keys.get(id))
    }

    /// Set current signing key
    pub fn set_current_key(&mut self, key_id: &str) -> Result<(), SignatureError> {
        if self.signing_keys.contains_key(key_id) {
            self.current_key_id = Some(key_id.to_string());
            Ok(())
        } else {
            Err(SignatureError::Verification(format!("Key not found: {}", key_id)))
        }
    }

    /// List all signing keys
    pub fn list_signing_keys(&self) -> Vec<&SignatureMetadata> {
        self.signing_keys.values().map(|key| &key.metadata).collect()
    }

    /// List all public keys
    pub fn list_public_keys(&self) -> Vec<&SignatureMetadata> {
        self.public_keys.values().map(|key| &key.metadata).collect()
    }

    /// Add trusted public key
    pub fn add_public_key(&mut self, public_key: PublicSignatureKey) -> String {
        let key_id = public_key.metadata.key_id.clone();
        self.public_keys.insert(key_id.clone(), public_key);
        key_id
    }

    /// Create integrity chain for multiple files
    pub fn create_integrity_chain(&self, files: &[(&str, &[u8])]) -> Result<Vec<SignedData>, SignatureError> {
        let mut chain = Vec::new();
        let mut previous_hash = String::new();
        
        for (filename, data) in files {
            // Create combined data with filename, content, and previous hash
            let combined_data = format!("{}|{}|{}", filename, hex::encode(data), previous_hash);
            let signed = self.sign(combined_data.as_bytes())?;
            
            previous_hash = signed.content_hash.clone();
            chain.push(signed);
        }
        
        Ok(chain)
    }

    /// Verify integrity chain
    pub fn verify_integrity_chain(&self, chain: &[SignedData]) -> Result<bool, SignatureError> {
        for signed_data in chain {
            if !self.verify(signed_data)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_key_generation() {
        let key = SecureSigningKey::generate("test".to_string(), Some(30)).unwrap();
        assert_eq!(key.metadata.algorithm, "Ed25519");
        assert!(!key.metadata.fingerprint.is_empty());
    }

    #[test]
    fn test_signing_verification() {
        let key = SecureSigningKey::generate("test".to_string(), None).unwrap();
        let data = b"Hello, signatures!";
        
        let signed = key.sign(data).unwrap();
        
        // Create public key for verification
        let public_key = PublicSignatureKey {
            metadata: key.metadata.clone(),
            verifying_key: key.verifying_key,
        };
        
        let is_valid = public_key.verify(&signed).unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_signature_manager() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SignatureManager::new(temp_dir.path()).unwrap();
        
        let key_id = manager.generate_key("test".to_string(), None).unwrap();
        assert!(manager.current_key().is_some());
        
        let data = b"Test message for signing";
        let signed = manager.sign(data).unwrap();
        
        let is_valid = manager.verify(&signed).unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_integrity_chain() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = SignatureManager::new(temp_dir.path()).unwrap();
        manager.generate_key("integrity".to_string(), None).unwrap();
        
        let files = vec![
            ("file1.txt", b"Content 1".as_slice()),
            ("file2.txt", b"Content 2".as_slice()),
            ("file3.txt", b"Content 3".as_slice()),
        ];
        
        let chain = manager.create_integrity_chain(&files).unwrap();
        assert_eq!(chain.len(), 3);
        
        let is_valid = manager.verify_integrity_chain(&chain).unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_key_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let key_path = temp_dir.path().join("test_sign.pem");
        
        let original_key = SecureSigningKey::generate("test".to_string(), None).unwrap();
        original_key.save_to_file(&key_path).unwrap();
        
        let loaded_key = SecureSigningKey::from_pem_file(&key_path).unwrap();
        
        // Test that both keys can verify each other's signatures
        let data = b"Test data";
        let sig1 = original_key.sign(data).unwrap();
        let sig2 = loaded_key.sign(data).unwrap();
        
        let pub_key = PublicSignatureKey {
            metadata: loaded_key.metadata.clone(),
            verifying_key: loaded_key.verifying_key,
        };
        
        // Both signatures should be valid
        assert!(pub_key.verify(&sig1).unwrap());
        assert!(pub_key.verify(&sig2).unwrap());
    }
}
