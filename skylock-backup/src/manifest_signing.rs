//! Manifest signing and verification with Ed25519
//!
//! Provides cryptographic signatures for backup manifests to ensure:
//! - **Integrity**: Detect unauthorized modifications to manifests
//! - **Authenticity**: Verify manifests were created by legitimate key holder
//! - **Anti-rollback**: Prevent restoration of older, potentially compromised manifests

use crate::direct_upload::{BackupManifest, ManifestSignature};
use crate::error::{Result, SkylockError};
use chrono::Utc;
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::path::{Path, PathBuf};

// We need to reference signatures module from the workspace root
// Since skylock-backup can't depend on the main binary, we'll need to add ed25519-dalek
// to skylock-backup's dependencies and redefine the types locally, or
// move the signatures module to skylock-core.
// For now, let's use ed25519-dalek directly here.

use ed25519_dalek::{
    Signature, Signer, SigningKey, Verifier, VerifyingKey,
};
use pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use rand::rngs::OsRng;
use uuid::Uuid;  
use zeroize::ZeroizeOnDrop;

/// Digital signature errors
#[derive(thiserror::Error, Debug)]
pub enum SignatureError {
    #[error("Key generation failed: {0}")]
    KeyGeneration(String),
    #[error("Key loading failed: {0}")]
    KeyLoading(String),
    #[error("Signing failed: {0}")]
    Signing(String),
    #[error("Verification failed: {0}")]
    Verification(String),
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Digital signature metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SignatureMetadata {
    pub key_id: String,
    pub algorithm: String,
    pub created_at: chrono::DateTime<Utc>,
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub purpose: String,
    pub public_key_hex: String,
    pub fingerprint: String,
}

/// Ed25519 signing key with secure storage
#[derive(ZeroizeOnDrop)]
pub struct SecureSigningKey {
    #[zeroize(skip)]
    pub metadata: SignatureMetadata,
    pub signing_key: SigningKey,
    #[zeroize(skip)]
    pub verifying_key: VerifyingKey,
}

impl SecureSigningKey {
    /// Generate a new Ed25519 key pair
    pub fn generate(purpose: String, expires_in_days: Option<u32>) -> std::result::Result<Self, SignatureError> {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        
        let key_id = Uuid::new_v4().to_string();
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

    /// Sign only the hash of data
    pub fn sign_hash(&self, hash: &[u8]) -> std::result::Result<Vec<u8>, SignatureError> {
        let signature = self.signing_key
            .try_sign(hash)
            .map_err(|e| SignatureError::Signing(e.to_string()))?;
        
        Ok(signature.to_bytes().to_vec())
    }

    /// Get public key
    pub fn public_key(&self) -> VerifyingKey {
        self.verifying_key
    }

    /// Get metadata
    pub fn metadata(&self) -> &SignatureMetadata {
        &self.metadata
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
    /// Verify signature of data hash
    pub fn verify_hash(&self, hash: &[u8], signature_bytes: &[u8]) -> std::result::Result<bool, SignatureError> {
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

// Conversion from SignatureError to SkylockError
impl From<SignatureError> for SkylockError {
    fn from(err: SignatureError) -> Self {
        SkylockError::Crypto(err.to_string())
    }
}

/// Latest chain state for anti-rollback protection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainState {
    /// Latest backup chain version seen
    pub latest_version: u64,
    /// Backup ID of the latest version
    pub latest_backup_id: String,
    /// Timestamp of latest update
    pub last_updated: chrono::DateTime<Utc>,
    /// Fingerprint of the signing key
    pub key_fingerprint: String,
}

impl ChainState {
    /// Load chain state from disk
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let data = tokio::fs::read_to_string(path).await
            .map_err(|e| SkylockError::Crypto(format!("Failed to load chain state: {}", e)))?;
        
        serde_json::from_str(&data)
            .map_err(|e| SkylockError::Crypto(format!("Invalid chain state: {}", e)))
    }
    
    /// Save chain state to disk
    pub async fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| SkylockError::Crypto(format!("Failed to serialize chain state: {}", e)))?;
        
        tokio::fs::write(path, json).await
            .map_err(|e| SkylockError::Crypto(format!("Failed to save chain state: {}", e)))?;
        
        Ok(())
    }
    
