use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};
use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Payload, generic_array::GenericArray, Aead as ChachaAead, AeadCore as ChachaAeadCore, KeyInit as ChachaKeyInit},
};
use crate::{
    Result, SkylockError,
    error_types::{Error, ErrorCategory, ErrorSeverity, SecurityErrorType},
};
use std::path::Path;
use tokio::{fs::File, io::{AsyncReadExt, AsyncWriteExt}};
use serde::{Serialize, Deserialize};
use argon2::{
    password_hash::SaltString,
    Argon2
};

// Key Types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeyType {
    Master,   // Used for key encryption
    File,     // Used for file encryption
    Block,    // Used for block-level encryption
}

// Extended Key Types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CipherType {
    AES256GCM,
    XChaCha20Poly1305,
}

// Key Status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyStatus {
    Valid,
    Expired,
    Compromised,
    Unknown,
}

// Secure Key Structure
#[derive(Debug, Clone)]
pub struct SecureKey {
    pub key_type: KeyType,
    pub cipher_type: CipherType,
    pub key: Key<Aes256Gcm>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used: chrono::DateTime<chrono::Utc>,
    pub status: KeyStatus,
}

// Encryption Engine Trait  
pub trait EncryptionEngine: Send + Sync {
    fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>>;
    fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>>;
    fn get_key_type(&self) -> KeyType;
    fn get_key_status(&self) -> KeyStatus;
}

const KEYS_FILE: &str = "block_keys.json";
const METADATA_FILE: &str = "encryption_metadata.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockKey {
    key: [u8; 32],
    nonce: [u8; 24],
    block_hash: String,
    key_type: KeyType,
    status: KeyStatus,
    created_at: chrono::DateTime<chrono::Utc>,
    last_used: chrono::DateTime<chrono::Utc>,
}

pub struct EncryptionManager {
    master_key: Key<Aes256Gcm>,
    chacha_cipher: XChaCha20Poly1305,
    block_keys: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, BlockKey>>>,
    key_store: FileKeyStore,
}

impl std::fmt::Debug for EncryptionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptionManager")
            .field("master_key", &"[REDACTED]")
            .field("chacha_cipher", &"[CIPHER]")
            .field("block_keys", &"[KEYS_STORE]")
            .field("key_store", &"[KEY_STORE]")
            .finish()
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct KeyMetadata {
    salt: String,
    key_hash: String,
    created_at: chrono::DateTime<chrono::Utc>,
    last_rotated: Option<chrono::DateTime<chrono::Utc>>,
}

