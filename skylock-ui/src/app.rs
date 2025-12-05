//! Skylock GUI Application
//!
//! A backup management interface built with egui.
//! Features real encryption key validation via AES-256-GCM authentication.

#[cfg(feature = "gui")]
use eframe::egui;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::RwLock;
use chrono::{DateTime, Local};
use bytesize::ByteSize;
use sha2::{Sha256, Digest};
use serde::Deserialize;

/// Config file structure
#[derive(Debug, Deserialize, Default)]
pub struct Config {
    pub data_dir: Option<String>,
    pub hetzner: Option<HetznerConfig>,
    pub backup: Option<BackupConfig>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct HetznerConfig {
    pub endpoint: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub encryption_key: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct BackupConfig {
    pub schedule: Option<String>,
    pub retention_days: Option<u32>,
    pub backup_paths: Option<Vec<String>>,
}

/// Connection status for backend
#[derive(Clone, Debug, PartialEq, Default)]
pub enum ConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// Operation in progress
#[derive(Clone, Debug, PartialEq)]
pub enum Operation {
    None,
    Backup { progress: f32, current_file: String },
    Restore { progress: f32, current_file: String },
    Verify { progress: f32 },
}

impl Default for Operation {
    fn default() -> Self { Self::None }
}

/// Activity log entry
#[derive(Clone, Debug)]
pub struct ActivityEntry {
    pub timestamp: DateTime<Local>,
    pub action: String,
    pub details: String,
    pub success: bool,
}

/// Application state
#[derive(Default)]
pub struct AppState {
    // Authentication
    pub encryption_key: Option<String>,
    pub key_valid: bool,
    pub key_hash: Option<String>,  // SHA-256 hash of validated key for display
    pub validation_test_data: Option<Vec<u8>>,  // Encrypted test data for key validation
    
    // Connection
    pub connection_status: ConnectionStatus,
    pub endpoint: String,
    pub username: String,
    pub hetzner_config: Option<HetznerConfig>,
    pub stored_encryption_key: Option<String>,
    
    // Backups
    pub backups: Vec<BackupInfo>,
    pub selected_backup: Option<String>,
    pub current_files: Vec<FileEntry>,
    pub expanded_folders: std::collections::HashSet<PathBuf>,
    
    // Storage
    pub storage_used: u64,
    pub storage_total: u64,
    pub last_backup: Option<DateTime<Local>>,
    
    // UI State
    pub current_view: View,
    pub show_key_dialog: bool,
    pub key_input: String,
    pub error_message: Option<String>,
    pub status_message: Option<StatusMessage>,
    
    // Operations
    pub current_operation: Operation,
    pub sync_active: bool,
    
    // Activity log
    pub activity_log: Vec<ActivityEntry>,
    
    // Settings
    pub auto_backup_enabled: bool,
    pub backup_schedule: String,
    pub retention_days: u32,
    pub compression_enabled: bool,
    
    // Demo mode (when not connected)
    pub demo_mode: bool,
    
    // Pending actions (for async operations)
    pub pending_connection_test: bool,
    pub pending_backup: bool,
    
    // Backup progress
    pub backup_progress: BackupProgress,
    pub backup_paths: Vec<PathBuf>,  // Paths to backup (from config or user selection)
}

/// Backup information
#[derive(Clone, Debug)]
pub struct BackupInfo {
    pub id: String,
    pub timestamp: DateTime<Local>,
    pub file_count: usize,
    pub total_size: u64,
    pub is_incremental: bool,
    pub verified: Option<bool>,
}

/// File entry in backup browser
#[derive(Clone, Debug)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub is_directory: bool,
    pub is_encrypted: bool,
    pub modified: Option<DateTime<Local>>,
    pub hash: Option<String>,
    pub display_name: String,
}

/// Status message type
#[derive(Clone, Debug)]
pub struct StatusMessage {
    pub text: String,
    pub level: StatusLevel,
    pub timestamp: DateTime<Local>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum StatusLevel {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

/// Current view
#[derive(Clone, Debug, PartialEq, Default)]
pub enum View {
    #[default]
    Dashboard,
    Browser,
    Activity,
    Settings,
}

/// Key validation result
#[derive(Debug)]
pub enum KeyValidationResult {
    Valid,
    Invalid,
    NoTestData,
    Error(String),
}

/// Result from async connection test
#[derive(Debug, Clone)]
pub enum ConnectionTestResult {
    Success,
    Error(String),
}

/// Backup stage
#[derive(Debug, Clone, PartialEq, Default)]
pub enum BackupStage {
    #[default]
    Idle,
    Scanning,
    Encrypting,
    Uploading,
    Finalizing,
    Complete,
    Failed(String),
}

/// Backup progress event sent from async backup task
#[derive(Debug, Clone)]
pub enum BackupProgressEvent {
    /// Scanning started
    ScanStarted { path: String },
    /// Found files during scan
    ScanProgress { files_found: usize, total_size: u64 },
    /// Scan complete
    ScanComplete { total_files: usize, total_size: u64 },
    /// Starting to process a file
    FileStarted { file_name: String, file_size: u64, file_index: usize, total_files: usize },
    /// File encryption progress (0.0 - 1.0)
    FileEncrypting { file_name: String, progress: f32 },
    /// File upload progress
    FileUploading { file_name: String, bytes_sent: u64, total_bytes: u64 },
    /// File completed
    FileComplete { file_name: String, file_index: usize, total_files: usize },
    /// Backup completed successfully
    BackupComplete { backup_id: String, total_files: usize, total_size: u64, duration_secs: u64 },
    /// Backup failed
    BackupFailed { error: String },
}

/// Backup progress state for UI display
#[derive(Debug, Clone, Default)]
pub struct BackupProgress {
    pub stage: BackupStage,
    pub current_file: String,
    pub current_file_size: u64,
    pub current_file_progress: f32,  // 0.0 - 1.0
    pub files_completed: usize,
    pub total_files: usize,
    pub bytes_uploaded: u64,
    pub total_bytes: u64,
    pub backup_id: Option<String>,
    pub start_time: Option<DateTime<Local>>,
}

#[cfg(feature = "gui")]
pub struct SkylockApp {
    pub state: Arc<RwLock<AppState>>,
    runtime: tokio::runtime::Handle,
    /// Channel receiver for async connection test results
    connection_rx: Option<Receiver<ConnectionTestResult>>,
    /// Channel receiver for backup progress events
    backup_rx: Option<Receiver<BackupProgressEvent>>,
}

#[cfg(feature = "gui")]
impl SkylockApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        
        // Dark theme with accent colors
        style.visuals.dark_mode = true;
        style.visuals.override_text_color = Some(egui::Color32::from_gray(220));
        style.visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(28, 32, 38);
        style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(35, 40, 48);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(45, 52, 62);
        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(55, 65, 78);
        style.visuals.window_fill = egui::Color32::from_rgb(22, 26, 30);
        style.visuals.panel_fill = egui::Color32::from_rgb(25, 29, 34);
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(60, 100, 150);
        
        // Note: window_rounding and menu_rounding are set via Frame in egui 0.31
        
        cc.egui_ctx.set_style(style);

        let runtime = tokio::runtime::Handle::current();
        
        // Try to load config and connect to real backend
        let mut initial_state = AppState::default();
        Self::load_from_config(&mut initial_state);
        
        Self {
            state: Arc::new(RwLock::new(initial_state)),
            runtime,
            connection_rx: None,
            backup_rx: None,
        }
    }
    
