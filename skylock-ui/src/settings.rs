use iced::{
    widget::{button, checkbox, column, container, row, text, text_input},
    Element, Length,
};
use serde::{Deserialize, Serialize};
use skylock_core::Result;
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    UpdateSyncthingApiKey(String),
    UpdateHetznerApiKey(String),
    UpdateBackupPath(String),
    ToggleAutoStart(bool),
    ToggleNotifications(bool),
    Save,
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub syncthing_api_key: String,
    pub hetzner_api_key: String,
    pub backup_path: PathBuf,
    pub auto_start: bool,
    pub notifications_enabled: bool,
    pub sync_interval: u64,
    pub backup_schedule: BackupSchedule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSchedule {
    pub daily_backup_hour: u8,
    pub weekly_backup_day: u8,
    pub retention_days: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            syncthing_api_key: String::new(),
            hetzner_api_key: String::new(),
            backup_path: PathBuf::from("C:/Backups"),
            auto_start: true,
            notifications_enabled: true,
            sync_interval: 3600,
            backup_schedule: BackupSchedule {
                daily_backup_hour: 2,
                weekly_backup_day: 0,
                retention_days: 30,
            },
        }
    }
}

pub struct SettingsView {
    settings: Settings,
    changed: bool,
}

impl SettingsView {
    pub fn new() -> Result<Self> {
        let settings = Self::load_settings().unwrap_or_default();
        Ok(Self {
            settings,
            changed: false,
        })
    }

    pub async fn load_settings() -> Result<Settings> {
        let config_path = Self::get_config_path()?;
        if config_path.exists() {
            let data = fs::read_to_string(&config_path).await?;
            Ok(serde_json::from_str(&data)?)
        } else {
            Ok(Settings::default())
        }
    }

    pub async fn save_settings(&self) -> Result<()> {
        let config_path = Self::get_config_path()?;
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let data = serde_json::to_string_pretty(&self.settings)?;
        fs::write(&config_path, data).await?;
        Ok(())
    }

    fn get_config_path() -> Result<PathBuf> {
        let mut path = dirs::config_dir()
            .ok_or_else(|| SkylockError::Generic("Could not find config directory".into()))?;
        path.push("skylock");
        path.push("config.json");
        Ok(path)
    }

    pub fn view(&self) -> Element<SettingsMessage> {
        let content = column![
            text("Settings").size(24),
            row![
                text("Syncthing API Key:"),
                text_input("Enter API key", &self.settings.syncthing_api_key)
                    .on_input(SettingsMessage::UpdateSyncthingApiKey)
            ],
            row![
                text("Hetzner API Key:"),
                text_input("Enter API key", &self.settings.hetzner_api_key)
                    .on_input(SettingsMessage::UpdateHetznerApiKey)
            ],
            row![
                text("Backup Path:"),
                text_input(
                    "Enter backup path",
                    &self.settings.backup_path.to_string_lossy()
                )
                .on_input(SettingsMessage::UpdateBackupPath)
            ],
            row![
                checkbox(
                    "Start with Windows",
                    self.settings.auto_start,
                    SettingsMessage::ToggleAutoStart
                )
            ],
            row![
                checkbox(
                    "Enable Notifications",
                    self.settings.notifications_enabled,
                    SettingsMessage::ToggleNotifications
                )
            ],
            row![
                button("Save").on_press(SettingsMessage::Save),
                button("Cancel").on_press(SettingsMessage::Cancel)
            ]
        ]
        .spacing(10)
        .padding(20);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }

    pub fn update(&mut self, message: SettingsMessage) {
        match message {
            SettingsMessage::UpdateSyncthingApiKey(key) => {
                self.settings.syncthing_api_key = key;
                self.changed = true;
            }
            SettingsMessage::UpdateHetznerApiKey(key) => {
                self.settings.hetzner_api_key = key;
                self.changed = true;
            }
            SettingsMessage::UpdateBackupPath(path) => {
                self.settings.backup_path = PathBuf::from(path);
                self.changed = true;
            }
            SettingsMessage::ToggleAutoStart(enabled) => {
                self.settings.auto_start = enabled;
                self.changed = true;
                // Update Windows registry for auto-start
                self.update_auto_start(enabled);
            }
            SettingsMessage::ToggleNotifications(enabled) => {
                self.settings.notifications_enabled = enabled;
                self.changed = true;
            }
            SettingsMessage::Save => {
                tokio::spawn(async move {
                    if let Err(e) = self.save_settings().await {
                        eprintln!("Failed to save settings: {}", e);
                    }
                });
                self.changed = false;
            }
            SettingsMessage::Cancel => {
                // Reload settings
                if let Ok(settings) = Self::load_settings() {
                    self.settings = settings;
                }
                self.changed = false;
            }
        }
    }

    fn update_auto_start(&self, enabled: bool) {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
        if let Ok(key) = hkcu.create_subkey(path) {
            let (key, _) = key;
            if enabled {
                if let Ok(exe_path) = std::env::current_exe() {
                    let _ = key.set_value("Skylock", &exe_path.to_string_lossy().to_string());
                }
            } else {
                let _ = key.delete_value("Skylock");
            }
        }
    }
}