impl EncryptionManager {
    pub async fn new(config_path: &Path, password: &str) -> crate::Result<Self> {
        let metadata = Self::load_or_create_metadata(config_path).await?;
        
        let salt = SaltString::from_b64(&metadata.salt)
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::KeyNotFound),
                ErrorSeverity::High,
                format!("Invalid salt: {}", e),
                "encryption_manager".to_string(),
            ))?;
            
        let argon2 = Argon2::default();
        let mut key = [0u8; 32];
        argon2
            .hash_password_into(password.as_bytes(), salt.as_str().as_bytes(), &mut key)
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::KeyNotFound),
                ErrorSeverity::High,
                format!("Key derivation failed: {}", e),
                "encryption_manager".to_string(),
            ))?;

        let master_key = Key::<Aes256Gcm>::from_slice(&key);
        let chacha_key = GenericArray::from_slice(&key);
        let chacha_cipher = XChaCha20Poly1305::new(chacha_key);
        
        let key_store = FileKeyStore::new(master_key).await?;
        let block_keys = std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));

        Ok(Self {
            master_key: *master_key,
            chacha_cipher,
            block_keys,
            key_store,
        })
    }

    async fn load_or_create_metadata(config_path: &Path) -> crate::Result<KeyMetadata> {
        let metadata_path = config_path.join(METADATA_FILE);
        if metadata_path.exists() {
            let json = tokio::fs::read_to_string(&metadata_path)
                .await
                .map_err(|e| Error::new(
                    ErrorCategory::Security(SecurityErrorType::DecryptionFailed),
                    ErrorSeverity::High,
                    format!("Failed to read metadata: {}", e),
                    "encryption_manager".to_string(),
                ))?;

            serde_json::from_str(&json)
                .map_err(|e| Error::new(
                    ErrorCategory::Security(SecurityErrorType::DecryptionFailed),
                    ErrorSeverity::High,
                    format!("Failed to parse metadata: {}", e),
                    "encryption_manager".to_string(),
                ).into())
        } else {
            let salt = SaltString::generate(&mut OsRng);
            let metadata = KeyMetadata {
                salt: salt.to_string(),
                key_hash: String::new(),
                created_at: chrono::Utc::now(),
                last_rotated: None,
            };

            let json = serde_json::to_string_pretty(&metadata)
                .map_err(|e| Error::new(
                    ErrorCategory::Security(SecurityErrorType::EncryptionFailed),
                    ErrorSeverity::High,
                    format!("Failed to serialize metadata: {}", e),
                    "encryption_manager".to_string(),
                ))?;
                
            tokio::fs::write(&metadata_path, json)
                .await
                .map_err(|e| Error::new(
                    ErrorCategory::Security(SecurityErrorType::EncryptionFailed),
                    ErrorSeverity::High,
                    format!("Failed to write metadata: {}", e),
                    "encryption_manager".to_string(),
                ))?;
                
            Ok(metadata)
        }
    }

    async fn get_or_create_block_key(&self, block_hash: &str) -> Result<BlockKey> {
        {
            let keys = self.block_keys.read().await;
            if let Some(key) = keys.get(block_hash) {
                return Ok(key.clone());
            }
        }

        let mut new_key = [0u8; 32];
        let mut new_nonce = [0u8; 24];
        OsRng.fill_bytes(&mut new_key);
        OsRng.fill_bytes(&mut new_nonce);

        let block_key = BlockKey {
            key: new_key,
            nonce: new_nonce,
            block_hash: block_hash.to_string(),
            key_type: KeyType::Block,
            status: KeyStatus::Valid,
            created_at: chrono::Utc::now(),
            last_used: chrono::Utc::now(),
        };

        {
            let mut keys = self.block_keys.write().await;
            keys.insert(block_hash.to_string(), block_key.clone());
            self.key_store.store_block_key(&block_key).await?;
        }

        Ok(block_key)
    }

    #[tracing::instrument(skip(self, data))]
    pub async fn encrypt_block(&self, data: &[u8], block_hash: &str) -> Result<Vec<u8>> {
        let block_key = self.get_or_create_block_key(block_hash).await?;
        let aad = block_hash.as_bytes();
        let payload = Payload { msg: data, aad };

        let nonce = GenericArray::from_slice(&block_key.nonce);
        let key = GenericArray::from_slice(&block_key.key);
        let cipher = XChaCha20Poly1305::new(key);
        
        cipher.encrypt(nonce, payload)
            .map_err(|e| SkylockError::Encryption(format!("Block encryption failed: {}", e)))
    }

    #[tracing::instrument(skip(self, data))]
    pub async fn decrypt_block(&self, data: &[u8], block_hash: &str) -> Result<Vec<u8>> {
        let block_key = self.get_or_create_block_key(block_hash).await?;
        let aad = block_hash.as_bytes();
        let payload = Payload { msg: data, aad };

        let nonce = GenericArray::from_slice(&block_key.nonce);
        let key = GenericArray::from_slice(&block_key.key);
        let cipher = XChaCha20Poly1305::new(key);
        
        cipher.decrypt(nonce, payload)
            .map_err(|e| SkylockError::Encryption(format!("Block decryption failed: {}", e)))
    }

    async fn get_block_key(&self, block_hash: &str) -> Result<BlockKey> {
        let keys = self.block_keys.read().await;
        keys.get(block_hash)
            .cloned()
            .ok_or_else(|| SkylockError::Encryption(format!("Block key not found for hash: {}", block_hash)))
    }

    pub async fn encrypt_file(&self, source: &Path, dest: &Path) -> Result<()> {
        let mut source_file = File::open(source)
            .await
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::EncryptionFailed),
                ErrorSeverity::High,
                format!("Failed to open source file: {}", e),
                "encryption_manager".to_string(),
            ))?;

        let mut dest_file = File::create(dest)
            .await
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::EncryptionFailed),
                ErrorSeverity::High,
                format!("Failed to create destination file: {}", e),
                "encryption_manager".to_string(),
            ))?;

        let mut buffer = vec![0u8; 1024 * 1024]; // 1MB chunks
        
        loop {
            let n = source_file.read(&mut buffer).await
                .map_err(|e| Error::new(
                    ErrorCategory::Security(SecurityErrorType::EncryptionFailed),
                    ErrorSeverity::High,
                    format!("Failed to read source file: {}", e),
                    "encryption_manager".to_string(),
                ))?;
                
            if n == 0 {
                break;
            }
            
            let mut hasher = sha2::Sha256::new();
            hasher.update(&buffer[..n]);
            let chunk_hash = format!("{:x}", hasher.finalize());
            
            let encrypted_chunk = self.encrypt_block(&buffer[..n], &chunk_hash).await?;
            
            dest_file.write_all(&encrypted_chunk).await
                .map_err(|e| Error::new(
                    ErrorCategory::Security(SecurityErrorType::EncryptionFailed),
                    ErrorSeverity::High,
                    format!("Failed to write encrypted data: {}", e),
                    "encryption_manager".to_string(),
                ))?;
        }
        
        dest_file.flush().await?;
        Ok(())
    }

    pub async fn decrypt_file(&self, source: &Path, dest: &Path) -> Result<()> {
        let mut source_file = File::open(source)
            .await
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::DecryptionFailed),
                ErrorSeverity::High,
                format!("Failed to open source file: {}", e),
                "encryption_manager".to_string(),
            ))?;

        let mut dest_file = File::create(dest)
            .await
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::DecryptionFailed),
                ErrorSeverity::High,
                format!("Failed to create destination file: {}", e),
                "encryption_manager".to_string(),
            ))?;

        let mut buffer = vec![0u8; 1024 * 1024 + 40]; // 1MB chunk + overhead for authentication tag
        let mut hasher = sha2::Sha256::new();
        
        loop {
            let n = source_file.read(&mut buffer).await
                .map_err(|e| Error::new(
                    ErrorCategory::Security(SecurityErrorType::DecryptionFailed),
                    ErrorSeverity::High,
                    format!("Failed to read encrypted data: {}", e),
                    "encryption_manager".to_string(),
                ))?;

            if n == 0 { break; }
            
            hasher.update(&buffer[..n]);
            let chunk_hash = format!("{:x}", hasher.clone().finalize());
            let decrypted_chunk = self.decrypt_block(&buffer[..n], &chunk_hash).await?;
            dest_file.write_all(&decrypted_chunk).await
                .map_err(|e| Error::new(
                    ErrorCategory::Security(SecurityErrorType::DecryptionFailed),
                    ErrorSeverity::High,
                    format!("Failed to write decrypted data: {}", e),
                    "encryption_manager".to_string(),
                ))?;
        }
        
        dest_file.flush().await?;
        Ok(())
    }
}

