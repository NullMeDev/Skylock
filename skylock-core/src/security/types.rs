use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum KeyType {
    Master,
    File,
    Block,
}

#[derive(Debug, Clone)]
pub struct SecureKey {
    pub key_type: KeyType,
    pub key_data: Vec<u8>,
    pub created_at: DateTime<Utc>,
    pub last_used: DateTime<Utc>,
}

pub trait EncryptionEngine: Send + Sync {
    fn encrypt(&self, data: &[u8]) -> crate::Result<Vec<u8>>;
    fn decrypt(&self, data: &[u8]) -> crate::Result<Vec<u8>>;
    fn get_key_type(&self) -> KeyType;
    fn get_key_status(&self) -> SecureKey;
}