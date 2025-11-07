//! Secure SFTP Client with Ed25519 Key Authentication
//! 
//! This module provides high-security SFTP connectivity with:
//! - Ed25519 public key authentication (no passwords)
//! - Host key verification
//! - Metadata encryption (filenames, paths)
//! - Zero-knowledge architecture

use skylock_core::{Result, SkylockError, StorageErrorType};
use ssh2::{Session, KnownHosts};
use std::{
    path::{Path, PathBuf},
    io::{Read, Write},
    net::TcpStream,
    fs,
};
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use tracing::{info, debug, warn, error};
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use rand::RngCore;

/// Configuration for secure SFTP connection
#[derive(Debug, Clone)]
pub struct SecureSftpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Path to Ed25519 private key
    pub private_key_path: PathBuf,
    /// Optional passphrase for encrypted private key
    pub key_passphrase: Option<String>,
    /// Path to known_hosts file for host key verification
    pub known_hosts_path: Option<PathBuf>,
    /// Encryption key for metadata (filenames, paths)
    pub metadata_key: [u8; 32],
}

/// Secure SFTP client with key-based authentication and metadata encryption
pub struct SecureSftpClient {
    session: Session,
    sftp: ssh2::Sftp,
    config: SecureSftpConfig,
    metadata_cipher: Aes256Gcm,
}

impl SecureSftpClient {
    /// Connect using Ed25519 key authentication (no passwords!)
    pub fn connect(config: SecureSftpConfig) -> Result<Self> {
        info!("🔐 Establishing secure SFTP connection to {}:{}", config.host, config.port);
        
        // Establish TCP connection
        let tcp = TcpStream::connect(format!("{}:{}", config.host, config.port))
            .map_err(|e| {
                error!("Failed to connect to {}:{}: {}", config.host, config.port, e);
                SkylockError::Storage(StorageErrorType::ConnectionFailed(e.to_string()))
            })?;

        // Create SSH session
        let mut session = Session::new()
            .map_err(|e| SkylockError::Storage(StorageErrorType::ConnectionFailed(e.to_string())))?;

        session.set_tcp_stream(tcp);
        session.handshake()
            .map_err(|e| {
                error!("SSH handshake failed: {}", e);
                SkylockError::Storage(StorageErrorType::ConnectionFailed(e.to_string()))
            })?;

        // Verify host key (MITM protection)
        if let Some(ref known_hosts_path) = config.known_hosts_path {
            Self::verify_host_key(&session, &config.host, known_hosts_path)?;
        } else {
            warn!("⚠️  Host key verification disabled - vulnerable to MITM attacks!");
        }

        // Authenticate using Ed25519 private key
        debug!("🔑 Authenticating with Ed25519 key: {}", config.private_key_path.display());
        session.userauth_pubkey_file(
            &config.username,
            None, // No public key file needed
            &config.private_key_path,
            config.key_passphrase.as_deref(),
        ).map_err(|e| {
            error!("Key authentication failed: {}", e);
            SkylockError::Storage(StorageErrorType::AuthenticationFailed)
        })?;

        if !session.authenticated() {
            error!("Authentication failed for user: {}", config.username);
            return Err(SkylockError::Storage(StorageErrorType::AuthenticationFailed));
        }

        info!("✅ Successfully authenticated with Ed25519 key");

        // Initialize SFTP subsystem
        let sftp = session.sftp()
            .map_err(|e| {
                error!("Failed to initialize SFTP: {}", e);
                SkylockError::Storage(StorageErrorType::StorageBoxUnavailable)
            })?;

        // Initialize metadata encryption cipher
        let key = Key::<Aes256Gcm>::from_slice(&config.metadata_key);
        let metadata_cipher = Aes256Gcm::new(key);

        info!("🔒 Metadata encryption initialized");

        Ok(Self {
            session,
            sftp,
            config,
            metadata_cipher,
        })
    }

    /// Verify host key against known_hosts file (prevents MITM attacks)
    fn verify_host_key(session: &Session, host: &str, known_hosts_path: &Path) -> Result<()> {
        debug!("🔍 Verifying host key for {}", host);
        
        let mut known_hosts = session.known_hosts()
            .map_err(|e| SkylockError::Storage(StorageErrorType::ConnectionFailed(e.to_string())))?;

        // Load known_hosts file
        if known_hosts_path.exists() {
            known_hosts.read_file(known_hosts_path, ssh2::KnownHostFileKind::OpenSSH)
                .map_err(|e| {
                    error!("Failed to read known_hosts: {}", e);
                    SkylockError::Storage(StorageErrorType::IOError(e.to_string()))
                })?;
        } else {
            warn!("⚠️  known_hosts file not found: {}", known_hosts_path.display());
        }

        // Get server's host key
        let (key, key_type) = session.host_key()
            .ok_or_else(|| {
                error!("Server did not provide host key");
                SkylockError::Storage(StorageErrorType::AuthenticationFailed)
            })?;

        // Check against known_hosts
        match known_hosts.check(host, key) {
            ssh2::CheckResult::Match => {
                info!("✅ Host key verified successfully");
                Ok(())
            }
            ssh2::CheckResult::NotFound => {
                warn!("⚠️  Host {} not in known_hosts - adding automatically", host);
                // In production, you should prompt the user!
                known_hosts.add(host, key, "", key_type.into())
                    .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;
                known_hosts.write_file(known_hosts_path, ssh2::KnownHostFileKind::OpenSSH)
                    .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;
                Ok(())
            }
            ssh2::CheckResult::Mismatch => {
                error!("❌ HOST KEY MISMATCH! Possible MITM attack for {}", host);
                Err(SkylockError::Storage(StorageErrorType::AuthenticationFailed))
            }
            ssh2::CheckResult::Failure => {
                error!("❌ Host key verification failed");
                Err(SkylockError::Storage(StorageErrorType::AuthenticationFailed))
            }
        }
    }

