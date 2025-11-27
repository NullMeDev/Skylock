//! HSM Provider Interface Module
//!
//! Defines trait interfaces for Hardware Security Module (HSM) integration.
//! Allows Skylock to:
//! - Store encryption keys in HSMs for enhanced security
//! - Perform cryptographic operations without exposing keys
//! - Support various HSM backends (PKCS#11, cloud HSMs, software HSMs)
//!
//! Security benefits:
//! - Keys never leave the HSM boundary
//! - Tamper-resistant hardware protection
//! - Audit logging of key usage
//! - Compliance with regulatory requirements

use std::fmt;
use std::sync::Arc;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

use crate::error::{Result, SkylockError};

/// HSM key identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct HsmKeyId {
    /// Provider-specific key identifier
    pub id: String,
    /// Key label/alias
    pub label: String,
    /// Provider type
    pub provider: HsmProviderType,
}

impl HsmKeyId {
    pub fn new(id: impl Into<String>, label: impl Into<String>, provider: HsmProviderType) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            provider,
        }
    }
}

impl fmt::Display for HsmKeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.provider, self.label)
    }
}

/// Supported HSM provider types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum HsmProviderType {
    /// PKCS#11 compliant hardware HSM
    Pkcs11,
    /// AWS CloudHSM
    AwsCloudHsm,
    /// Azure Key Vault HSM
    AzureKeyVaultHsm,
    /// Google Cloud HSM
    GcpCloudHsm,
    /// HashiCorp Vault
    HashiCorpVault,
    /// YubiKey / YubiHSM
    YubiKey,
    /// Software HSM for testing (SoftHSM2)
    SoftHsm,
    /// In-memory mock HSM for testing
    Mock,
}

impl fmt::Display for HsmProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pkcs11 => write!(f, "pkcs11"),
            Self::AwsCloudHsm => write!(f, "aws-cloudhsm"),
            Self::AzureKeyVaultHsm => write!(f, "azure-keyvault-hsm"),
            Self::GcpCloudHsm => write!(f, "gcp-cloud-hsm"),
            Self::HashiCorpVault => write!(f, "hashicorp-vault"),
            Self::YubiKey => write!(f, "yubikey"),
            Self::SoftHsm => write!(f, "softhsm"),
            Self::Mock => write!(f, "mock"),
        }
    }
}

/// Key algorithm supported by HSM
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HsmKeyAlgorithm {
    /// AES-256 symmetric key
    Aes256,
    /// AES-128 symmetric key
    Aes128,
    /// RSA 2048-bit
    Rsa2048,
    /// RSA 4096-bit
    Rsa4096,
    /// ECDSA P-256
    EcdsaP256,
    /// ECDSA P-384
    EcdsaP384,
    /// Ed25519
    Ed25519,
    /// X25519 (for key exchange)
    X25519,
}

/// Key usage permissions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct HsmKeyUsage {
    /// Can be used for encryption
    pub encrypt: bool,
    /// Can be used for decryption
    pub decrypt: bool,
    /// Can be used for signing
    pub sign: bool,
    /// Can be used for verification
    pub verify: bool,
    /// Can be used for key wrapping
    pub wrap: bool,
    /// Can be used for key unwrapping
    pub unwrap: bool,
    /// Can be used for key derivation
    pub derive: bool,
}

impl Default for HsmKeyUsage {
    fn default() -> Self {
        Self {
            encrypt: true,
            decrypt: true,
            sign: false,
            verify: false,
            wrap: true,
            unwrap: true,
            derive: true,
        }
    }
}

impl HsmKeyUsage {
    /// Create usage for encryption/decryption only
    pub fn encryption_only() -> Self {
        Self {
            encrypt: true,
            decrypt: true,
            sign: false,
            verify: false,
            wrap: false,
            unwrap: false,
            derive: false,
        }
    }
    
