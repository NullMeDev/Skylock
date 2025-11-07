use crate::error::Result;
use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, NewAead};
use rand::Rng;
use std::path::PathBuf;
use tokio::fs;

pub struct CredentialManager {
    key_file: PathBuf,
    cipher: Aes256Gcm,
}

impl CredentialManager {
    pub fn new(key_file: PathBuf) -> Result<Self> {
        let key = if key_file.exists() {
            fs::read(&key_file).await?
        } else {
            let mut key = [0u8; 32];
            rand::thread_rng().fill(&mut key);
            fs::write(&key_file, &key).await?;
            key.to_vec()
        };

        let cipher = Aes256Gcm::new(Key::from_slice(&key));

        Ok(Self {
            key_file,
            cipher,
        })
    }

    pub async fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill(&mut nonce);
        let nonce = Nonce::from_slice(&nonce);

        let ciphertext = self.cipher
            .encrypt(nonce, data)
            .map_err(|_| SecurityErrorType::EncryptionFailed)?;

        let mut result = nonce.to_vec();
        result.extend(ciphertext);
        Ok(result)
    }

    pub async fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 12 {
            return Err(SecurityErrorType::DecryptionFailed)?;
        }

        let (nonce, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce);

        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| SecurityErrorType::DecryptionFailed.into())
    }

    pub async fn store_credential(&self, key: &str, value: &str) -> Result<()> {
        let encrypted = self.encrypt(value.as_bytes()).await?;
        let path = self.credential_path(key);
        fs::create_dir_all(path.parent().unwrap()).await?;
        fs::write(path, encrypted).await?;
        Ok(())
    }

    pub async fn get_credential(&self, key: &str) -> Result<String> {
        let path = self.credential_path(key);
        let encrypted = fs::read(path).await?;
        let decrypted = self.decrypt(&encrypted).await?;
        String::from_utf8(decrypted)
            .map_err(|_| SecurityErrorType::InvalidCredential.into())
    }

    fn credential_path(&self, key: &str) -> PathBuf {
        self.key_file.parent().unwrap()
            .join("credentials")
            .join(key)
    }
}