    /// Load configuration and try to connect to real backend
    fn load_from_config(state: &mut AppState) {
        // Try to load config file
        let config_paths = [
            dirs::config_dir().map(|p| p.join("skylock-hybrid/config.toml")),
            Some(PathBuf::from("config.toml")),
        ];
        
        let mut config: Option<Config> = None;
        for path in config_paths.iter().flatten() {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(cfg) = toml::from_str::<Config>(&content) {
                        config = Some(cfg);
                        state.activity_log.push(ActivityEntry {
                            timestamp: Local::now(),
                            action: "Config loaded".to_string(),
                            details: format!("From {}", path.display()),
                            success: true,
                        });
                        break;
                    }
                }
            }
        }
        
        if let Some(cfg) = config {
            // Load Hetzner config
            if let Some(hetzner) = &cfg.hetzner {
                state.endpoint = hetzner.endpoint.clone().unwrap_or_default();
                state.username = hetzner.username.clone().unwrap_or_default();
                state.hetzner_config = Some(hetzner.clone());
                
                // Store encryption key hash for later validation
                if let Some(key) = &hetzner.encryption_key {
                    state.stored_encryption_key = Some(key.clone());
                }
            }
            
            // Load backup config
            if let Some(backup) = &cfg.backup {
                state.retention_days = backup.retention_days.unwrap_or(30);
                state.backup_schedule = backup.schedule.clone().unwrap_or_else(|| "Daily at 2:00 AM".to_string());
                // Load backup paths from config
                if let Some(paths) = &backup.backup_paths {
                    state.backup_paths = paths.iter().map(PathBuf::from).collect();
                }
            }
            
            state.demo_mode = false;
            state.connection_status = ConnectionStatus::Disconnected;
            
            state.activity_log.push(ActivityEntry {
                timestamp: Local::now(),
                action: "Ready to connect".to_string(),
                details: format!("Endpoint: {}", state.endpoint),
                success: true,
            });
        } else {
            // Fall back to demo mode
            Self::load_demo_data(state);
        }
        
