use crate::error::{Result, ConfigErrorType};
use super::Config;
use std::path::Path;

impl Config {
    pub fn validate(&self) -> Result<()> {
        self.validate_paths()?;
        self.validate_credentials()?;
        self.validate_network_settings()?;
        self.validate_backup_settings()?;
        Ok(())
    }

    fn validate_paths(&self) -> Result<()> {
        for path in &self.backup.backup_paths {
            if !Path::new(path).exists() {
                return Err(ConfigErrorType::InvalidPath(path.clone()).into());
            }
        }
        Ok(())
    }

    fn validate_credentials(&self) -> Result<()> {
        if self.hetzner.api_key.trim().is_empty() {
            return Err(ConfigErrorType::MissingCredentials("Hetzner API key".into()).into());
        }
        if self.syncthing.api_key.trim().is_empty() {
            return Err(ConfigErrorType::MissingCredentials("Syncthing API key".into()).into());
        }
        Ok(())
    }

    fn validate_network_settings(&self) -> Result<()> {
        if !self.syncthing.api_url.starts_with("http://") && !self.syncthing.api_url.starts_with("https://") {
            return Err(ConfigErrorType::InvalidUrl("Syncthing API URL".into()).into());
        }
        if self.hetzner.port <= 0 || self.hetzner.port > 65535 {
            return Err(ConfigErrorType::InvalidPort(self.hetzner.port).into());
        }
        Ok(())
    }

    fn validate_backup_settings(&self) -> Result<()> {
        if self.backup.retention_days <= 0 {
            return Err(ConfigErrorType::InvalidSetting("retention_days must be positive".into()).into());
        }
        // Validate cron schedule
        if let Err(_) = cron::Schedule::from_str(&self.backup.schedule) {
            return Err(ConfigErrorType::InvalidSetting("invalid backup schedule".into()).into());
        }
        Ok(())
    }
}