// Implement EncryptionEngine trait for EncryptionManager
impl EncryptionEngine for EncryptionManager {
    fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut hasher = sha2::Sha256::new();
        hasher.update(data);
        let hash = format!("{:x}", hasher.finalize());
        
        tokio::runtime::Handle::current().block_on(async {
            self.encrypt_block(data, &hash).await
        })
    }

    fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut hasher = sha2::Sha256::new();
        hasher.update(data);
        let hash = format!("{:x}", hasher.finalize());
        
        tokio::runtime::Handle::current().block_on(async {
            self.decrypt_block(data, &hash).await
        })
    }

    fn get_key_type(&self) -> KeyType {
        KeyType::Block
    }

    fn get_key_status(&self) -> KeyStatus {
        KeyStatus::Valid
    }
}

struct FileKeyStore {
    store_path: std::path::PathBuf,
    master_key: Key<Aes256Gcm>,
}

impl std::fmt::Debug for FileKeyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileKeyStore")
            .field("store_path", &self.store_path)
            .field("master_key", &"[REDACTED]")
            .finish()
    }
}

// Implementation block for key storage functionality
impl FileKeyStore {
    async fn new(master_key: &Key<Aes256Gcm>) -> Result<Self> {
        let mut store_path = dirs::data_dir()
            .ok_or_else(|| SkylockError::Generic("Could not find data directory".into()))?;
        store_path.push("skylock");
        store_path.push("keys");

        tokio::fs::create_dir_all(&store_path).await?;

        Ok(Self {
            store_path,
            master_key: *master_key,
        })
    }

    fn get_key_path(&self, file_path: &Path) -> std::path::PathBuf {
        let file_hash = sha2::Sha256::digest(
            file_path.to_string_lossy().as_bytes(),
        );
        self.store_path.join(format!("{:x}.key", file_hash))
    }