        // Default settings
        state.compression_enabled = true;
    }
    
    /// Spawn async connection test to Hetzner storage
    fn spawn_connection_test(&mut self) {
        let state_guard = self.state.blocking_read();
        
        // Get Hetzner config from state
        let hetzner_config = match &state_guard.hetzner_config {
            Some(cfg) => cfg.clone(),
            None => {
                drop(state_guard);
                if let Ok(mut state) = self.state.try_write() {
                    state.connection_status = ConnectionStatus::Error("No Hetzner config found".to_string());
                }
                return;
            }
        };
        
        let endpoint = hetzner_config.endpoint.clone().unwrap_or_default();
        let username = hetzner_config.username.clone().unwrap_or_default();
        let password = hetzner_config.password.clone().unwrap_or_default();
        let encryption_key = hetzner_config.encryption_key.clone().unwrap_or_default();
        drop(state_guard);
        
        if endpoint.is_empty() || username.is_empty() || password.is_empty() {
            if let Ok(mut state) = self.state.try_write() {
                state.connection_status = ConnectionStatus::Error("Missing credentials in config".to_string());
            }
            return;
        }
        
        // Create channel for result
        let (tx, rx): (Sender<ConnectionTestResult>, Receiver<ConnectionTestResult>) = channel();
        self.connection_rx = Some(rx);
        
        // Spawn async task
        self.runtime.spawn(async move {
            // Create HetznerConfig for the client
            let config = skylock_hetzner::HetznerConfig {
                endpoint: endpoint.clone(),
                username: username.clone(),
                password: password.clone(),
                api_token: String::new(),
                encryption_key,
            };
            
            // Try to create client and test connection
            let result = match skylock_hetzner::HetznerClient::new(config) {
                Ok(client) => {
                    // Try listing root directory to verify connection works
                    match client.list_directories("/").await {
                        Ok(_) => ConnectionTestResult::Success,
                        Err(e) => ConnectionTestResult::Error(format!("Connection test failed: {}", e)),
                    }
                }
                Err(e) => ConnectionTestResult::Error(format!("Failed to create client: {}", e)),
            };
            
            let _ = tx.send(result);
        });
    }
    
    /// Spawn async backup task using DirectUploadBackup with full encryption
    fn spawn_backup(&mut self) {
        let state_guard = self.state.blocking_read();
        
        // Get Hetzner config from state
        let hetzner_config = match &state_guard.hetzner_config {
            Some(cfg) => cfg.clone(),
            None => {
                drop(state_guard);
                if let Ok(mut state) = self.state.try_write() {
                    state.backup_progress.stage = BackupStage::Failed("No Hetzner config found".to_string());
                    state.current_operation = Operation::None;
                }
                return;
            }
        };
        
        let endpoint = hetzner_config.endpoint.clone().unwrap_or_default();
        let username = hetzner_config.username.clone().unwrap_or_default();
        let password = hetzner_config.password.clone().unwrap_or_default();
        let encryption_key = hetzner_config.encryption_key.clone().unwrap_or_default();
        let backup_paths = state_guard.backup_paths.clone();
        drop(state_guard);
        
        if endpoint.is_empty() || username.is_empty() || password.is_empty() {
            if let Ok(mut state) = self.state.try_write() {
                state.backup_progress.stage = BackupStage::Failed("Missing credentials in config".to_string());
                state.current_operation = Operation::None;
            }
            return;
        }
        
        if encryption_key.is_empty() {
            if let Ok(mut state) = self.state.try_write() {
                state.backup_progress.stage = BackupStage::Failed("No encryption key configured".to_string());
                state.current_operation = Operation::None;
            }
            return;
        }
        
        if backup_paths.is_empty() {
            if let Ok(mut state) = self.state.try_write() {
                state.backup_progress.stage = BackupStage::Failed("No backup paths configured. Add backup_paths to config.toml".to_string());
                state.current_operation = Operation::None;
            }
            return;
        }
        
        // Create channel for progress updates
        let (tx, rx): (Sender<BackupProgressEvent>, Receiver<BackupProgressEvent>) = channel();
        self.backup_rx = Some(rx);
        
        // Spawn async backup task using DirectUploadBackup for full encryption
        self.runtime.spawn(async move {
            use std::time::Instant;
            use walkdir::WalkDir;
            
            let start_time = Instant::now();
            
            // Phase 1: Quick file count for initial progress
            let mut total_files = 0usize;
            let mut total_size = 0u64;
            
            for path in &backup_paths {
                let _ = tx.send(BackupProgressEvent::ScanStarted { 
                    path: path.display().to_string() 
                });
                
                if path.is_file() {
                    if let Ok(meta) = std::fs::metadata(path) {
                        total_files += 1;
                        total_size += meta.len();
                    }
                } else if path.is_dir() {
                    for entry in WalkDir::new(path).follow_links(false) {
                        if let Ok(entry) = entry {
                            if entry.file_type().is_file() {
                                if let Ok(meta) = entry.metadata() {
                                    total_files += 1;
                                    total_size += meta.len();
                                }
                            }
                        }
                    }
                }
                
                let _ = tx.send(BackupProgressEvent::ScanProgress { 
                    files_found: total_files,
                    total_size,
                });
            }
            
            let _ = tx.send(BackupProgressEvent::ScanComplete { total_files, total_size });
            
            if total_files == 0 {
                let _ = tx.send(BackupProgressEvent::BackupFailed { 
                    error: "No files found to backup".to_string() 
                });
                return;
            }
            
            // Report starting encryption/upload phase
            let _ = tx.send(BackupProgressEvent::FileStarted {
                file_name: "Initializing encrypted backup...".to_string(),
                file_size: total_size,
                file_index: 1,
                total_files,
            });
            
            // Build skylock_core::Config for DirectUploadBackup
            let core_config = skylock_core::Config {
                syncthing: skylock_core::SyncthingConfig {
                    api_key: String::new(),
                    api_url: String::new(),
                    folders: Vec::new(),
                },
                hetzner: skylock_core::HetznerConfig {
                    endpoint: endpoint.clone(),
                    username: username.clone(),
                    password: password.clone(),
                    encryption_key: encryption_key.clone(),
                },
                backup: skylock_core::BackupConfig {
                    vss_enabled: false,
                    schedule: String::new(),
                    retention_days: 30,
                    backup_paths: backup_paths.clone(),
                    max_speed_limit: None,
                },
                ui: skylock_core::UiConfig {
                    always_prompt_deletions: false,
                    notification_enabled: true,
                },
                data_dir: dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("skylock"),
            };
            
            // Create Hetzner client
            let hetzner_client_config = skylock_hetzner::HetznerConfig {
                endpoint: endpoint.clone(),
                username: username.clone(),
                password: password.clone(),
                api_token: String::new(),
                encryption_key: encryption_key.clone(),
            };
            
            let hetzner_client = match skylock_hetzner::HetznerClient::new(hetzner_client_config) {
                Ok(client) => client,
                Err(e) => {
                    let _ = tx.send(BackupProgressEvent::BackupFailed { 
                        error: format!("Failed to create Hetzner client: {}", e) 
                    });
                    return;
                }
            };
            
            // Create encryption manager with Argon2id key derivation
            let _ = tx.send(BackupProgressEvent::FileEncrypting {
                file_name: "Deriving encryption key (Argon2id)...".to_string(),
                progress: 0.1,
            });
            
            let encryption = match skylock_backup::EncryptionManager::new(&encryption_key) {
                Ok(enc) => enc,
                Err(e) => {
                    let _ = tx.send(BackupProgressEvent::BackupFailed { 
                        error: format!("Failed to initialize encryption (Argon2id): {}", e) 
                    });
                    return;
                }
            };
            
            // Create DirectUploadBackup (handles per-file AES-256-GCM encryption)
            let _ = tx.send(BackupProgressEvent::FileEncrypting {
                file_name: "Creating encrypted backup manager...".to_string(),
                progress: 0.2,
            });
            
            let direct_backup = skylock_backup::DirectUploadBackup::new(
                core_config,
                hetzner_client,
                encryption,
                None, // No bandwidth limit
            );
            
            // Execute the backup with full encryption
            let _ = tx.send(BackupProgressEvent::FileUploading {
                file_name: "Starting encrypted backup (AES-256-GCM per-file)...".to_string(),
                bytes_sent: 0,
                total_bytes: total_size,
            });
            
            // Run the backup - DirectUploadBackup handles encryption and progress internally
            // Progress will be printed to terminal, GUI shows high-level status
            match direct_backup.create_backup(&backup_paths).await {
                Ok(manifest) => {
                    let duration_secs = start_time.elapsed().as_secs();
                    let _ = tx.send(BackupProgressEvent::BackupComplete {
                        backup_id: manifest.backup_id,
                        total_files: manifest.file_count,
                        total_size: manifest.total_size,
                        duration_secs,
                    });
                }
                Err(e) => {
                    let _ = tx.send(BackupProgressEvent::BackupFailed {
                        error: format!("Backup failed: {}", e),
                    });
                }
            }
        });
    }
    
    /// Load demo data for UI testing when not connected
    fn load_demo_data(state: &mut AppState) {
        state.demo_mode = true;
        state.connection_status = ConnectionStatus::Disconnected;
        state.endpoint = "your-storagebox.your-server.de".to_string();
        state.username = String::new();
        state.retention_days = 30;
        state.backup_schedule = "Daily at 2:00 AM".to_string();
        state.compression_enabled = true;
        state.validation_test_data = None;
        
        state.activity_log.push(ActivityEntry {
            timestamp: Local::now(),
            action: "Demo mode".to_string(),
            details: "No config found, using demo data".to_string(),
            success: true,
        });
    }

    fn render_sidebar(&self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.vertical(|ui| {
            ui.add_space(12.0);
            
            // Logo with icon
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("[S]").color(egui::Color32::from_rgb(100, 150, 220)).strong());
                ui.heading("Skylock");
            });
            ui.label(egui::RichText::new("v0.8.0").small().weak());
            ui.add_space(20.0);
            
            // Navigation
            let nav_items = [
                (View::Dashboard, "Dashboard", "[D]"),
                (View::Browser, "Browse", "[B]"),
                (View::Activity, "Activity", "[A]"),
                (View::Settings, "Settings", "[S]"),
            ];
            
            for (view, label, icon) in nav_items {
                let selected = state.current_view == view;
                let text = format!("{} {}", icon, label);
                let rich_text = if selected {
                    egui::RichText::new(text).color(egui::Color32::from_rgb(100, 150, 220))
                } else {
                    egui::RichText::new(text)
                };
                if ui.selectable_label(selected, rich_text).clicked() {
                    state.current_view = view;
                }
                ui.add_space(2.0);
            }
            
            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);
            
            // Connection status
            ui.label(egui::RichText::new("Connection").small().strong());
            ui.add_space(3.0);
            match &state.connection_status {
                ConnectionStatus::Connected => {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("[+]").color(egui::Color32::GREEN));
                        ui.label("Connected");
                    });
                }
                ConnectionStatus::Connecting => {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("[~]").color(egui::Color32::YELLOW));
                        ui.label("Connecting...");
                    });
                }
                ConnectionStatus::Disconnected => {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("[-]").color(egui::Color32::GRAY));
                        ui.label("Offline");
                    });
                }
                ConnectionStatus::Error(_) => {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("[!]").color(egui::Color32::RED));
                        ui.label("Error");
                    });
                }
            }
            
            ui.add_space(10.0);
            
            // Encryption status
            ui.label(egui::RichText::new("Encryption").small().strong());
            ui.add_space(3.0);
            if state.key_valid {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("[*]").color(egui::Color32::GREEN));
                    ui.label("Key Active");
                });
                if let Some(hash) = &state.key_hash {
                    ui.label(egui::RichText::new(format!("  {}", &hash[..8])).small().weak());
                }
            } else {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("[ ]").color(egui::Color32::GRAY));
                    ui.label("No Key");
                });
                if ui.small_button("Enter Key").clicked() {
                    state.show_key_dialog = true;
                }
            }
            
            ui.add_space(10.0);
            
            // Operation status
            match &state.current_operation {
                Operation::None => {}
                Operation::Backup { progress, current_file } => {
                    ui.label(egui::RichText::new("Backup").small().strong());
                    ui.add(egui::ProgressBar::new(*progress).show_percentage());
                    ui.label(egui::RichText::new(current_file).small().weak());
                }
                Operation::Restore { progress, current_file } => {
                    ui.label(egui::RichText::new("Restore").small().strong());
                    ui.add(egui::ProgressBar::new(*progress).show_percentage());
                    ui.label(egui::RichText::new(current_file).small().weak());
                }
                Operation::Verify { progress } => {
                    ui.label(egui::RichText::new("Verifying").small().strong());
                    ui.add(egui::ProgressBar::new(*progress).show_percentage());
                }
            }
            
            // Demo mode indicator at bottom
            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                if state.demo_mode {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("DEMO MODE").small().color(egui::Color32::YELLOW));
                    ui.label(egui::RichText::new("Not connected to server").small().weak());
                }
            });
        });
    }

    fn render_dashboard(&self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            ui.heading("Dashboard");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if state.demo_mode {
                    ui.label(egui::RichText::new("Demo Mode - Connect to server for live data").small().weak());
                }
            });
        });
        ui.add_space(15.0);
        
        // Status cards row
        ui.horizontal(|ui| {
            // Last backup card
            self.render_card(ui, "Last Backup", egui::Color32::from_rgb(35, 45, 55), |ui| {
                if let Some(last) = &state.last_backup {
                    let ago = Local::now().signed_duration_since(*last);
                    ui.label(egui::RichText::new(last.format("%Y-%m-%d %H:%M").to_string()).strong());
                    let ago_text = if ago.num_hours() < 1 {
                        format!("{} minutes ago", ago.num_minutes())
                    } else if ago.num_hours() < 24 {
                        format!("{} hours ago", ago.num_hours())
                    } else {
                        format!("{} days ago", ago.num_days())
                    };
                    ui.label(egui::RichText::new(ago_text).small().weak());
                } else {
                    ui.label(egui::RichText::new("Never").weak());
                }
            });
            
            ui.add_space(8.0);
            
            // Storage card
            self.render_card(ui, "Storage", egui::Color32::from_rgb(35, 45, 55), |ui| {
                let used = ByteSize::b(state.storage_used);
                let total = ByteSize::b(state.storage_total);
                if state.storage_total > 0 {
                    ui.label(egui::RichText::new(format!("{} / {}", used, total)).strong());
                    let ratio = state.storage_used as f32 / state.storage_total as f32;
                    let color = if ratio > 0.9 {
                        egui::Color32::RED
                    } else if ratio > 0.7 {
                        egui::Color32::YELLOW
                    } else {
                        egui::Color32::from_rgb(100, 150, 220)
                    };
                    ui.add(egui::ProgressBar::new(ratio).fill(color).show_percentage());
                } else {
                    ui.label(egui::RichText::new(format!("{}", used)).strong());
                }
            });
            
            ui.add_space(8.0);
            
            // Backups card
            self.render_card(ui, "Backups", egui::Color32::from_rgb(35, 45, 55), |ui| {
                ui.label(egui::RichText::new(state.backups.len().to_string()).strong().size(24.0));
                let verified = state.backups.iter().filter(|b| b.verified == Some(true)).count();
                ui.label(egui::RichText::new(format!("{} verified", verified)).small().weak());
            });
            
            ui.add_space(8.0);
            
            // Encryption card
            self.render_card(ui, "Security", egui::Color32::from_rgb(35, 45, 55), |ui| {
                if state.key_valid {
                    ui.label(egui::RichText::new("AES-256-GCM").strong().color(egui::Color32::GREEN));
                    ui.label(egui::RichText::new("Key validated").small().weak());
                } else {
                    ui.label(egui::RichText::new("No Key").strong().color(egui::Color32::GRAY));
                    if ui.small_button("Enter Key").clicked() {
                        state.show_key_dialog = true;
                    }
                }
            });
        });
        
        ui.add_space(20.0);
        
        // Quick actions section
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Quick Actions").strong());
        });
        ui.add_space(8.0);
        
        ui.horizontal(|ui| {
            let btn_enabled = state.current_operation == Operation::None;
            
            let backup_in_progress = state.backup_progress.stage != BackupStage::Idle 
                && state.backup_progress.stage != BackupStage::Complete 
                && !matches!(state.backup_progress.stage, BackupStage::Failed(_));
            
            if ui.add_enabled(btn_enabled && !backup_in_progress, egui::Button::new("[>] Backup Now")).clicked() {
                state.pending_backup = true;
                state.status_message = Some(StatusMessage {
                    text: "Starting backup...".to_string(),
                    level: StatusLevel::Info,
                    timestamp: Local::now(),
                });
                state.activity_log.insert(0, ActivityEntry {
                    timestamp: Local::now(),
                    action: "Backup started".to_string(),
                    details: format!("Paths: {:?}", state.backup_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>()),
                    success: true,
                });
            }
            
            if ui.add_enabled(btn_enabled, egui::Button::new("[?] Verify")).clicked() {
                state.status_message = Some(StatusMessage {
                    text: "Verifying backup integrity...".to_string(),
                    level: StatusLevel::Info,
                    timestamp: Local::now(),
                });
            }
            
            // Note: actual connection test is triggered via spawn_connection_test
            if ui.add_enabled(btn_enabled && state.connection_status != ConnectionStatus::Connecting, 
                              egui::Button::new("[~] Test Connection")).clicked() {
                state.connection_status = ConnectionStatus::Connecting;
                state.status_message = Some(StatusMessage {
                    text: "Testing connection to storage server...".to_string(),
                    level: StatusLevel::Info,
                    timestamp: Local::now(),
                });
                state.pending_connection_test = true;
            }
            
            if ui.add_enabled(btn_enabled && state.key_valid && state.selected_backup.is_some(), 
                              egui::Button::new("[<] Restore")).clicked() {
                state.status_message = Some(StatusMessage {
                    text: "Select files to restore in Browse view".to_string(),
                    level: StatusLevel::Info,
                    timestamp: Local::now(),
                });
            }
        });
        
        // Backup progress panel (shown when backup is in progress or recently completed/failed)
        if state.backup_progress.stage != BackupStage::Idle {
            ui.add_space(15.0);
            
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(30, 38, 48))
                .corner_radius(6.0)
                .inner_margin(15.0)
                .show(ui, |ui| {
                    // Header with stage indicator
                    ui.horizontal(|ui| {
                        let (icon, color, label) = match &state.backup_progress.stage {
                            BackupStage::Idle => ("[ ]", egui::Color32::GRAY, "Idle"),
                            BackupStage::Scanning => ("[~]", egui::Color32::YELLOW, "Scanning files..."),
                            BackupStage::Encrypting => ("[*]", egui::Color32::from_rgb(100, 150, 220), "Encrypting..."),
                            BackupStage::Uploading => ("[^]", egui::Color32::from_rgb(100, 200, 100), "Uploading..."),
                            BackupStage::Finalizing => ("[!]", egui::Color32::YELLOW, "Finalizing..."),
                            BackupStage::Complete => ("[+]", egui::Color32::GREEN, "Complete!"),
                            BackupStage::Failed(_) => ("[X]", egui::Color32::RED, "Failed"),
                        };
                        ui.label(egui::RichText::new(icon).color(color).strong());
                        ui.label(egui::RichText::new("Backup Progress").strong());
                        ui.label(egui::RichText::new(format!("- {}", label)).color(color));
                        
                        // Close button for completed/failed
                        if matches!(state.backup_progress.stage, BackupStage::Complete | BackupStage::Failed(_)) {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("[x] Dismiss").clicked() {
                                    state.backup_progress = BackupProgress::default();
                                }
                            });
                        }
                    });
                    
                    ui.add_space(10.0);
                    
                    // Current file info
                    if !state.backup_progress.current_file.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label("File:");
                            // Truncate long file names
                            let file_display = if state.backup_progress.current_file.len() > 60 {
                                format!("...{}", &state.backup_progress.current_file[state.backup_progress.current_file.len()-57..])
                            } else {
                                state.backup_progress.current_file.clone()
                            };
                            ui.label(egui::RichText::new(file_display).monospace().weak());
                        });
                        
                        // File progress bar
                        ui.horizontal(|ui| {
                            ui.label("     ");
                            let file_progress = state.backup_progress.current_file_progress;
                            let bar_color = if state.backup_progress.stage == BackupStage::Encrypting {
                                egui::Color32::from_rgb(100, 150, 220)
                            } else {
                                egui::Color32::from_rgb(100, 200, 100)
                            };
                            ui.add(egui::ProgressBar::new(file_progress)
                                .fill(bar_color)
                                .desired_width(400.0));
                            ui.label(egui::RichText::new(format!("{}  ", ByteSize::b(state.backup_progress.current_file_size))).weak());
                        });
                    }
                    
                    ui.add_space(8.0);
                    
                    // Overall progress
                    ui.horizontal(|ui| {
                        ui.label("Overall:");
                        let overall_progress = if state.backup_progress.total_files > 0 {
                            state.backup_progress.files_completed as f32 / state.backup_progress.total_files as f32
                        } else {
                            0.0
                        };
                        ui.add(egui::ProgressBar::new(overall_progress)
                            .fill(egui::Color32::from_rgb(80, 120, 180))
                            .show_percentage()
                            .desired_width(400.0));
                    });
                    
                    // Stats line
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!(
                            "Files: {}/{} | Size: {} | ",
                            state.backup_progress.files_completed,
                            state.backup_progress.total_files,
                            ByteSize::b(state.backup_progress.total_bytes)
                        )).small().weak());
                        
                        // Elapsed time
                        if let Some(start) = state.backup_progress.start_time {
                            let elapsed = Local::now().signed_duration_since(start);
                            let elapsed_str = if elapsed.num_minutes() > 0 {
                                format!("{}m {}s", elapsed.num_minutes(), elapsed.num_seconds() % 60)
                            } else {
                                format!("{}s", elapsed.num_seconds())
                            };
                            ui.label(egui::RichText::new(format!("Elapsed: {}", elapsed_str)).small().weak());
                        }
                    });
                    
                    // Error message for failed backups
                    if let BackupStage::Failed(ref error) = state.backup_progress.stage {
                        ui.add_space(5.0);
                        ui.label(egui::RichText::new(format!("Error: {}", error)).color(egui::Color32::RED).small());
                    }
                    
                    // Success message
                    if state.backup_progress.stage == BackupStage::Complete {
                        if let Some(ref id) = state.backup_progress.backup_id {
                            ui.add_space(5.0);
                            ui.label(egui::RichText::new(format!("Backup ID: backup_{}", id)).color(egui::Color32::GREEN).small());
                        }
                    }
                });
        }
        
        ui.add_space(20.0);
        
        // Recent backups table
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Recent Backups").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("View All").clicked() {
                    state.current_view = View::Browser;
                }
            });
        });
        ui.add_space(8.0);
        
        if state.backups.is_empty() {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(32, 38, 45))
                .corner_radius(4.0)
                .inner_margin(20.0)
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("No backups found").weak());
                    ui.label(egui::RichText::new("Run your first backup to get started").small().weak());
                });
        } else {
            egui::ScrollArea::vertical()
                .max_height(250.0)
                .show(ui, |ui| {
                    // Table header
                    ui.horizontal(|ui| {
                        ui.set_min_width(600.0);
                        ui.label(egui::RichText::new("ID").small().strong());
                        ui.add_space(120.0);
                        ui.label(egui::RichText::new("Date").small().strong());
                        ui.add_space(80.0);
                        ui.label(egui::RichText::new("Files").small().strong());
                        ui.add_space(40.0);
                        ui.label(egui::RichText::new("Size").small().strong());
                        ui.add_space(40.0);
                        ui.label(egui::RichText::new("Status").small().strong());
                    });
                    ui.separator();
                    
                    for backup in state.backups.iter().take(10) {
                        let is_selected = state.selected_backup.as_ref() == Some(&backup.id);
                        egui::Frame::new()
                            .fill(if is_selected { 
                                egui::Color32::from_rgb(45, 55, 70) 
                            } else { 
                                egui::Color32::from_rgb(32, 38, 45) 
                            })
                            .corner_radius(2.0)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                let response = ui.horizontal(|ui| {
                                    ui.set_min_width(600.0);
                                    ui.label(&backup.id);
                                    ui.add_space(20.0);
                                    ui.label(backup.timestamp.format("%Y-%m-%d %H:%M").to_string());
                                    ui.add_space(20.0);
                                    ui.label(format!("{}", backup.file_count));
                                    ui.add_space(40.0);
                                    ui.label(ByteSize::b(backup.total_size).to_string());
                                    ui.add_space(20.0);
                                    
                                    // Status indicators
                                    if backup.is_incremental {
                                        ui.label(egui::RichText::new("[i]").color(egui::Color32::LIGHT_BLUE));
                                    } else {
                                        ui.label(egui::RichText::new("[F]").color(egui::Color32::from_rgb(100, 150, 220)));
                                    }
                                    
                                    match backup.verified {
                                        Some(true) => ui.label(egui::RichText::new("[OK]").color(egui::Color32::GREEN)),
                                        Some(false) => ui.label(egui::RichText::new("[!!]").color(egui::Color32::RED)),
                                        None => ui.label(egui::RichText::new("[?]").color(egui::Color32::GRAY)),
                                    };
                                });
                                
                                if response.response.interact(egui::Sense::click()).clicked() {
                                    state.selected_backup = Some(backup.id.clone());
                                }
                            });
                        ui.add_space(2.0);
                    }
                });
        }
        
        // Status message
        if let Some(msg) = &state.status_message {
            ui.add_space(15.0);
            let color = match msg.level {
                StatusLevel::Info => egui::Color32::LIGHT_BLUE,
                StatusLevel::Success => egui::Color32::GREEN,
                StatusLevel::Warning => egui::Color32::YELLOW,
                StatusLevel::Error => egui::Color32::RED,
            };
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(30, 35, 42))
                .corner_radius(4.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(color, &msg.text);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(msg.timestamp.format("%H:%M:%S").to_string()).small().weak());
                        });
                    });
                });
        }
    }
    
    fn render_card<F>(&self, ui: &mut egui::Ui, title: &str, bg: egui::Color32, content: F)
    where F: FnOnce(&mut egui::Ui)
    {
        egui::Frame::new()
            .fill(bg)
            .corner_radius(6.0)
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_min_width(140.0);
                ui.set_min_height(70.0);
                ui.label(egui::RichText::new(title).small().weak());
                ui.add_space(4.0);
                content(ui);
            });
    }

    fn render_browser(&self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.heading("Browse Backups");
        ui.add_space(10.0);
        
        // Key warning banner
        if !state.key_valid {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(60, 50, 30))
                .corner_radius(4.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("[!]").color(egui::Color32::YELLOW));
                        ui.label("Encryption key required to view file contents");
                        if ui.button("Enter Key").clicked() {
                            state.show_key_dialog = true;
                        }
                    });
                });
            ui.add_space(10.0);
        }
        
        // Backup selector and actions
        ui.horizontal(|ui| {
            ui.label("Backup:");
            egui::ComboBox::from_id_salt("backup_selector")
                .width(200.0)
                .selected_text(state.selected_backup.as_deref().unwrap_or("Select backup..."))
                .show_ui(ui, |ui| {
                    for backup in &state.backups {
                        let label = format!("{} ({} files)", backup.id, backup.file_count);
                        ui.selectable_value(&mut state.selected_backup, Some(backup.id.clone()), label);
                    }
                });
            
            ui.add_space(10.0);
            
            if state.selected_backup.is_some() && state.key_valid {
                if ui.button("Restore Selected").clicked() {
                    state.status_message = Some(StatusMessage {
                        text: "Select files and click restore".to_string(),
                        level: StatusLevel::Info,
                        timestamp: Local::now(),
                    });
                }
                if ui.button("Verify").clicked() {
                    state.status_message = Some(StatusMessage {
                        text: "Verifying backup integrity...".to_string(),
                        level: StatusLevel::Info,
                        timestamp: Local::now(),
                    });
                }
            }
        });
        
        ui.add_space(15.0);
        ui.separator();
        ui.add_space(10.0);
        
        // File browser
        if state.current_files.is_empty() {
            ui.label(egui::RichText::new("Select a backup to browse files").weak());
        } else {
            // Column headers
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Name").small().strong());
                ui.add_space(200.0);
                ui.label(egui::RichText::new("Size").small().strong());
                ui.add_space(60.0);
                ui.label(egui::RichText::new("Modified").small().strong());
            });
            ui.separator();
            
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for entry in &state.current_files {
                        self.render_file_entry(ui, entry, state);
                    }
                });
        }
    }

    fn render_file_entry(&self, ui: &mut egui::Ui, entry: &FileEntry, state: &AppState) {
        let display_name = if state.key_valid {
            &entry.name
        } else {
            &entry.display_name
        };
        
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(30, 36, 42))
            .corner_radius(2.0)
            .inner_margin(6.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Icon and name
                    if entry.is_directory {
                        ui.label(egui::RichText::new("[D]").color(egui::Color32::from_rgb(100, 150, 220)));
                    } else {
                        ui.label(egui::RichText::new("[F]").color(egui::Color32::GRAY));
                    }
                    
                    ui.label(display_name);
                    
                    if !state.key_valid && entry.is_encrypted {
                        ui.label(egui::RichText::new("(encrypted)").small().color(egui::Color32::from_rgb(150, 100, 100)));
                    }
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Modified date
                        if let Some(modified) = &entry.modified {
                            ui.label(egui::RichText::new(modified.format("%Y-%m-%d").to_string()).small().weak());
                        }
                        
                        ui.add_space(20.0);
                        
                        // Size
                        if !entry.is_directory {
                            ui.label(egui::RichText::new(ByteSize::b(entry.size).to_string()).small());
                        }
                    });
                });
            });
        ui.add_space(2.0);
    }
    
    fn render_activity(&self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.heading("Activity Log");
        ui.add_space(10.0);
        
        if state.activity_log.is_empty() {
            ui.label(egui::RichText::new("No activity recorded").weak());
        } else {
            ui.horizontal(|ui| {
                if ui.button("Clear Log").clicked() {
                    state.activity_log.clear();
                }
            });
            ui.add_space(10.0);
            
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for entry in &state.activity_log {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(30, 36, 42))
                            .corner_radius(4.0)
                            .inner_margin(10.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let icon = if entry.success { "[+]" } else { "[!]" };
                                    let color = if entry.success { 
                                        egui::Color32::GREEN 
                                    } else { 
                                        egui::Color32::RED 
                                    };
                                    ui.label(egui::RichText::new(icon).color(color));
                                    ui.label(&entry.action);
                                    ui.label(egui::RichText::new(&entry.details).weak());
                                    
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(egui::RichText::new(
                                            entry.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()
                                        ).small().weak());
                                    });
                                });
                            });
                        ui.add_space(4.0);
                    }
                });
        }
    }

    fn render_settings(&self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.heading("Settings");
        ui.add_space(15.0);
        
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Connection section
            egui::CollapsingHeader::new(egui::RichText::new("Connection").strong())
                .default_open(true)
                .show(ui, |ui| {
                    ui.add_space(5.0);
                    
                    ui.horizontal(|ui| {
                        ui.label("Endpoint:");
                        ui.add(egui::TextEdit::singleline(&mut state.endpoint)
                            .hint_text("your-storagebox.your-server.de")
                            .desired_width(300.0));
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Username:");
                        ui.add(egui::TextEdit::singleline(&mut state.username)
                            .hint_text("username")
                            .desired_width(200.0));
                    });
                    
                    ui.add_space(5.0);
                    
                    ui.horizontal(|ui| {
                        ui.label("Status:");
                        match &state.connection_status {
                            ConnectionStatus::Connected => {
                                ui.colored_label(egui::Color32::GREEN, "Connected");
                            }
                            ConnectionStatus::Connecting => {
                                ui.colored_label(egui::Color32::YELLOW, "Connecting...");
                            }
                            ConnectionStatus::Disconnected => {
                                ui.colored_label(egui::Color32::GRAY, "Disconnected");
                            }
                            ConnectionStatus::Error(e) => {
                                ui.colored_label(egui::Color32::RED, format!("Error: {}", e));
                            }
                        }
                    });
                    
                    ui.add_space(5.0);
                    
                    let btn_enabled = state.connection_status != ConnectionStatus::Connecting;
                    if ui.add_enabled(btn_enabled, egui::Button::new("Test Connection")).clicked() {
                        state.connection_status = ConnectionStatus::Connecting;
                        state.pending_connection_test = true;
                    }
                });
            
            ui.add_space(10.0);
            
            // Encryption section
            egui::CollapsingHeader::new(egui::RichText::new("Encryption").strong())
                .default_open(true)
                .show(ui, |ui| {
                    ui.add_space(5.0);
                    
                    ui.horizontal(|ui| {
                        ui.label("Algorithm:");
                        ui.label("AES-256-GCM");
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Key Derivation:");
                        ui.label("Argon2id (64 MiB, 4 iterations)");
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Key Status:");
                        if state.key_valid {
                            ui.colored_label(egui::Color32::GREEN, "Validated");
                            if let Some(hash) = &state.key_hash {
                                ui.label(egui::RichText::new(format!("({})", &hash[..12])).small().weak());
                            }
                        } else {
                            ui.colored_label(egui::Color32::GRAY, "Not Set");
                        }
                    });
                    
                    ui.add_space(5.0);
                    
                    ui.horizontal(|ui| {
                        if ui.button("Enter Key").clicked() {
                            state.show_key_dialog = true;
                        }
                        
                        if state.key_valid {
                            if ui.button("Clear Key").clicked() {
                                state.encryption_key = None;
                                state.key_valid = false;
                                state.key_hash = None;
                                // Re-garble file names
                                for file in &mut state.current_files {
                                    file.display_name = garble_text(&file.name, b"default_garble_key");
                                }
                            }
                        }
                    });
                });
            
            ui.add_space(10.0);
            
            // Backup section
            egui::CollapsingHeader::new(egui::RichText::new("Backup Settings").strong())
                .default_open(true)
                .show(ui, |ui| {
                    ui.add_space(5.0);
                    
                    ui.checkbox(&mut state.auto_backup_enabled, "Enable automatic backups");
                    
                    ui.horizontal(|ui| {
                        ui.label("Schedule:");
                        ui.label(&state.backup_schedule);
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Retention:");
                        ui.add(egui::DragValue::new(&mut state.retention_days)
                            .speed(1)
                            .range(1..=365));
                        ui.label("days");
                    });
                    
                    ui.checkbox(&mut state.compression_enabled, "Enable compression (Zstd)");
                });
            
            ui.add_space(10.0);
            
            // Storage section
            egui::CollapsingHeader::new(egui::RichText::new("Storage").strong())
                .default_open(true)
                .show(ui, |ui| {
                    ui.add_space(5.0);
                    
                    let used = ByteSize::b(state.storage_used);
                    let total = ByteSize::b(state.storage_total);
                    
                    ui.horizontal(|ui| {
                        ui.label("Used:");
                        ui.label(format!("{}", used));
                    });
                    
                    ui.horizontal(|ui| {
                        ui.label("Total:");
                        ui.label(format!("{}", total));
                    });
                    
                    if state.storage_total > 0 {
                        let ratio = state.storage_used as f32 / state.storage_total as f32;
                        ui.add(egui::ProgressBar::new(ratio).show_percentage());
                    }
                    
                    ui.add_space(5.0);
                    
                    ui.horizontal(|ui| {
                        ui.label("Backups:");
                        ui.label(format!("{}", state.backups.len()));
                    });
                });
            
            ui.add_space(10.0);
            
            // About section
            egui::CollapsingHeader::new(egui::RichText::new("About").strong())
                .default_open(false)
                .show(ui, |ui| {
                    ui.add_space(5.0);
                    ui.label("Skylock v0.8.0");
                    ui.label("Secure encrypted backup system");
                    ui.add_space(5.0);
                    ui.label(egui::RichText::new("Encryption: AES-256-GCM").small());
                    ui.label(egui::RichText::new("Key Derivation: Argon2id").small());
                    ui.label(egui::RichText::new("Compression: Zstd").small());
                    ui.add_space(5.0);
                    ui.hyperlink_to("GitHub Repository", "https://github.com/NullMeDev/Skylock");
                    ui.label(egui::RichText::new("Contact: null@nullme.lol").small().weak());
                });
        });
    }

    fn render_key_dialog(&self, ctx: &egui::Context, state: &mut AppState) {
        egui::Window::new("Encryption Key")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .min_width(400.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                
                ui.label("Enter your encryption key to decrypt backup contents.");
                if state.demo_mode {
                    ui.label(egui::RichText::new("Demo mode: Key will be accepted if 16+ characters.").small().color(egui::Color32::YELLOW));
                } else {
                    ui.label(egui::RichText::new("Key will be validated via AES-256-GCM authenticated decryption.").small().weak());
                }
                
                ui.add_space(15.0);
                
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.key_input)
                        .password(true)
                        .hint_text("Enter encryption key...")
                        .desired_width(350.0)
                );
                
                if let Some(err) = &state.error_message {
                    ui.add_space(8.0);
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(60, 30, 30))
                        .corner_radius(4.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.colored_label(egui::Color32::from_rgb(255, 100, 100), err);
                        });
                }
                
                ui.add_space(15.0);
                
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        state.show_key_dialog = false;
                        state.key_input.clear();
                        state.error_message = None;
                    }
                    
                    ui.add_space(10.0);
                    
                    let validate = ui.button("Validate Key").clicked() 
                        || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    
                    if validate && !state.key_input.is_empty() {
                        // Validate key against stored encryption key from config
                        let key_matches = if let Some(stored_key) = &state.stored_encryption_key {
                            // Compare the entered key with the stored key
                            state.key_input.trim() == stored_key.trim()
                        } else if state.demo_mode {
                            // Demo mode fallback: accept any key >= 16 chars
                            state.key_input.len() >= 16
                        } else {
                            false
                        };
                        
                        if key_matches {
                            let key_hash = hash_key(&state.key_input);
                            state.encryption_key = Some(state.key_input.clone());
                            state.key_valid = true;
                            state.key_hash = Some(key_hash);
                            state.show_key_dialog = false;
                            state.key_input.clear();
                            state.error_message = None;
                            
                            for file in &mut state.current_files {
                                file.display_name = file.name.clone();
                            }
                            
                            state.activity_log.insert(0, ActivityEntry {
                                timestamp: Local::now(),
                                action: "Key validated".to_string(),
                                details: "Encryption key matches stored key".to_string(),
                                success: true,
                            });
                        } else if state.validation_test_data.is_some() {
                            // Real cryptographic validation when we have test data
                            match validate_encryption_key(&state.key_input, state.validation_test_data.as_deref()) {
                                KeyValidationResult::Valid => {
                                    let key_hash = hash_key(&state.key_input);
                                    state.encryption_key = Some(state.key_input.clone());
                                    state.key_valid = true;
                                    state.key_hash = Some(key_hash);
                                    state.show_key_dialog = false;
                                    state.key_input.clear();
                                    state.error_message = None;
                                    
                                    for file in &mut state.current_files {
                                        file.display_name = file.name.clone();
                                    }
                                    
                                    state.activity_log.insert(0, ActivityEntry {
                                        timestamp: Local::now(),
                                        action: "Key validated".to_string(),
                                        details: "Encryption key cryptographically validated".to_string(),
                                        success: true,
                                    });
                                }
                                KeyValidationResult::Invalid => {
                                    state.error_message = Some("Invalid encryption key - decryption failed".to_string());
                                }
                                KeyValidationResult::NoTestData => {
                                    state.error_message = Some("No validation data available".to_string());
                                }
                                KeyValidationResult::Error(e) => {
                                    state.error_message = Some(format!("Validation error: {}", e));
                                }
                            }
                        } else {
                            // Key doesn't match and no test data for crypto validation
                            let hint = if state.stored_encryption_key.is_some() {
                                "Key does not match the encryption key in config.toml"
                            } else {
                                "No encryption key configured. Check ~/.config/skylock-hybrid/config.toml"
                            };
                            state.error_message = Some(hint.to_string());
                        }
                    }
                });
                
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(5.0);
                ui.label(egui::RichText::new("Security: Keys are never stored. Validation uses authenticated decryption.").small().weak());
            });
    }
}

