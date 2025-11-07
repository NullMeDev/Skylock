use crate::{
    Result,
    error_types::{Error, ErrorCategory, ErrorSeverity, SecurityErrorType}
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

#[async_trait]
pub trait HsmProvider: Send + Sync {
    async fn generate_key(&self) -> Result<Vec<u8>>;
    async fn store_key(&self, key_id: &str, key_data: &[u8]) -> Result<()>;
    async fn get_key(&self, key_id: &str) -> Result<Vec<u8>>;
    async fn delete_key(&self, key_id: &str) -> Result<()>;
    async fn list_keys(&self) -> Result<Vec<String>>;
}

pub struct SoftwareHsm {
    key_store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl SoftwareHsm {
    pub fn new() -> Self {
        Self {
            key_store: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl HsmProvider for SoftwareHsm {
    async fn generate_key(&self) -> Result<Vec<u8>> {
        let mut key = vec![0u8; 32];
        getrandom::getrandom(&mut key)
            .map_err(|e| Error::new(
                ErrorCategory::Security(SecurityErrorType::KeyGenerationFailed),
                ErrorSeverity::High,
                format!("Random key generation failed: {}", e),
                "software_hsm".to_string(),
            ))?;
        Ok(key)
    }

    async fn store_key(&self, key_id: &str, key_data: &[u8]) -> Result<()> {
        let mut store = self.key_store.write().await;
        store.insert(key_id.to_string(), key_data.to_vec());
        Ok(())
    }

    async fn get_key(&self, key_id: &str) -> Result<Vec<u8>> {
        let store = self.key_store.read().await;
        store.get(key_id)
            .cloned()
            .ok_or_else(|| Error::new(
                ErrorCategory::Security(SecurityErrorType::KeyNotFound),
                ErrorSeverity::High,
                format!("HSM key not found: {}", key_id),
                "software_hsm".to_string(),
            ).into())
    }

    async fn delete_key(&self, key_id: &str) -> Result<()> {
        let mut store = self.key_store.write().await;
        store.remove(key_id);
        Ok(())
    }

    async fn list_keys(&self) -> Result<Vec<String>> {
        let store = self.key_store.read().await;
        Ok(store.keys().cloned().collect())
    }
}