    /// Encrypt filename/path for zero-knowledge storage
    fn encrypt_metadata(&self, plaintext: &str) -> Result<String> {
        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the metadata
        let ciphertext = self.metadata_cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| {
                error!("Metadata encryption failed: {}", e);
                SkylockError::Storage(StorageErrorType::IOError(e.to_string()))
            })?;

        // Combine nonce + ciphertext and encode as URL-safe base64
        let mut combined = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);

        Ok(URL_SAFE_NO_PAD.encode(&combined))
    }

    /// Decrypt filename/path
    fn decrypt_metadata(&self, encrypted: &str) -> Result<String> {
        // Decode from base64
        let combined = URL_SAFE_NO_PAD.decode(encrypted)
            .map_err(|e| {
                error!("Failed to decode metadata: {}", e);
                SkylockError::Storage(StorageErrorType::IOError(e.to_string()))
            })?;

        if combined.len() < 12 {
            return Err(SkylockError::Storage(StorageErrorType::IOError(
                "Invalid encrypted metadata".to_string()
            )));
        }

        // Split nonce and ciphertext
        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        // Decrypt
        let plaintext = self.metadata_cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| {
                error!("Metadata decryption failed: {}", e);
                SkylockError::Storage(StorageErrorType::IOError(e.to_string()))
            })?;

        String::from_utf8(plaintext)
            .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))
    }

    /// Upload file with encrypted filename
    pub async fn upload_file(&self, local_path: &Path, remote_path: &Path) -> Result<u64> {
        // Encrypt the remote path (zero-knowledge)
        let encrypted_path = self.encrypt_path(remote_path)?;
        
        let enc_display = encrypted_path.to_string_lossy();
        info!("📤 Uploading (encrypted): {} -> encrypted:{}", 
            local_path.display(), 
            &enc_display[..32.min(enc_display.len())]
        );

        // Create encrypted parent directories
        if let Some(parent) = encrypted_path.parent() {
            self.create_dir_all(parent).await?;
        }

        let mut local_file = tokio::fs::File::open(local_path).await
            .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;

        let mut remote_file = self.sftp.create(&encrypted_path)
            .map_err(|e| {
                error!("Failed to create remote file: {}", e);
                SkylockError::Storage(StorageErrorType::AccessDenied)
            })?;

        let mut buffer = vec![0; 65536]; // 64KB chunks
        let mut total_bytes = 0u64;

        loop {
            let n = local_file.read(&mut buffer).await
                .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;
            
            if n == 0 {
                break; // EOF
            }

            remote_file.write_all(&buffer[..n])
                .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;
            
            total_bytes += n as u64;
            
            if total_bytes % (1024 * 1024) == 0 {
                debug!("📤 Uploaded {} MB", total_bytes / (1024 * 1024));
            }
        }

        info!("✅ Upload complete: {} bytes", total_bytes);
        Ok(total_bytes)
    }

    /// Download file with encrypted filename
    pub async fn download_file(&self, remote_path: &Path, local_path: &Path) -> Result<u64> {
        let encrypted_path = self.encrypt_path(remote_path)?;
        
        let enc_display = encrypted_path.to_string_lossy();
        info!("📥 Downloading (encrypted): encrypted:{} -> {}", 
            &enc_display[..32.min(enc_display.len())],
            local_path.display()
        );

        // Create local parent directories
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;
        }

        let mut remote_file = self.sftp.open(&encrypted_path)
            .map_err(|e| {
                error!("Failed to open remote file: {}", e);
                SkylockError::Storage(StorageErrorType::FileNotFound)
            })?;

        let mut local_file = tokio::fs::File::create(local_path).await
            .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;

        let mut buffer = vec![0; 65536];
        let mut total_bytes = 0u64;

        loop {
            let n = remote_file.read(&mut buffer)
                .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;
            
            if n == 0 {
                break;
            }

            local_file.write_all(&buffer[..n]).await
                .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;
            
            total_bytes += n as u64;
            
            if total_bytes % (1024 * 1024) == 0 {
                debug!("📥 Downloaded {} MB", total_bytes / (1024 * 1024));
            }
        }

        info!("✅ Download complete: {} bytes", total_bytes);
        Ok(total_bytes)
    }

    /// Delete file with encrypted filename
    pub fn delete_file(&self, remote_path: &Path) -> Result<()> {
        let encrypted_path = self.encrypt_path(remote_path)?;
        
        let enc_display = encrypted_path.to_string_lossy();
        info!("🗑️  Deleting (encrypted): encrypted:{}", 
            &enc_display[..32.min(enc_display.len())]
        );

        self.sftp.unlink(&encrypted_path)
            .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;

        Ok(())
    }

    /// List directory contents (decrypted)
    pub fn list_dir(&self, remote_path: &Path) -> Result<Vec<PathBuf>> {
        let encrypted_path = self.encrypt_path(remote_path)?;
        
        let entries = self.sftp.readdir(&encrypted_path)
            .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;

        // Decrypt filenames
        let mut decrypted = Vec::new();
        for (path, _stat) in entries {
            if let Some(filename) = path.file_name() {
                if let Some(filename_str) = filename.to_str() {
                    match self.decrypt_metadata(filename_str) {
                        Ok(decrypted_name) => {
                            let mut full_path = path.clone();
                            full_path.set_file_name(decrypted_name);
                            decrypted.push(full_path);
                        }
                        Err(e) => {
                            warn!("Failed to decrypt filename: {} - {}", filename_str, e);
                        }
                    }
                }
            }
        }

        Ok(decrypted)
    }

    /// Create all parent directories with encrypted names
    async fn create_dir_all(&self, path: &Path) -> Result<()> {
        let mut current = PathBuf::new();
        
        for component in path.components() {
            current.push(component);
            
            match self.sftp.mkdir(&current, 0o755) {
                Ok(_) => debug!("📁 Created directory: {:?}", current),
                Err(e) if e.message().contains("already exists") => {
                    // Directory exists, continue
                }
                Err(_) => return Err(SkylockError::Storage(StorageErrorType::AccessDenied)),
            }
        }
        
        Ok(())
    }

    /// Helper: Encrypt a full path (each component separately for efficient directory traversal)
    fn encrypt_path(&self, path: &Path) -> Result<PathBuf> {
        let mut encrypted = PathBuf::new();
        
        for component in path.components() {
            if let Some(os_str) = component.as_os_str().to_str() {
                let encrypted_component = self.encrypt_metadata(os_str)?;
                encrypted.push(encrypted_component);
            }
        }
        
        Ok(encrypted)
    }

    /// Test connection
    pub fn test_connection(&self) -> Result<()> {
        if !self.session.authenticated() {
            return Err(SkylockError::Storage(StorageErrorType::AuthenticationFailed));
        }

        // Try to stat root directory
        self.sftp.stat(Path::new("/"))
            .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;

        info!("✅ Connection test successful");
        Ok(())
    }
}