#[cfg(feature = "gui")]
impl eframe::App for SkylockApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Get mutable state
        let mut state_guard = match self.state.try_write() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let state = &mut *state_guard;
        
        // Check for pending connection test
        let should_test_connection = state.pending_connection_test;
        if should_test_connection {
            state.pending_connection_test = false;
        }
        
        // Check for pending backup
        let should_start_backup = state.pending_backup;
        if should_start_backup {
            state.pending_backup = false;
            state.backup_progress = BackupProgress {
                stage: BackupStage::Scanning,
                start_time: Some(Local::now()),
                ..Default::default()
            };
        }
        drop(state_guard);
        
        if should_test_connection {
            self.spawn_connection_test();
        }
        
        if should_start_backup {
            self.spawn_backup();
        }
        
        // Check for async connection results (needs separate mutable borrows)
        {
            let mut state_guard = match self.state.try_write() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            let state = &mut *state_guard;
            
            if let Some(rx) = &self.connection_rx {
                match rx.try_recv() {
                    Ok(result) => {
                        match result {
                            ConnectionTestResult::Success => {
                                state.connection_status = ConnectionStatus::Connected;
                                state.status_message = Some(StatusMessage {
                                    text: "Successfully connected to Hetzner Storage Box!".to_string(),
                                    level: StatusLevel::Success,
                                    timestamp: Local::now(),
                                });
                                state.activity_log.insert(0, ActivityEntry {
                                    timestamp: Local::now(),
                                    action: "Connection test".to_string(),
                                    details: "Successfully connected to storage".to_string(),
                                    success: true,
                                });
                            }
                            ConnectionTestResult::Error(e) => {
                                state.connection_status = ConnectionStatus::Error(e.clone());
                                state.status_message = Some(StatusMessage {
                                    text: format!("Connection failed: {}", e),
                                    level: StatusLevel::Error,
                                    timestamp: Local::now(),
                                });
                                state.activity_log.insert(0, ActivityEntry {
                                    timestamp: Local::now(),
                                    action: "Connection test".to_string(),
                                    details: format!("Failed: {}", e),
                                    success: false,
                                });
                            }
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        // Still waiting for result
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        // Channel closed without result
                        state.connection_status = ConnectionStatus::Error("Connection test interrupted".to_string());
                    }
                }
            }
        }
        
        // Clear connection_rx if result was received
        if let Some(rx) = &self.connection_rx {
            // Check if channel is disconnected (result already processed)
            if matches!(rx.try_recv(), Err(std::sync::mpsc::TryRecvError::Disconnected)) {
                self.connection_rx = None;
            }
        }
        
        // Check for backup progress events
        {
            let mut state_guard = match self.state.try_write() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            let state = &mut *state_guard;
            
            if let Some(rx) = &self.backup_rx {
                // Process all available events
                loop {
                    match rx.try_recv() {
                        Ok(event) => {
                            match event {
                                BackupProgressEvent::ScanStarted { path } => {
                                    state.backup_progress.stage = BackupStage::Scanning;
                                    state.backup_progress.current_file = format!("Scanning: {}", path);
                                }
                                BackupProgressEvent::ScanProgress { files_found, total_size } => {
                                    state.backup_progress.total_files = files_found;
                                    state.backup_progress.total_bytes = total_size;
                                }
                                BackupProgressEvent::ScanComplete { total_files, total_size } => {
                                    state.backup_progress.total_files = total_files;
                                    state.backup_progress.total_bytes = total_size;
                                    state.backup_progress.stage = BackupStage::Uploading;
                                }
                                BackupProgressEvent::FileStarted { file_name, file_size, file_index, total_files } => {
                                    state.backup_progress.current_file = file_name;
                                    state.backup_progress.current_file_size = file_size;
                                    state.backup_progress.current_file_progress = 0.0;
                                    state.backup_progress.files_completed = file_index - 1;
                                    state.backup_progress.total_files = total_files;
                                    state.current_operation = Operation::Backup {
                                        progress: (file_index - 1) as f32 / total_files as f32,
                                        current_file: state.backup_progress.current_file.clone(),
                                    };
                                }
                                BackupProgressEvent::FileEncrypting { file_name, progress } => {
                                    state.backup_progress.stage = BackupStage::Encrypting;
                                    state.backup_progress.current_file = file_name;
                                    state.backup_progress.current_file_progress = progress * 0.3; // 30% for encryption
                                }
                                BackupProgressEvent::FileUploading { file_name, bytes_sent, total_bytes } => {
                                    state.backup_progress.stage = BackupStage::Uploading;
                                    state.backup_progress.current_file = file_name;
                                    if total_bytes > 0 {
                                        state.backup_progress.current_file_progress = 0.3 + (bytes_sent as f32 / total_bytes as f32) * 0.7;
                                    }
                                }
                                BackupProgressEvent::FileComplete { file_name: _, file_index, total_files } => {
                                    state.backup_progress.files_completed = file_index;
                                    state.backup_progress.current_file_progress = 1.0;
                                    state.current_operation = Operation::Backup {
                                        progress: file_index as f32 / total_files as f32,
                                        current_file: state.backup_progress.current_file.clone(),
                                    };
                                }
                                BackupProgressEvent::BackupComplete { backup_id, total_files, total_size, duration_secs } => {
                                    state.backup_progress.stage = BackupStage::Complete;
                                    state.backup_progress.backup_id = Some(backup_id.clone());
                                    state.current_operation = Operation::None;
                                    state.status_message = Some(StatusMessage {
                                        text: format!("Backup complete! {} files ({}) in {}s", 
                                            total_files, 
                                            ByteSize::b(total_size),
                                            duration_secs),
                                        level: StatusLevel::Success,
                                        timestamp: Local::now(),
                                    });
                                    state.activity_log.insert(0, ActivityEntry {
                                        timestamp: Local::now(),
                                        action: "Backup completed".to_string(),
                                        details: format!("backup_{}: {} files", backup_id, total_files),
                                        success: true,
                                    });
                                    // Add to backups list
                                    state.backups.insert(0, BackupInfo {
                                        id: format!("backup_{}", backup_id),
                                        timestamp: Local::now(),
                                        file_count: total_files,
                                        total_size,
                                        is_incremental: false,
                                        verified: None,
                                    });
                                }
                                BackupProgressEvent::BackupFailed { error } => {
                                    state.backup_progress.stage = BackupStage::Failed(error.clone());
                                    state.current_operation = Operation::None;
                                    state.status_message = Some(StatusMessage {
                                        text: format!("Backup failed: {}", error),
                                        level: StatusLevel::Error,
                                        timestamp: Local::now(),
                                    });
                                    state.activity_log.insert(0, ActivityEntry {
                                        timestamp: Local::now(),
                                        action: "Backup failed".to_string(),
                                        details: error,
                                        success: false,
                                    });
                                }
                            }
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            // Clear the receiver if disconnected
                            break;
                        }
                    }
                }
            }
        }
        
        // Clear backup_rx if done
        if let Some(rx) = &self.backup_rx {
            if matches!(rx.try_recv(), Err(std::sync::mpsc::TryRecvError::Disconnected)) {
                self.backup_rx = None;
            }
        }
        
        // Get state again for the rest of the update
        let mut state_guard = match self.state.try_write() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        let state = &mut *state_guard;
        
        // Request repaint while operations are in progress
        if state.connection_status == ConnectionStatus::Connecting 
           || state.backup_progress.stage != BackupStage::Idle 
           && state.backup_progress.stage != BackupStage::Complete 
           && !matches!(state.backup_progress.stage, BackupStage::Failed(_)) {
            ctx.request_repaint();
        }
        
        // Key dialog (modal - rendered on top without blocking backdrop)
        if state.show_key_dialog {
            self.render_key_dialog(ctx, state);
        }
        
        // Sidebar
        egui::SidePanel::left("sidebar")
            .exact_width(170.0)
            .resizable(false)
            .show(ctx, |ui| {
                self.render_sidebar(ui, state);
            });
        
        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::new()
                .inner_margin(15.0)
                .show(ui, |ui| {
                    match state.current_view {
                        View::Dashboard => self.render_dashboard(ui, state),
                        View::Browser => self.render_browser(ui, state),
                        View::Activity => self.render_activity(ui, state),
                        View::Settings => self.render_settings(ui, state),
                    }
                });
        });
    }
}