    /// Verify chain state allows a new version
    pub fn verify_chain_advance(&self, new_version: u64, new_backup_id: &str) -> Result<()> {
        if new_version <= self.latest_version {
            return Err(SkylockError::Crypto(format!(
                "Anti-rollback violation: New version {} <= current version {}",
                new_version, self.latest_version
            )));
        }
        
        Ok(())
    }
}

/// Sign a backup manifest with Ed25519
pub fn sign_manifest(
    manifest: &mut BackupManifest,
    signing_key: &SecureSigningKey,
    chain_version: u64,
) -> Result<()> {
    // Temporarily remove existing signature to create canonical form
    let existing_sig = manifest.signature.take();
    manifest.backup_chain_version = chain_version;
    
    // Serialize manifest to JSON (canonical form for signing)
    let manifest_json = serde_json::to_vec(manifest)
        .map_err(|e| SkylockError::Crypto(format!("Failed to serialize manifest: {}", e)))?;
    
    // Sign the manifest bytes
    let signature_bytes = signing_key.sign_hash(&manifest_json)
        .map_err(|e| SkylockError::Crypto(format!("Signing failed: {}", e)))?;
    
    // Create signature metadata
    let signature = ManifestSignature {
        algorithm: "Ed25519".to_string(),
        fingerprint: signing_key.metadata().fingerprint.clone(),
        signature_hex: hex::encode(&signature_bytes),
        signed_at: Utc::now(),
        key_id: signing_key.metadata().key_id.clone(),
    };
    
    // Attach signature to manifest
    manifest.signature = Some(signature);
    
    Ok(())
}

/// Verify a manifest signature
pub fn verify_manifest(
    manifest: &BackupManifest,
    public_key: &PublicSignatureKey,
) -> Result<bool> {
    // Extract signature
    let sig_metadata = manifest.signature.as_ref()
        .ok_or_else(|| SkylockError::Crypto("Manifest is not signed".to_string()))?;
    
    // Verify key fingerprint matches
    if sig_metadata.fingerprint != public_key.metadata.fingerprint {
        return Err(SkylockError::Crypto(format!(
            "Key fingerprint mismatch: manifest={}, key={}",
            sig_metadata.fingerprint,
            public_key.metadata.fingerprint
        )));
    }
    
    // Decode signature
    let signature_bytes = hex::decode(&sig_metadata.signature_hex)
        .map_err(|e| SkylockError::Crypto(format!("Invalid signature hex: {}", e)))?;
    
    // Create canonical manifest (without signature)
    let mut canonical_manifest = manifest.clone();
    canonical_manifest.signature = None;
    
    let manifest_json = serde_json::to_vec(&canonical_manifest)
        .map_err(|e| SkylockError::Crypto(format!("Failed to serialize manifest: {}", e)))?;
    
    // Verify signature
    public_key.verify_hash(&manifest_json, &signature_bytes)
        .map_err(|e| SkylockError::Crypto(format!("Verification failed: {}", e)))
}

/// Verify manifest and check for rollback attacks
pub async fn verify_manifest_with_chain(
    manifest: &BackupManifest,
    public_key: &PublicSignatureKey,
    chain_state_path: &Path,
) -> Result<bool> {
    // First verify signature
    let sig_valid = verify_manifest(manifest, public_key)?;
    if !sig_valid {
        return Ok(false);
    }
    
    // Load chain state if it exists
    if chain_state_path.exists() {
        let chain_state = ChainState::load(chain_state_path).await?;
        
        // Verify key fingerprint hasn't changed (key rotation attack)
        if chain_state.key_fingerprint != public_key.metadata.fingerprint {
            return Err(SkylockError::Crypto(format!(
                "Key rotation detected: expected {}, got {}. Use 'skylock key rotate' to authorize key change.",
                chain_state.key_fingerprint,
                public_key.metadata.fingerprint
            )));
        }
        
        // Verify chain version is advancing
        chain_state.verify_chain_advance(manifest.backup_chain_version, &manifest.backup_id)?;
        
        // Update chain state
        let new_state = ChainState {
            latest_version: manifest.backup_chain_version,
            latest_backup_id: manifest.backup_id.clone(),
            last_updated: Utc::now(),
            key_fingerprint: public_key.metadata.fingerprint.clone(),
        };
        new_state.save(chain_state_path).await?;
    } else {
        // First backup - initialize chain state
        let initial_state = ChainState {
            latest_version: manifest.backup_chain_version,
            latest_backup_id: manifest.backup_id.clone(),
            last_updated: Utc::now(),
            key_fingerprint: public_key.metadata.fingerprint.clone(),
        };
        
        tokio::fs::create_dir_all(chain_state_path.parent().unwrap()).await
            .map_err(|e| SkylockError::Crypto(format!("Failed to create chain state dir: {}", e)))?;
        
        initial_state.save(chain_state_path).await?;
    }
    
    Ok(true)
}

/// Get the next chain version from current state
pub async fn get_next_chain_version(chain_state_path: &Path) -> Result<u64> {
    if chain_state_path.exists() {
        let state = ChainState::load(chain_state_path).await?;
        Ok(state.latest_version + 1)
    } else {
        // First backup starts at version 1
        Ok(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::path::PathBuf;
    
    fn create_test_manifest() -> BackupManifest {
        BackupManifest {
            backup_id: "backup_20240101_000000".to_string(),
            timestamp: Utc::now(),
            files: vec![],
            total_size: 0,
            file_count: 0,
            source_paths: vec![PathBuf::from("/test")],
            base_backup_id: None,
            encryption_version: "v2".to_string(),
            kdf_params: None,
            signature: None,
            backup_chain_version: 0,
        }
    }
    
    #[test]
    fn test_manifest_signing() {
        let mut manifest = create_test_manifest();
        let signing_key = SecureSigningKey::generate("backup_integrity".to_string(), None).unwrap();
        
        // Sign manifest
        sign_manifest(&mut manifest, &signing_key, 1).unwrap();
        
        assert!(manifest.signature.is_some());
        assert_eq!(manifest.backup_chain_version, 1);
        
        // Verify signature
        let public_key = PublicSignatureKey {
            metadata: signing_key.metadata().clone(),
            verifying_key: signing_key.public_key(),
        };
        
        let is_valid = verify_manifest(&manifest, &public_key).unwrap();
        assert!(is_valid);
    }
    
    #[test]
    fn test_tampered_manifest_detected() {
        let mut manifest = create_test_manifest();
        let signing_key = SecureSigningKey::generate("backup_integrity".to_string(), None).unwrap();
        
        sign_manifest(&mut manifest, &signing_key, 1).unwrap();
        
        // Tamper with manifest
        manifest.file_count = 999;
        
        let public_key = PublicSignatureKey {
            metadata: signing_key.metadata().clone(),
            verifying_key: signing_key.public_key(),
        };
        
        // Verification should fail
        let is_valid = verify_manifest(&manifest, &public_key).unwrap();
        assert!(!is_valid);
    }
    
    #[tokio::test]
    async fn test_chain_version_anti_rollback() {
        let temp_dir = TempDir::new().unwrap();
        let chain_path = temp_dir.path().join("chain_state.json");
        
        let signing_key = SecureSigningKey::generate("backup_integrity".to_string(), None).unwrap();
        let public_key = PublicSignatureKey {
            metadata: signing_key.metadata().clone(),
            verifying_key: signing_key.public_key(),
        };
        
        // First backup - version 1
        let mut manifest1 = create_test_manifest();
        manifest1.backup_id = "backup_20240101_000000".to_string();
        sign_manifest(&mut manifest1, &signing_key, 1).unwrap();
        
        let is_valid = verify_manifest_with_chain(&manifest1, &public_key, &chain_path).await.unwrap();
        assert!(is_valid);
        
        // Second backup - version 2
        let mut manifest2 = create_test_manifest();
        manifest2.backup_id = "backup_20240102_000000".to_string();
        sign_manifest(&mut manifest2, &signing_key, 2).unwrap();
        
        let is_valid = verify_manifest_with_chain(&manifest2, &public_key, &chain_path).await.unwrap();
        assert!(is_valid);
        
        // Attempt rollback - try to restore version 1 again
        let result = verify_manifest_with_chain(&manifest1, &public_key, &chain_path).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Anti-rollback"));
    }
    
    #[tokio::test]
    async fn test_key_rotation_detection() {
        let temp_dir = TempDir::new().unwrap();
        let chain_path = temp_dir.path().join("chain_state.json");
        
        // First key
        let signing_key1 = SecureSigningKey::generate("backup_integrity".to_string(), None).unwrap();
        let public_key1 = PublicSignatureKey {
            metadata: signing_key1.metadata().clone(),
            verifying_key: signing_key1.public_key(),
        };
        
        let mut manifest1 = create_test_manifest();
        sign_manifest(&mut manifest1, &signing_key1, 1).unwrap();
        verify_manifest_with_chain(&manifest1, &public_key1, &chain_path).await.unwrap();
        
        // Second key (unauthorized rotation)
        let signing_key2 = SecureSigningKey::generate("backup_integrity".to_string(), None).unwrap();
        let public_key2 = PublicSignatureKey {
            metadata: signing_key2.metadata().clone(),
            verifying_key: signing_key2.public_key(),
        };
        
        let mut manifest2 = create_test_manifest();
        manifest2.backup_id = "backup_20240102_000000".to_string();
        sign_manifest(&mut manifest2, &signing_key2, 2).unwrap();
        
        // Should fail - different key fingerprint
        let result = verify_manifest_with_chain(&manifest2, &public_key2, &chain_path).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Key rotation detected"));
    }
}