/// Helper function to generate Ed25519 SSH key pair
pub fn generate_ed25519_keypair(output_path: &Path, passphrase: Option<&str>) -> Result<()> {
    use std::process::Command;
    
    info!("🔑 Generating Ed25519 key pair at: {}", output_path.display());
    
    let mut cmd = Command::new("ssh-keygen");
    cmd.arg("-t").arg("ed25519")
        .arg("-f").arg(output_path)
        .arg("-C").arg("skylock-backup")
        .arg("-q"); // Quiet mode

    if passphrase.is_some() {
        cmd.arg("-N").arg(passphrase.unwrap());
    } else {
        cmd.arg("-N").arg(""); // No passphrase
    }

    let output = cmd.output()
        .map_err(|e| SkylockError::Storage(StorageErrorType::IOError(e.to_string())))?;

    if !output.status.success() {
        error!("Failed to generate SSH key: {}", String::from_utf8_lossy(&output.stderr));
        return Err(SkylockError::Storage(StorageErrorType::IOError(
            "SSH key generation failed".to_string()
        )));
    }

    info!("✅ Ed25519 key pair generated successfully");
    info!("📝 Private key: {}", output_path.display());
    info!("📝 Public key: {}.pub", output_path.display());
    info!("⚠️  Upload the public key to your Hetzner Storage Box!");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_encryption() {
        let config = SecureSftpConfig {
            host: "test.example.com".to_string(),
            port: 22,
            username: "test".to_string(),
            private_key_path: PathBuf::from("/tmp/test_key"),
            key_passphrase: None,
            known_hosts_path: None,
            metadata_key: [42u8; 32],
        };

        let key = Key::<Aes256Gcm>::from_slice(&config.metadata_key);
        let cipher = Aes256Gcm::new(key);

        let client = SecureSftpClient {
            session: unsafe { std::mem::zeroed() }, // Mock for test
            sftp: unsafe { std::mem::zeroed() },
            config,
            metadata_cipher: cipher,
        };

        let plaintext = "secret_file.txt";
        let encrypted = client.encrypt_metadata(plaintext).unwrap();
        let decrypted = client.decrypt_metadata(&encrypted).unwrap();

        assert_eq!(plaintext, decrypted);
        assert_ne!(plaintext, encrypted); // Should be different!
    }
}
