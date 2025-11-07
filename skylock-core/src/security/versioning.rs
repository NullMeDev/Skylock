use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyVersion {
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub encrypted_key: Vec<u8>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedKey {
    pub key_id: String,
    pub current_version: u32,
    pub versions: HashMap<u32, KeyVersion>,
    pub policy: KeyVersioningPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyVersioningPolicy {
    pub max_versions: u32,
    pub version_ttl_days: Option<u32>,
    pub auto_rotate: bool,
    pub rotation_interval_days: Option<u32>,
}

impl Default for KeyVersioningPolicy {
    fn default() -> Self {
        Self {
            max_versions: 5,
            version_ttl_days: Some(365),
            auto_rotate: true,
            rotation_interval_days: Some(90),
        }
    }
}

impl VersionedKey {
    pub fn new(key_id: String, policy: KeyVersioningPolicy) -> Self {
        Self {
            key_id,
            current_version: 0,
            versions: HashMap::new(),
            policy,
        }
    }

    pub fn add_version(&mut self, encrypted_key: Vec<u8>, metadata: HashMap<String, String>) {
        let new_version = self.current_version + 1;
        
        let version = KeyVersion {
            version: new_version,
            created_at: Utc::now(),
            expires_at: self.policy.version_ttl_days.map(|days| {
                Utc::now() + chrono::Duration::days(days as i64)
            }),
            encrypted_key,
            metadata,
        };

        self.versions.insert(new_version, version);
        self.current_version = new_version;
        
        // Cleanup old versions if needed
        self.cleanup_versions();
    }

    pub fn get_current_version(&self) -> Option<&KeyVersion> {
        self.versions.get(&self.current_version)
    }

    pub fn get_version(&self, version: u32) -> Option<&KeyVersion> {
        self.versions.get(&version)
    }

    pub fn needs_rotation(&self) -> bool {
        if !self.policy.auto_rotate {
            return false;
        }

        if let Some(interval_days) = self.policy.rotation_interval_days {
            if let Some(current) = self.get_current_version() {
                let age = Utc::now() - current.created_at;
                return age.num_days() >= interval_days as i64;
            }
        }

        false
    }

    fn cleanup_versions(&mut self) {
        // Remove expired versions
        self.versions.retain(|_, v| {
            if let Some(expires_at) = v.expires_at {
                expires_at > Utc::now()
            } else {
                true
            }
        });

        // Keep only max_versions most recent versions
        if self.versions.len() > self.policy.max_versions as usize {
            let mut version_keys: Vec<_> = self.versions.keys().cloned().collect();
            version_keys.sort_by(|a, b| {
                self.versions[a].created_at.cmp(&self.versions[b].created_at)
            });
            
            let versions_to_remove = version_keys.len() - (self.policy.max_versions as usize);
            for &version_key in version_keys.iter().take(versions_to_remove) {
                self.versions.remove(&version_key);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionMetadata {
    pub created_by: String,
    pub rotation_reason: Option<String>,
    pub tags: HashMap<String, String>,
    pub checksum: String,
}