    /// Create usage for key wrapping
    pub fn wrapping_only() -> Self {
        Self {
            encrypt: false,
            decrypt: false,
            sign: false,
            verify: false,
            wrap: true,
            unwrap: true,
            derive: false,
        }
    }
    
    /// Create usage for signing
    pub fn signing_only() -> Self {
        Self {
            encrypt: false,
            decrypt: false,
            sign: true,
            verify: true,
            wrap: false,
            unwrap: false,
            derive: false,
        }
    }
}

/// Key metadata from HSM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HsmKeyInfo {
    /// Key identifier
    pub id: HsmKeyId,
    /// Algorithm
    pub algorithm: HsmKeyAlgorithm,
    /// Key size in bits
    pub key_size_bits: u32,
    /// Usage permissions
    pub usage: HsmKeyUsage,
    /// When the key was created
    pub created_at: Option<DateTime<Utc>>,
    /// Key expiration (if any)
    pub expires_at: Option<DateTime<Utc>>,
    /// Whether the key is exportable
    pub exportable: bool,
    /// Whether the key is enabled
    pub enabled: bool,
    /// Provider-specific attributes
    pub attributes: std::collections::HashMap<String, String>,
}

/// HSM session handle
#[derive(Debug, Clone)]
pub struct HsmSession {
    /// Session identifier
    pub session_id: String,
    /// When session was opened
    pub opened_at: DateTime<Utc>,
    /// Whether session is authenticated
    pub authenticated: bool,
    /// Session timeout (if any)
    pub timeout_at: Option<DateTime<Utc>>,
}

/// HSM provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HsmConfig {
    /// Provider type
    pub provider: HsmProviderType,
    /// Connection string/path (e.g., PKCS#11 library path)
    pub connection: String,
    /// Slot ID (for PKCS#11)
    pub slot_id: Option<u64>,
    /// Token label (for PKCS#11)
    pub token_label: Option<String>,
    /// PIN/password for authentication
    #[serde(skip_serializing)]
    pub pin: Option<String>,
    /// Whether to use User PIN vs SO PIN
    pub use_user_pin: bool,
    /// Additional provider-specific config
    pub extra: std::collections::HashMap<String, String>,
}

impl Default for HsmConfig {
    fn default() -> Self {
        Self {
            provider: HsmProviderType::Mock,
            connection: String::new(),
            slot_id: None,
            token_label: None,
            pin: None,
            use_user_pin: true,
            extra: std::collections::HashMap::new(),
        }
    }
}

/// HSM Provider Trait
/// 
/// Implement this trait to support a new HSM backend.
/// All cryptographic operations should be performed within the HSM;
/// secret keys should never be exposed.
#[async_trait]
pub trait HsmProvider: Send + Sync {
    /// Get the provider type
    fn provider_type(&self) -> HsmProviderType;
    
    /// Check if provider is connected and ready
    async fn is_available(&self) -> bool;
    
    /// Open a session with the HSM
    async fn open_session(&self) -> Result<HsmSession>;
    
    /// Close a session
    async fn close_session(&self, session: &HsmSession) -> Result<()>;
    
    /// Authenticate to the HSM with PIN
    async fn login(&self, session: &HsmSession, pin: &str) -> Result<()>;
    
    /// Log out from the HSM
    async fn logout(&self, session: &HsmSession) -> Result<()>;
    
    /// Generate a new key in the HSM
    async fn generate_key(
        &self,
        session: &HsmSession,
        label: &str,
        algorithm: HsmKeyAlgorithm,
        usage: HsmKeyUsage,
        exportable: bool,
    ) -> Result<HsmKeyId>;
    
    /// Import an existing key into the HSM (if allowed)
    async fn import_key(
        &self,
        session: &HsmSession,
        label: &str,
        key_bytes: &[u8],
        algorithm: HsmKeyAlgorithm,
        usage: HsmKeyUsage,
    ) -> Result<HsmKeyId>;
    
    /// Delete a key from the HSM
    async fn delete_key(&self, session: &HsmSession, key_id: &HsmKeyId) -> Result<()>;
    