    async fn store_block_key(&self, block_key: &BlockKey) -> Result<()> {
        let mut key_path = self.store_path.clone();
        key_path.push("blocks");
        key_path.push(&block_key.block_hash);

        if let Some(parent) = key_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let cipher = Aes256Gcm::new(&self.master_key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        
        let serialized_key = serde_json::to_vec(block_key)
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::InvalidCredentials),
                ErrorSeverity::High,
                format!("Failed to serialize block key: {}", e),
                "encryption_manager".to_string(),
            ))?;

        let mut encrypted_data = nonce.to_vec();
        encrypted_data.extend_from_slice(
            &cipher
                .encrypt(&nonce, &*serialized_key)
                .map_err(|e| Error::new(
                    ErrorCategory::Security(SecurityErrorType::EncryptionFailed),
                    ErrorSeverity::High,
                    format!("Block key encryption failed: {}", e),
                    "encryption_manager".to_string(),
                ))?,
        );

        tokio::fs::write(&key_path, encrypted_data).await?;
        Ok(())
    }

    async fn load_block_key(&self, block_id: &str) -> Result<BlockKey> {
        let mut key_path = self.store_path.clone();
        key_path.push("blocks");
        key_path.push(block_id);

        let encrypted_data = tokio::fs::read(&key_path).await?;
        let cipher = Aes256Gcm::new(&self.master_key);
        let nonce = Nonce::from_slice(&encrypted_data[..12]);
        
        let decrypted_data = cipher
            .decrypt(nonce, &encrypted_data[12..])
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::DecryptionFailed),
                ErrorSeverity::High,
                format!("Block key decryption failed: {}", e),
                "encryption_manager".to_string(),
            ))?;

        let block_key: BlockKey = serde_json::from_slice(&decrypted_data)
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::InvalidCredentials),
                ErrorSeverity::High,
                format!("Invalid block key format: {}", e),
                "encryption_manager".to_string(),
            ))?;

        Ok(block_key)
    }

    async fn rotate_master_key(&mut self, new_master_key: &Key<Aes256Gcm>) -> Result<()> {
        // Re-encrypt all block keys with new master key
        let mut dir = tokio::fs::read_dir(&self.store_path).await?;

        while let Some(entry) = dir.next_entry().await? {
            if let Ok(file_type) = entry.file_type().await {
                if file_type.is_file() {
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if ext == "key" {
                            // Read and decrypt with old key
                            let encrypted_data = tokio::fs::read(&path).await?;
                            let old_cipher = Aes256Gcm::new(&self.master_key);
                            let old_nonce = Nonce::from_slice(&encrypted_data[..12]);
                            let decrypted_data = old_cipher
                                .decrypt(old_nonce, &encrypted_data[12..])
                                .map_err(|e| Error::new(
                                    ErrorCategory::Security(SecurityErrorType::DecryptionFailed),
                                    ErrorSeverity::High,
                                    format!("Failed to decrypt key during rotation: {}", e),
                                    "encryption_manager".to_string(),
                                ))?;

                            // Re-encrypt with new key
                            let new_cipher = Aes256Gcm::new(new_master_key);
                            let new_nonce = Aes256Gcm::generate_nonce(&mut OsRng);
                            let mut new_encrypted_data = new_nonce.to_vec();
                            new_encrypted_data.extend_from_slice(
                                &new_cipher
                                    .encrypt(&new_nonce, &*decrypted_data)
                                    .map_err(|e| Error::new(
                                        ErrorCategory::Security(SecurityErrorType::EncryptionFailed),
                                        ErrorSeverity::High,
                                        format!("Failed to encrypt key during rotation: {}", e),
                                        "encryption_manager".to_string(),
                                    ))?,
                            );

                            // Write back
                            tokio::fs::write(&path, new_encrypted_data).await?;
                        }
                    }
                }
            }
        }

        self.master_key = *new_master_key;
        Ok(())
    }
}
impl EncryptionManager {
    pub async fn rotate_keys(&mut self) -> crate::Result<()> {
        // Generate new master key
        let new_key = Aes256Gcm::generate_key(&mut OsRng);

        // Re-encrypt all file keys with new master key
        self.key_store.rotate_master_key(&new_key).await?;

        self.master_key = new_key;
        Ok(())
    }
}