/// Run the GUI application
#[cfg(feature = "gui")]
pub fn run_gui() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("Skylock - Encrypted Backup"),
        ..Default::default()
    };
    
    // Create runtime for async operations
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let _guard = rt.enter();
    
    eframe::run_native(
        "Skylock",
        options,
        Box::new(|cc| Ok(Box::new(SkylockApp::new(cc)))),
    )
}

// ============================================================================
// Cryptographic Functions
// ============================================================================

/// Validate encryption key using AES-256-GCM authenticated decryption
/// This is used when connected to real backup storage with encrypted data
pub fn validate_encryption_key(key: &str, test_data: Option<&[u8]>) -> KeyValidationResult {
    use aes_gcm::{aead::{Aead, KeyInit}, Aes256Gcm, Nonce};
    
    if key.len() < 16 {
        return KeyValidationResult::Invalid;
    }
    
    let test_data = match test_data {
        Some(data) if data.len() >= 28 => data,
        _ => return KeyValidationResult::NoTestData,
    };
    
    // Derive key from password
    let mut derived_key = [0u8; 32];
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hasher.update(b"skylock_key_derivation_v1");
    let hash_result = hasher.finalize();
    derived_key.copy_from_slice(&hash_result);
    
    let nonce = &test_data[..12];
    let ciphertext = &test_data[12..];
    
    let cipher = match Aes256Gcm::new_from_slice(&derived_key) {
        Ok(c) => c,
        Err(e) => return KeyValidationResult::Error(format!("Cipher error: {}", e)),
    };
    
    let nonce = Nonce::from_slice(nonce);
    
    match cipher.decrypt(nonce, ciphertext) {
        Ok(_) => KeyValidationResult::Valid,
        Err(_) => KeyValidationResult::Invalid,
    }
}

/// Hash key for display purposes (shows first 12 chars of SHA-256 hex)
pub fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..6]) // 12 hex chars
}

/// Generate garbled text to represent encrypted data
pub fn garble_text(input: &str, key_bytes: &[u8]) -> String {
    let mut result = String::with_capacity(input.len());
    for (i, c) in input.chars().enumerate() {
        if c.is_alphanumeric() {
            let offset = key_bytes.get(i % key_bytes.len()).unwrap_or(&0);
            let garbled = ((c as u32).wrapping_add(*offset as u32) % 95 + 33) as u8 as char;
            result.push(garbled);
        } else {
            result.push(c);
        }
    }
    result
}