    /// Get key information
    async fn get_key_info(&self, session: &HsmSession, key_id: &HsmKeyId) -> Result<HsmKeyInfo>;
    
    /// List all keys (optionally filtered by label pattern)
    async fn list_keys(
        &self,
        session: &HsmSession,
        label_pattern: Option<&str>,
    ) -> Result<Vec<HsmKeyInfo>>;
    
    /// Encrypt data using HSM key
    async fn encrypt(
        &self,
        session: &HsmSession,
        key_id: &HsmKeyId,
        plaintext: &[u8],
        nonce: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>>;
    
    /// Decrypt data using HSM key
    async fn decrypt(
        &self,
        session: &HsmSession,
        key_id: &HsmKeyId,
        ciphertext: &[u8],
        nonce: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>>;
    
    /// Wrap (encrypt) another key using the HSM key
    async fn wrap_key(
        &self,
        session: &HsmSession,
        wrapping_key_id: &HsmKeyId,
        key_to_wrap: &[u8],
    ) -> Result<Vec<u8>>;
    
    /// Unwrap (decrypt) a wrapped key using the HSM key
    async fn unwrap_key(
        &self,
        session: &HsmSession,
        wrapping_key_id: &HsmKeyId,
        wrapped_key: &[u8],
    ) -> Result<Vec<u8>>;
    
    /// Derive a new key from the HSM key
    async fn derive_key(
        &self,
        session: &HsmSession,
        base_key_id: &HsmKeyId,
        derivation_data: &[u8],
        derived_key_length: usize,
    ) -> Result<Vec<u8>>;
    
    /// Sign data using HSM key
    async fn sign(
        &self,
        session: &HsmSession,
        key_id: &HsmKeyId,
        data: &[u8],
    ) -> Result<Vec<u8>>;
    
    /// Verify signature using HSM key
    async fn verify(
        &self,
        session: &HsmSession,
        key_id: &HsmKeyId,
        data: &[u8],
        signature: &[u8],
    ) -> Result<bool>;
    
    /// Generate random bytes from HSM's RNG
    async fn generate_random(&self, session: &HsmSession, length: usize) -> Result<Vec<u8>>;
}

/// Mock HSM provider for testing
pub struct MockHsmProvider {
    available: std::sync::atomic::AtomicBool,
    keys: parking_lot::RwLock<std::collections::HashMap<String, Vec<u8>>>,
}

impl Default for MockHsmProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockHsmProvider {
    pub fn new() -> Self {
        Self {
            available: std::sync::atomic::AtomicBool::new(true),
            keys: parking_lot::RwLock::new(std::collections::HashMap::new()),
        }
    }
    
    pub fn set_available(&self, available: bool) {
        self.available.store(available, std::sync::atomic::Ordering::SeqCst);
    }
}

#[async_trait]
impl HsmProvider for MockHsmProvider {
    fn provider_type(&self) -> HsmProviderType {
        HsmProviderType::Mock
    }
    
    async fn is_available(&self) -> bool {
        self.available.load(std::sync::atomic::Ordering::SeqCst)
    }
    
    async fn open_session(&self) -> Result<HsmSession> {
        Ok(HsmSession {
            session_id: uuid::Uuid::new_v4().to_string(),
            opened_at: Utc::now(),
            authenticated: false,
            timeout_at: None,
        })
    }
    
    async fn close_session(&self, _session: &HsmSession) -> Result<()> {
        Ok(())
    }
    
    async fn login(&self, _session: &HsmSession, _pin: &str) -> Result<()> {
        Ok(())
    }
    
    async fn logout(&self, _session: &HsmSession) -> Result<()> {
        Ok(())
    }
    
    async fn generate_key(
        &self,
        _session: &HsmSession,
        label: &str,
        algorithm: HsmKeyAlgorithm,
        _usage: HsmKeyUsage,
        _exportable: bool,
    ) -> Result<HsmKeyId> {
        use rand::RngCore;
        
        let key_size = match algorithm {
            HsmKeyAlgorithm::Aes256 => 32,
            HsmKeyAlgorithm::Aes128 => 16,
            _ => 32,
        };
        
        let mut key_bytes = vec![0u8; key_size];
        rand::rngs::OsRng.fill_bytes(&mut key_bytes);
        
        let key_id = uuid::Uuid::new_v4().to_string();
        self.keys.write().insert(key_id.clone(), key_bytes);
        
        Ok(HsmKeyId::new(&key_id, label, HsmProviderType::Mock))
    }
    
    async fn import_key(
        &self,
        _session: &HsmSession,
        label: &str,
        key_bytes: &[u8],
        _algorithm: HsmKeyAlgorithm,
        _usage: HsmKeyUsage,
    ) -> Result<HsmKeyId> {
        let key_id = uuid::Uuid::new_v4().to_string();
        self.keys.write().insert(key_id.clone(), key_bytes.to_vec());
        
        Ok(HsmKeyId::new(&key_id, label, HsmProviderType::Mock))
    }
    
    async fn delete_key(&self, _session: &HsmSession, key_id: &HsmKeyId) -> Result<()> {
        self.keys.write().remove(&key_id.id);
        Ok(())
    }
    
    async fn get_key_info(&self, _session: &HsmSession, key_id: &HsmKeyId) -> Result<HsmKeyInfo> {
        let keys = self.keys.read();
        if let Some(key_bytes) = keys.get(&key_id.id) {
            Ok(HsmKeyInfo {
                id: key_id.clone(),
                algorithm: HsmKeyAlgorithm::Aes256,
                key_size_bits: (key_bytes.len() * 8) as u32,
                usage: HsmKeyUsage::default(),
                created_at: Some(Utc::now()),
                expires_at: None,
                exportable: true,
                enabled: true,
                attributes: std::collections::HashMap::new(),
            })
        } else {
            Err(SkylockError::Encryption(format!("Key not found: {}", key_id)))
        }
    }
    
    async fn list_keys(
        &self,
        _session: &HsmSession,
        _label_pattern: Option<&str>,
    ) -> Result<Vec<HsmKeyInfo>> {
        // Simplified - just return empty for mock
        Ok(Vec::new())
    }
    
    async fn encrypt(
        &self,
        _session: &HsmSession,
        key_id: &HsmKeyId,
        plaintext: &[u8],
        nonce: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>> {
        use aes_gcm::{Aes256Gcm, KeyInit, aead::{Aead, Payload}};
        use aes_gcm::aead::generic_array::GenericArray;
        
        let keys = self.keys.read();
        let key_bytes = keys.get(&key_id.id)
            .ok_or_else(|| SkylockError::Encryption(format!("Key not found: {}", key_id)))?;
        
        let cipher = Aes256Gcm::new(GenericArray::from_slice(key_bytes));
        let nonce = GenericArray::from_slice(nonce);
        
        let payload = Payload {
            msg: plaintext,
            aad: aad.unwrap_or(&[]),
        };
        
        cipher.encrypt(nonce, payload)
            .map_err(|e| SkylockError::Encryption(format!("Encryption failed: {}", e)))
    }
    
    async fn decrypt(
        &self,
        _session: &HsmSession,
        key_id: &HsmKeyId,
        ciphertext: &[u8],
        nonce: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>> {
        use aes_gcm::{Aes256Gcm, KeyInit, aead::{Aead, Payload}};
        use aes_gcm::aead::generic_array::GenericArray;
        
        let keys = self.keys.read();
        let key_bytes = keys.get(&key_id.id)
            .ok_or_else(|| SkylockError::Encryption(format!("Key not found: {}", key_id)))?;
        
        let cipher = Aes256Gcm::new(GenericArray::from_slice(key_bytes));
        let nonce = GenericArray::from_slice(nonce);
        
        let payload = Payload {
            msg: ciphertext,
            aad: aad.unwrap_or(&[]),
        };
        
        cipher.decrypt(nonce, payload)
            .map_err(|e| SkylockError::Encryption(format!("Decryption failed: {}", e)))
    }
    
    async fn wrap_key(
        &self,
        session: &HsmSession,
        wrapping_key_id: &HsmKeyId,
        key_to_wrap: &[u8],
    ) -> Result<Vec<u8>> {
        // Use encrypt for wrapping in mock
        use rand::RngCore;
        let mut nonce = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        
        let mut result = nonce.to_vec();
        let encrypted = self.encrypt(session, wrapping_key_id, key_to_wrap, &nonce, None).await?;
        result.extend(encrypted);
        
        Ok(result)
    }
    
    async fn unwrap_key(
        &self,
        session: &HsmSession,
        wrapping_key_id: &HsmKeyId,
        wrapped_key: &[u8],
    ) -> Result<Vec<u8>> {
        if wrapped_key.len() < 12 {
            return Err(SkylockError::Encryption("Invalid wrapped key".to_string()));
        }
        
        let nonce = &wrapped_key[..12];
        let ciphertext = &wrapped_key[12..];
        
        self.decrypt(session, wrapping_key_id, ciphertext, nonce, None).await
    }
    
    async fn derive_key(
        &self,
        _session: &HsmSession,
        key_id: &HsmKeyId,
        derivation_data: &[u8],
        derived_key_length: usize,
    ) -> Result<Vec<u8>> {
        use hkdf::Hkdf;
        use sha2::Sha256;
        
        let keys = self.keys.read();
        let key_bytes = keys.get(&key_id.id)
            .ok_or_else(|| SkylockError::Encryption(format!("Key not found: {}", key_id)))?;
        
        let hkdf = Hkdf::<Sha256>::new(Some(derivation_data), key_bytes);
        let mut derived = vec![0u8; derived_key_length];
        hkdf.expand(b"skylock-derived-key", &mut derived)
            .map_err(|_| SkylockError::Encryption("Key derivation failed".to_string()))?;
        
        Ok(derived)
    }
    
    async fn sign(
        &self,
        _session: &HsmSession,
        key_id: &HsmKeyId,
        data: &[u8],
    ) -> Result<Vec<u8>> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        
        let keys = self.keys.read();
        let key_bytes = keys.get(&key_id.id)
            .ok_or_else(|| SkylockError::Encryption(format!("Key not found: {}", key_id)))?;
        
        let mut mac = Hmac::<Sha256>::new_from_slice(key_bytes)
            .map_err(|_| SkylockError::Encryption("Invalid key length".to_string()))?;
        mac.update(data);
        
        Ok(mac.finalize().into_bytes().to_vec())
    }
    
    async fn verify(
        &self,
        _session: &HsmSession,
        key_id: &HsmKeyId,
        data: &[u8],
        signature: &[u8],
    ) -> Result<bool> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        
        let keys = self.keys.read();
        let key_bytes = keys.get(&key_id.id)
            .ok_or_else(|| SkylockError::Encryption(format!("Key not found: {}", key_id)))?;
        
        let mut mac = Hmac::<Sha256>::new_from_slice(key_bytes)
            .map_err(|_| SkylockError::Encryption("Invalid key length".to_string()))?;
        mac.update(data);
        
        Ok(mac.verify_slice(signature).is_ok())
    }
    
    async fn generate_random(&self, _session: &HsmSession, length: usize) -> Result<Vec<u8>> {
        use rand::RngCore;
        let mut bytes = vec![0u8; length];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        Ok(bytes)
    }
}

/// HSM-backed key manager that integrates with Skylock's encryption
pub struct HsmKeyManager {
    provider: Arc<dyn HsmProvider>,
    session: parking_lot::RwLock<Option<HsmSession>>,
    master_key_id: parking_lot::RwLock<Option<HsmKeyId>>,
}

impl HsmKeyManager {
    /// Create a new HSM key manager
    pub fn new(provider: Arc<dyn HsmProvider>) -> Self {
        Self {
            provider,
            session: parking_lot::RwLock::new(None),
            master_key_id: parking_lot::RwLock::new(None),
        }
    }
    
    /// Initialize HSM connection
    pub async fn initialize(&self, pin: &str) -> Result<()> {
        let session = self.provider.open_session().await?;
        self.provider.login(&session, pin).await?;
        *self.session.write() = Some(session);
        Ok(())
    }
    
    /// Close HSM connection
    pub async fn close(&self) -> Result<()> {
        if let Some(session) = self.session.write().take() {
            self.provider.logout(&session).await?;
            self.provider.close_session(&session).await?;
        }
        Ok(())
    }
    
    /// Get or create master encryption key
    pub async fn get_or_create_master_key(&self, label: &str) -> Result<HsmKeyId> {
        let session = self.get_session()?;
        
        // Check if we already have the key ID cached
        if let Some(key_id) = self.master_key_id.read().clone() {
            return Ok(key_id);
        }
        
        // Try to find existing key
        let keys = self.provider.list_keys(&session, Some(label)).await?;
        if let Some(key_info) = keys.first() {
            *self.master_key_id.write() = Some(key_info.id.clone());
            return Ok(key_info.id.clone());
        }
        
        // Create new key
        let key_id = self.provider.generate_key(
            &session,
            label,
            HsmKeyAlgorithm::Aes256,
            HsmKeyUsage::default(),
            false, // Not exportable for security
        ).await?;
        
        *self.master_key_id.write() = Some(key_id.clone());
        Ok(key_id)
    }
    
    /// Encrypt data using HSM
    pub async fn encrypt(&self, plaintext: &[u8], nonce: &[u8]) -> Result<Vec<u8>> {
        let session = self.get_session()?;
        let key_id = self.master_key_id.read().clone()
            .ok_or_else(|| SkylockError::Encryption("No master key configured".to_string()))?;
        
        self.provider.encrypt(&session, &key_id, plaintext, nonce, None).await
    }
    
    /// Decrypt data using HSM
    pub async fn decrypt(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>> {
        let session = self.get_session()?;
        let key_id = self.master_key_id.read().clone()
            .ok_or_else(|| SkylockError::Encryption("No master key configured".to_string()))?;
        
        self.provider.decrypt(&session, &key_id, ciphertext, nonce, None).await
    }
    
    /// Wrap a data encryption key (DEK) with the HSM master key
    pub async fn wrap_dek(&self, dek: &[u8]) -> Result<Vec<u8>> {
        let session = self.get_session()?;
        let key_id = self.master_key_id.read().clone()
            .ok_or_else(|| SkylockError::Encryption("No master key configured".to_string()))?;
        
        self.provider.wrap_key(&session, &key_id, dek).await
    }
    
    /// Unwrap a data encryption key (DEK) using the HSM master key
    pub async fn unwrap_dek(&self, wrapped_dek: &[u8]) -> Result<Vec<u8>> {
        let session = self.get_session()?;
        let key_id = self.master_key_id.read().clone()
            .ok_or_else(|| SkylockError::Encryption("No master key configured".to_string()))?;
        
        self.provider.unwrap_key(&session, &key_id, wrapped_dek).await
    }
    
    fn get_session(&self) -> Result<HsmSession> {
        self.session.read().clone()
            .ok_or_else(|| SkylockError::Encryption("HSM session not initialized".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_mock_hsm_key_generation() {
        let provider = MockHsmProvider::new();
        let session = provider.open_session().await.unwrap();
        
        let key_id = provider.generate_key(
            &session,
            "test-key",
            HsmKeyAlgorithm::Aes256,
            HsmKeyUsage::default(),
            true,
        ).await.unwrap();
        
        assert_eq!(key_id.label, "test-key");
        assert_eq!(key_id.provider, HsmProviderType::Mock);
    }
    
    #[tokio::test]
    async fn test_mock_hsm_encrypt_decrypt() {
        let provider = MockHsmProvider::new();
        let session = provider.open_session().await.unwrap();
        
        // Import a known key
        let key_bytes = [0x42u8; 32];
        let key_id = provider.import_key(
            &session,
            "enc-key",
            &key_bytes,
            HsmKeyAlgorithm::Aes256,
            HsmKeyUsage::encryption_only(),
        ).await.unwrap();
        
        // Encrypt
        let plaintext = b"Hello, HSM!";
        let nonce = [0x00u8; 12];
        let ciphertext = provider.encrypt(&session, &key_id, plaintext, &nonce, None).await.unwrap();
        
        // Decrypt
        let decrypted = provider.decrypt(&session, &key_id, &ciphertext, &nonce, None).await.unwrap();
        assert_eq!(decrypted, plaintext);
    }
    
    #[tokio::test]
    async fn test_mock_hsm_wrap_unwrap() {
        let provider = MockHsmProvider::new();
        let session = provider.open_session().await.unwrap();
        
        // Create wrapping key
        let wrap_key_id = provider.generate_key(
            &session,
            "wrap-key",
            HsmKeyAlgorithm::Aes256,
            HsmKeyUsage::wrapping_only(),
            false,
        ).await.unwrap();
        
        // Wrap a DEK
        let dek = [0x33u8; 32];
        let wrapped = provider.wrap_key(&session, &wrap_key_id, &dek).await.unwrap();
        
        // Unwrap
        let unwrapped = provider.unwrap_key(&session, &wrap_key_id, &wrapped).await.unwrap();
        assert_eq!(unwrapped, dek);
    }
    
    #[tokio::test]
    async fn test_mock_hsm_sign_verify() {
        let provider = MockHsmProvider::new();
        let session = provider.open_session().await.unwrap();
        
        let key_id = provider.generate_key(
            &session,
            "sign-key",
            HsmKeyAlgorithm::Aes256,
            HsmKeyUsage::signing_only(),
            false,
        ).await.unwrap();
        
        let data = b"Data to sign";
        let signature = provider.sign(&session, &key_id, data).await.unwrap();
        
        // Verify valid signature
        let valid = provider.verify(&session, &key_id, data, &signature).await.unwrap();
        assert!(valid);
        
        // Verify invalid signature
        let invalid = provider.verify(&session, &key_id, b"Wrong data", &signature).await.unwrap();
        assert!(!invalid);
    }
    
    #[tokio::test]
    async fn test_hsm_key_manager() {
        let provider = Arc::new(MockHsmProvider::new());
        let manager = HsmKeyManager::new(provider);
        
        // Initialize
        manager.initialize("1234").await.unwrap();
        
        // Get or create master key
        let key_id = manager.get_or_create_master_key("skylock-master").await.unwrap();
        assert_eq!(key_id.label, "skylock-master");
        
        // Close
        manager.close().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_key_usage_variants() {
        let default = HsmKeyUsage::default();
        assert!(default.encrypt);
        assert!(default.decrypt);
        
        let enc_only = HsmKeyUsage::encryption_only();
        assert!(enc_only.encrypt);
        assert!(!enc_only.wrap);
        
        let wrap_only = HsmKeyUsage::wrapping_only();
        assert!(!wrap_only.encrypt);
        assert!(wrap_only.wrap);
        
        let sign_only = HsmKeyUsage::signing_only();
        assert!(!sign_only.encrypt);
        assert!(sign_only.sign);
    }
    
    #[test]
    fn test_hsm_key_id_display() {
        let key_id = HsmKeyId::new("abc123", "my-key", HsmProviderType::Pkcs11);
        assert_eq!(format!("{}", key_id), "pkcs11:my-key");
    }
    
    #[test]
    fn test_provider_type_display() {
        assert_eq!(format!("{}", HsmProviderType::Pkcs11), "pkcs11");
        assert_eq!(format!("{}", HsmProviderType::AwsCloudHsm), "aws-cloudhsm");
        assert_eq!(format!("{}", HsmProviderType::Mock), "mock");
    }
}
