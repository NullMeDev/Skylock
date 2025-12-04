//! Skylock GUI Application
//!
//! A minimalistic backup management interface built with egui.

#[cfg(feature = "gui")]
use eframe::egui;

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Local};
use bytesize::ByteSize;

/// Application state
#[derive(Default)]
pub struct AppState {
    /// Currently loaded encryption key (validated)
    pub encryption_key: Option<String>,
    /// Key validation status
    pub key_valid: bool,
    /// Available backups
    pub backups: Vec<BackupInfo>,
    /// Currently selected backup
    pub selected_backup: Option<String>,
    /// Current view
    pub current_view: View,
    /// Status message
    pub status_message: Option<StatusMessage>,
    /// Last backup time
    pub last_backup: Option<DateTime<Local>>,
    /// Storage usage
    pub storage_used: u64,
    /// Storage total
    pub storage_total: u64,
    /// Sync status
    pub sync_active: bool,
    /// Files in current backup view
    pub current_files: Vec<FileEntry>,
    /// Expanded folders in tree view
    pub expanded_folders: std::collections::HashSet<PathBuf>,
    /// Key input field
    pub key_input: String,
    /// Show key input dialog
    pub show_key_dialog: bool,
    /// Error message
    pub error_message: Option<String>,
}

/// Backup information
#[derive(Clone, Debug)]
pub struct BackupInfo {
    pub id: String,
    pub timestamp: DateTime<Local>,
    pub file_count: usize,
    pub total_size: u64,
    pub is_incremental: bool,
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
    /// Display name (decrypted if key valid, garbled if not)
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
    Settings,
}

#[cfg(feature = "gui")]
pub struct SkylockApp {
    pub state: Arc<RwLock<AppState>>,
    runtime: tokio::runtime::Handle,
}

#[cfg(feature = "gui")]
impl SkylockApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Configure custom fonts and visuals
        let mut style = (*cc.egui_ctx.style()).clone();
        
        // Minimalistic dark theme
        style.visuals.dark_mode = true;
        style.visuals.override_text_color = Some(egui::Color32::from_gray(220));
        style.visuals.widgets.noninteractive.bg_fill = egui::Color32::from_gray(30);
        style.visuals.widgets.inactive.bg_fill = egui::Color32::from_gray(40);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_gray(50);
        style.visuals.widgets.active.bg_fill = egui::Color32::from_gray(60);
        style.visuals.window_fill = egui::Color32::from_gray(25);
        style.visuals.panel_fill = egui::Color32::from_gray(28);
        
        // Reduce visual clutter - styling is handled by Frame::new().corner_radius()
        
        cc.egui_ctx.set_style(style);

        let runtime = tokio::runtime::Handle::current();
        
        Self {
            state: Arc::new(RwLock::new(AppState::default())),
            runtime,
        }
    }

    fn render_sidebar(&self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.vertical(|ui| {
            ui.add_space(10.0);
            
            // Logo/Title
            ui.heading("Skylock");
            ui.add_space(20.0);
            
            // Navigation buttons
            if ui.selectable_label(state.current_view == View::Dashboard, "Dashboard").clicked() {
                state.current_view = View::Dashboard;
            }
            
            ui.add_space(5.0);
            
            if ui.selectable_label(state.current_view == View::Browser, "Browse Backups").clicked() {
                state.current_view = View::Browser;
            }
            
            ui.add_space(5.0);
            
            if ui.selectable_label(state.current_view == View::Settings, "Settings").clicked() {
                state.current_view = View::Settings;
            }
            
            ui.add_space(20.0);
            ui.separator();
            ui.add_space(10.0);
            
            // Key status indicator
            if state.key_valid {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("*").color(egui::Color32::GREEN));
                    ui.label("Key Active");
                });
            } else {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("*").color(egui::Color32::GRAY));
                    ui.label("No Key");
                });
                if ui.small_button("Enter Key").clicked() {
                    state.show_key_dialog = true;
                }
            }
            
            ui.add_space(10.0);
            
            // Sync status
            if state.sync_active {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("*").color(egui::Color32::LIGHT_BLUE));
                    ui.label("Syncing...");
                });
            }
        });
    }

    fn render_dashboard(&self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.heading("Dashboard");
        ui.add_space(15.0);
        
        // Status cards
        ui.horizontal(|ui| {
            // Last backup card
            egui::Frame::new()
                .fill(egui::Color32::from_gray(35))
                .corner_radius(4.0)
                .inner_margin(15.0)
                .show(ui, |ui| {
                    ui.set_min_width(180.0);
                    ui.label(egui::RichText::new("Last Backup").small());
                    if let Some(last) = &state.last_backup {
                        ui.label(egui::RichText::new(last.format("%Y-%m-%d %H:%M").to_string()).strong());
                    } else {
                        ui.label(egui::RichText::new("Never").weak());
                    }
                });
            
            ui.add_space(10.0);
            
            // Storage usage card
            egui::Frame::new()
                .fill(egui::Color32::from_gray(35))
                .corner_radius(4.0)
                .inner_margin(15.0)
                .show(ui, |ui| {
                    ui.set_min_width(180.0);
                    ui.label(egui::RichText::new("Storage Used").small());
                    let used = ByteSize::b(state.storage_used);
                    let total = ByteSize::b(state.storage_total);
                    if state.storage_total > 0 {
                        ui.label(egui::RichText::new(format!("{} / {}", used, total)).strong());
                        let ratio = state.storage_used as f32 / state.storage_total as f32;
                        ui.add(egui::ProgressBar::new(ratio).show_percentage());
                    } else {
                        ui.label(egui::RichText::new(format!("{}", used)).strong());
                    }
                });
            
            ui.add_space(10.0);
            
            // Backup count card
            egui::Frame::new()
                .fill(egui::Color32::from_gray(35))
                .corner_radius(4.0)
                .inner_margin(15.0)
                .show(ui, |ui| {
                    ui.set_min_width(180.0);
                    ui.label(egui::RichText::new("Total Backups").small());
                    ui.label(egui::RichText::new(state.backups.len().to_string()).strong());
                });
        });
        
        ui.add_space(25.0);
        
        // Quick actions
        ui.heading("Quick Actions");
        ui.add_space(10.0);
        
        ui.horizontal(|ui| {
            if ui.button("Backup Now").clicked() {
                state.status_message = Some(StatusMessage {
                    text: "Starting backup...".to_string(),
                    level: StatusLevel::Info,
                    timestamp: Local::now(),
                });
            }
            
            if ui.button("Verify Backups").clicked() {
                state.status_message = Some(StatusMessage {
                    text: "Verifying backups...".to_string(),
                    level: StatusLevel::Info,
                    timestamp: Local::now(),
                });
            }
            
            if ui.button("Test Connection").clicked() {
                state.status_message = Some(StatusMessage {
                    text: "Testing connection...".to_string(),
                    level: StatusLevel::Info,
                    timestamp: Local::now(),
                });
            }
        });
        
        ui.add_space(25.0);
        
        // Recent backups list
        ui.heading("Recent Backups");
        ui.add_space(10.0);
        
        if state.backups.is_empty() {
            ui.label(egui::RichText::new("No backups found").weak());
        } else {
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for backup in state.backups.iter().take(10) {
                        egui::Frame::new()
                            .fill(egui::Color32::from_gray(32))
                            .corner_radius(2.0)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(&backup.id);
                                    ui.separator();
                                    ui.label(backup.timestamp.format("%Y-%m-%d %H:%M").to_string());
                                    ui.separator();
                                    ui.label(format!("{} files", backup.file_count));
                                    ui.separator();
                                    ui.label(ByteSize::b(backup.total_size).to_string());
                                    if backup.is_incremental {
                                        ui.label(egui::RichText::new("(incremental)").small().weak());
                                    }
                                });
                            });
                        ui.add_space(4.0);
                    }
                });
        }
        
        // Status message
        if let Some(msg) = &state.status_message {
            ui.add_space(20.0);
            ui.separator();
            ui.add_space(10.0);
            
            let color = match msg.level {
                StatusLevel::Info => egui::Color32::LIGHT_BLUE,
                StatusLevel::Success => egui::Color32::GREEN,
                StatusLevel::Warning => egui::Color32::YELLOW,
                StatusLevel::Error => egui::Color32::RED,
            };
            
            ui.colored_label(color, &msg.text);
        }
    }

    fn render_browser(&self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.heading("Browse Backups");
        ui.add_space(10.0);
        
        if !state.key_valid {
            ui.label(egui::RichText::new("Enter encryption key to view file contents").weak());
            ui.add_space(10.0);
            if ui.button("Enter Key").clicked() {
                state.show_key_dialog = true;
            }
            ui.add_space(20.0);
            ui.separator();
            ui.add_space(10.0);
            ui.label("Without a valid key, file names appear scrambled:");
            ui.add_space(5.0);
            
            // Show garbled preview
            for entry in &state.current_files {
                let display = if state.key_valid {
                    &entry.name
                } else {
                    &entry.display_name
                };
                
                ui.horizontal(|ui| {
                    if entry.is_directory {
                        ui.label("[DIR]");
                    } else {
                        ui.label("[FILE]");
                    }
                    ui.label(display);
                    if !entry.is_directory {
                        ui.label(egui::RichText::new(ByteSize::b(entry.size).to_string()).weak());
                    }
                });
            }
            return;
        }
        
        // Backup selector
        ui.horizontal(|ui| {
            ui.label("Select Backup:");
            egui::ComboBox::from_id_salt("backup_selector")
                .selected_text(state.selected_backup.as_deref().unwrap_or("Select..."))
                .show_ui(ui, |ui| {
                    for backup in &state.backups {
                        ui.selectable_value(
                            &mut state.selected_backup,
                            Some(backup.id.clone()),
                            &backup.id,
                        );
                    }
                });
        });
        
        ui.add_space(15.0);
        
        // File tree
        if state.selected_backup.is_some() {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for entry in &state.current_files {
                        self.render_file_entry(ui, entry, state);
                    }
                });
        } else {
            ui.label(egui::RichText::new("Select a backup to browse").weak());
        }
    }

    fn render_file_entry(&self, ui: &mut egui::Ui, entry: &FileEntry, _state: &AppState) {
        ui.horizontal(|ui| {
            let indent = entry.path.components().count().saturating_sub(1) * 20;
            ui.add_space(indent as f32);
            
            if entry.is_directory {
                ui.label("[+]");
            } else {
                ui.label("   ");
            }
            
            ui.label(&entry.display_name);
            
            if !entry.is_directory {
                ui.label(egui::RichText::new(ByteSize::b(entry.size).to_string()).weak());
                
                if entry.is_encrypted {
                    ui.label(egui::RichText::new("[encrypted]").small().weak());
                }
            }
        });
    }

    fn render_settings(&self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.heading("Settings");
        ui.add_space(15.0);
        
        // Encryption key section
        egui::CollapsingHeader::new("Encryption")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Key Status:");
                    if state.key_valid {
                        ui.colored_label(egui::Color32::GREEN, "Active");
                    } else {
                        ui.colored_label(egui::Color32::GRAY, "Not Set");
                    }
                });
                
                ui.add_space(5.0);
                
                if ui.button("Change Key").clicked() {
                    state.show_key_dialog = true;
                }
                
                if state.key_valid {
                    if ui.button("Clear Key").clicked() {
                        state.encryption_key = None;
                        state.key_valid = false;
                    }
                }
            });
        
        ui.add_space(15.0);
        
        // Storage section
        egui::CollapsingHeader::new("Storage")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(format!("Used: {}", ByteSize::b(state.storage_used)));
                ui.label(format!("Total: {}", ByteSize::b(state.storage_total)));
            });
        
        ui.add_space(15.0);
        
        // About section
        egui::CollapsingHeader::new("About")
            .default_open(false)
            .show(ui, |ui| {
                ui.label("Skylock v0.8.0");
                ui.label("Secure encrypted backup system");
                ui.add_space(5.0);
                ui.hyperlink_to("GitHub", "https://github.com/NullMeDev/Skylock");
            });
    }

    fn render_key_dialog(&self, ctx: &egui::Context, state: &mut AppState) {
        egui::Window::new("Enter Encryption Key")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.label("Enter your encryption key to decrypt backup contents:");
                ui.add_space(10.0);
                
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.key_input)
                        .password(true)
                        .hint_text("Encryption key...")
                        .desired_width(300.0)
                );
                
                if let Some(err) = &state.error_message {
                    ui.add_space(5.0);
                    ui.colored_label(egui::Color32::RED, err);
                }
                
                ui.add_space(15.0);
                
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        state.show_key_dialog = false;
                        state.key_input.clear();
                        state.error_message = None;
                    }
                    
                    let validate = ui.button("Validate").clicked() 
                        || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    
                    if validate {
                        if state.key_input.len() >= 16 {
                            state.encryption_key = Some(state.key_input.clone());
                            state.key_valid = true;
                            state.show_key_dialog = false;
                            state.key_input.clear();
                            state.error_message = None;
                            
                            // Update file display names
                            for file in &mut state.current_files {
                                file.display_name = file.name.clone();
                            }
                        } else {
                            state.error_message = Some("Key must be at least 16 characters".to_string());
                        }
                    }
                });
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
        
        // Key dialog (modal)
        if state.show_key_dialog {
            self.render_key_dialog(ctx, state);
        }
        
        // Sidebar
        egui::SidePanel::left("sidebar")
            .exact_width(150.0)
            .resizable(false)
            .show(ctx, |ui| {
                self.render_sidebar(ui, state);
            });
        
        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            match state.current_view {
                View::Dashboard => self.render_dashboard(ui, state),
                View::Browser => self.render_browser(ui, state),
                View::Settings => self.render_settings(ui, state),
            }
        });
    }
}

/// Run the GUI application
#[cfg(feature = "gui")]
pub fn run_gui() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 600.0])
            .with_min_inner_size([600.0, 400.0])
            .with_title("Skylock"),
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
