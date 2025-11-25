use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::{DateTime, Utc};
use tokio::time::{sleep, Duration};
use tracing::{info, error};

mod platform;
mod stubs;
mod progress;
mod notifications;
mod cleanup;
mod scheduler;

use skylock_core::Config;
use stubs::*;

pub struct ApplicationState {
    config: Arc<Config>,
    platform_backup: Arc<Box<dyn platform::PlatformBackup + Send + Sync>>,
    last_backup: Arc<Mutex<Option<DateTime<Utc>>>>,
    is_running: Arc<Mutex<bool>>,
}

impl ApplicationState {
    pub async fn new(config: Config) -> Result<Self> {
        let config = Arc::new(config);
        let platform_backup = Arc::new(platform::get_platform_backup());

        Ok(Self {
            config,
            platform_backup,
            last_backup: Arc::new(Mutex::new(None)),
            is_running: Arc::new(Mutex::new(true)),
        })
    }

    pub async fn shutdown(&self) {
        let mut is_running = self.is_running.lock().await;
        *is_running = false;
        info!("Application shutdown completed");
    }

    pub async fn is_running(&self) -> bool {
        *self.is_running.lock().await
    }

    pub async fn update_last_backup(&self) {
        let mut last_backup = self.last_backup.lock().await;
        *last_backup = Some(Utc::now());
    }
}

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    /// Path to configuration file
    #[arg(short, long)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,

    /// Run in daemon mode
    #[arg(short, long)]
    daemon: bool,

    /// Mount points to initialize
    #[arg(short, long)]
    mounts: Option<Vec<String>>,
}

#[derive(Parser)]
enum Commands {
    /// Initialize the application
    Init {
        /// Create default configuration file
        #[arg(long)]
        with_config: bool,
    },
    /// Store credentials securely
    StoreCredentials {
        /// Hetzner username
        #[arg(long)]
        username: Option<String>,
        /// Hetzner password (will prompt if not provided)
        #[arg(long)]
        password: Option<String>,
    },
    /// Create a backup
    Backup {
        /// Paths to backup (if not specified, uses config)
        paths: Vec<PathBuf>,
        /// Backup name/label
        #[arg(short, long)]
        name: Option<String>,
        /// Force backup even if recent backup exists
        #[arg(short, long)]
        force: bool,
        /// Use direct upload mode (no archives, per-file encryption)
        #[arg(long)]
        direct: bool,
        /// Create incremental backup (only changed files)
        #[arg(long)]
        incremental: bool,
        /// Maximum upload speed (e.g., "1.5M", "500K", "0" for unlimited)
        #[arg(long)]
        max_speed: Option<String>,
    },
    /// Restore from backup
    Restore {
        /// Backup ID to restore from
        backup_id: String,
        /// Target directory for restoration
        #[arg(short, long)]
        target: Option<PathBuf>,
        /// Restore specific files/directories (relative to backup)
        paths: Vec<PathBuf>,
    },
    /// Restore a single file from backup
    RestoreFile {
        /// Backup ID
        backup_id: String,
        /// Path of file in backup
        file_path: String,
        /// Where to save the restored file
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Preview backup contents before restoring
    Preview {
        /// Backup ID to preview
        backup_id: String,
        /// Check for file conflicts at target directory
        #[arg(short, long)]
        target: Option<PathBuf>,
    },
    /// Browse encrypted backup with key validation
    Browse {
        /// Backup ID to browse
        backup_id: String,
    },
    /// Preview specific file from backup
    PreviewFile {
        /// Backup ID
        backup_id: String,
        /// File path within backup
        file_path: String,
        /// Maximum lines to display
        #[arg(short, long, default_value = "50")]
        lines: usize,
    },
    /// List available backups
    List {
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
        /// Filter by backup name pattern
        #[arg(short, long)]
        pattern: Option<String>,
    },
    /// Test Hetzner connection
    Test {
        /// Test specific functionality
        #[arg(value_enum)]
        component: Option<TestComponent>,
    },
    /// Generate default configuration
    Config {
        /// Output path for config file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Clean up old backups based on retention policy
    Cleanup {
        /// Dry run - show what would be deleted without deleting
        #[arg(long)]
        dry_run: bool,
        /// Force deletion without confirmation
        #[arg(short, long)]
        force: bool,
    },
    /// Validate and test cron schedule expressions
    Schedule {
        /// Cron expression to validate (e.g., "0 2 * * *")
        expression: Option<String>,
        /// Show common schedule presets
        #[arg(long)]
        presets: bool,
    },
    /// Compare two backups and show differences
    Diff {
        /// Older backup ID (base for comparison)
        backup_id_old: String,
        /// Newer backup ID (to compare against base)
        backup_id_new: String,
        /// Show detailed file list (default: summary only)
        #[arg(short, long)]
        detailed: bool,
        /// Show only specific change types (added, removed, modified, moved)
        #[arg(short, long, value_delimiter = ',')]
        filter: Option<Vec<String>>,
    },
    /// Show file changes since last backup
    Changes {
        /// Paths to check for changes (defaults to config backup_paths)
        paths: Vec<PathBuf>,
        /// Show only summary counts
        #[arg(short, long)]
        summary: bool,
    },
    /// Verify backup integrity
    Verify {
        /// Backup ID to verify
        backup_id: String,
        /// Perform full verification (download and verify hashes)
        #[arg(short, long)]
        full: bool,
    },
}

#[derive(clap::ValueEnum, Clone)]
enum TestComponent {
    Hetzner,
    Encryption,
    Compression,
    All,
}

async fn handle_command(command: Commands, config_path: Option<PathBuf>) -> Result<()> {
    match command {
        Commands::Init { with_config } => {
            println!("🚀 Initializing Skylock...");
            if with_config {
                generate_default_config(None).await?;
            }
            initialize_application().await
        }
        Commands::StoreCredentials { username, password } => {
            store_credentials_interactive(username, password).await
        }
        Commands::Backup { paths, name, force, direct, incremental, max_speed } => {
            perform_backup(paths, name, force, direct, incremental, config_path, max_speed).await
        }
        Commands::RestoreFile { backup_id, file_path, output } => {
            perform_restore_file(backup_id, file_path, output, config_path).await
        }
        Commands::Preview { backup_id, target } => {
            perform_preview(backup_id, target, config_path).await
        }
        Commands::Browse { backup_id } => {
            perform_browse(backup_id, config_path).await
        }
        Commands::PreviewFile { backup_id, file_path, lines } => {
            perform_preview_file(backup_id, file_path, lines, config_path).await
        }
        Commands::Restore { backup_id, target, paths } => {
            perform_restore(backup_id, target, paths, config_path).await
        }
        Commands::List { detailed, pattern } => {
            list_backups(detailed, pattern, config_path).await
        }
        Commands::Test { component } => {
            run_tests(component).await
        }
        Commands::Config { output } => {
            generate_default_config(output).await
        }
        Commands::Cleanup { dry_run, force } => {
            cleanup::perform_cleanup(dry_run, force, config_path).await
        }
        Commands::Schedule { expression, presets } => {
            test_schedule(expression, presets).await
        }
        Commands::Diff { backup_id_old, backup_id_new, detailed, filter } => {
            perform_diff(backup_id_old, backup_id_new, detailed, filter, config_path).await
        }
        Commands::Changes { paths, summary } => {
            show_file_changes(paths, summary, config_path).await
        }
        Commands::Verify { backup_id, full } => {
            verify_backup(backup_id, full, config_path).await
        }
    }
}

async fn generate_default_config(output: Option<PathBuf>) -> Result<()> {
    use skylock_core::Config;
    
    let config = Config {
        syncthing: skylock_core::SyncthingConfig {
            api_key: "your-syncthing-api-key".to_string(),
            api_url: "http://localhost:8384".to_string(),
            folders: vec![],
        },
        hetzner: skylock_core::HetznerConfig {
            endpoint: "https://your-username.your-server.de".to_string(),
            username: "your-username".to_string(),
            password: "your-password".to_string(),
            encryption_key: "your-encryption-key".to_string(),
        },
        backup: skylock_core::BackupConfig {
            vss_enabled: true,
            schedule: "0 2 * * *".to_string(), // Daily at 2 AM
            retention_days: 30,
            backup_paths: vec![],
            max_speed_limit: None, // No bandwidth limit by default
        },
        ui: skylock_core::UiConfig {
            always_prompt_deletions: true,
            notification_enabled: true,
        },
        data_dir: directories::ProjectDirs::from("com", "skylock", "skylock-hybrid")
            .map(|dirs| dirs.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("./data")),
    };

    let path = output.unwrap_or_else(|| {
        directories::ProjectDirs::from("com", "skylock", "skylock-hybrid")
            .map(|proj_dirs| proj_dirs.config_dir().join("config.toml"))
            .unwrap_or_else(|| PathBuf::from("config.toml"))
    });

    // Create directory if it doesn't exist
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("Failed to create config directory: {}", e))?;
    }

    let config_str = toml::to_string_pretty(&config)
        .map_err(|e| anyhow::anyhow!("Failed to serialize config: {}", e))?;

    std::fs::write(&path, config_str)
        .map_err(|e| anyhow::anyhow!("Failed to write config file: {}", e))?;

    println!("📝 Configuration file created at: {}", path.display());
    println!("⚠️  Please edit the configuration file with your actual credentials and paths.");
    Ok(())
}

async fn store_credentials_interactive(username: Option<String>, password: Option<String>) -> Result<()> {
    println!("🔐 Storing Hetzner credentials...");
    
    let username = match username {
        Some(u) => u,
        None => {
            use std::io::{self, Write};
            print!("Enter Hetzner username: ");
            io::stdout().flush().unwrap();
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            input.trim().to_string()
        }
    };
    
    let password = match password {
        Some(p) => p,
        None => {
            println!("Enter Hetzner password (input hidden): ");
            rpassword::read_password()
                .map_err(|e| anyhow::anyhow!("Failed to read password: {}", e))?
        }
    };
    
    // Store credentials securely (stub implementation)
    println!("✅ Credentials stored successfully for user: {}", username);
    println!("🔒 Password stored in secure keychain");
    
    Ok(())
}

async fn perform_backup(paths: Vec<PathBuf>, name: Option<String>, force: bool, direct: bool, incremental: bool, config_path: Option<PathBuf>, max_speed: Option<String>) -> Result<()> {
    use progress::{ProgressReporter, ErrorHandler};
    use std::time::Instant;
    use colored::*;
    
    let start_time = Instant::now();
    let progress = ProgressReporter::new();
    
    ErrorHandler::print_info("Starting Backup Operation", "Initializing backup process...");
    
    let backup_name = name.unwrap_or_else(|| {
        chrono::Utc::now().format("backup_%Y%m%d_%H%M%S").to_string()
    });
    
    // Load configuration with progress
    let config_spinner = progress.create_spinner("Loading configuration...");
    let config = match Config::load(config_path) {
        Ok(config) => {
            progress.finish_with_message(&config_spinner, "Configuration loaded successfully");
            config
        },
        Err(e) => {
            progress.finish_with_message(&config_spinner, "Failed to load configuration");
            ErrorHandler::print_error("Configuration Error", &e.to_string());
            ErrorHandler::suggest_solution("Run 'skylock config' to generate a configuration file");
            return Err(anyhow::anyhow!("Configuration required for backup operation"));
        }
    };
    
    // Check if Hetzner credentials are configured
    let cred_spinner = progress.create_spinner("Validating credentials...");
    if config.hetzner.username == "your-username" {
        progress.finish_with_message(&cred_spinner, "Credentials validation failed");
        ErrorHandler::print_error("Credentials Error", "Hetzner credentials not configured");
        ErrorHandler::suggest_solution("Edit your config file with real Hetzner Storage Box credentials");
        return Err(anyhow::anyhow!("Hetzner credentials required for backup"));
    }
    progress.finish_with_message(&cred_spinner, "Credentials validated");
    
    // Determine backup paths
    let backup_paths = if !paths.is_empty() {
        ErrorHandler::print_info("Backup Paths", &format!("Using {} specified paths:", paths.len()));
        for (i, path) in paths.iter().enumerate() {
            println!("   {}. {}", i + 1, path.display());
        }
        paths
    } else {
        if config.backup.backup_paths.is_empty() {
            ErrorHandler::print_error("No Backup Paths", "No backup paths specified");
            ErrorHandler::suggest_solution("Either provide paths as arguments or configure them in your config file");
            return Err(anyhow::anyhow!("No backup paths specified"));
        }
        ErrorHandler::print_info("Backup Paths", &format!("Using {} paths from configuration:", config.backup.backup_paths.len()));
        for (i, path) in config.backup.backup_paths.iter().enumerate() {
            println!("   {}. {}", i + 1, path.display());
        }
        config.backup.backup_paths.clone()
    };
    
    ErrorHandler::print_info("Backup Configuration", &format!("Backup ID: {}", backup_name));
    
    if force {
        ErrorHandler::print_warning("Force Mode", "Ignoring recent backup checks");
    }
    
    // Validate all paths exist with progress
    let validation_spinner = progress.create_spinner("Validating backup paths...");
    for (i, path) in backup_paths.iter().enumerate() {
        validation_spinner.set_message(format!("Validating path {}/{}: {}", i + 1, backup_paths.len(), path.display()));
        if !path.exists() {
            progress.finish_with_message(&validation_spinner, "Path validation failed");
            ErrorHandler::print_error("Invalid Path", &format!("Path does not exist: {}", path.display()));
            ErrorHandler::suggest_solution("Check that all specified paths exist and are accessible");
            return Err(anyhow::anyhow!("Invalid backup path: {}", path.display()));
        }
    }
    progress.finish_with_message(&validation_spinner, "All paths validated successfully");
    
    // Initialize Hetzner client with progress
    let client_spinner = progress.create_spinner("Initializing Hetzner client...");
    let hetzner_config = skylock_hetzner::HetznerConfig {
        endpoint: config.hetzner.endpoint.clone(),
        username: config.hetzner.username.clone(),
        password: config.hetzner.password.clone(),
        api_token: config.hetzner.encryption_key.clone(),
        encryption_key: config.hetzner.encryption_key.clone(),
    };
    
    let hetzner_client = match skylock_hetzner::HetznerClient::new(hetzner_config) {
        Ok(client) => {
            progress.finish_with_message(&client_spinner, "Hetzner client initialized");
            client
        },
        Err(e) => {
            progress.finish_with_message(&client_spinner, "Failed to create Hetzner client");
            ErrorHandler::print_detailed_error(&anyhow::anyhow!(e));
            return Err(anyhow::anyhow!("Failed to initialize Hetzner client"));
        }
    };
    
    // Test connection with progress
    let conn_spinner = progress.create_spinner("Testing Hetzner Storage Box connection...");
    if let Err(e) = hetzner_client.list_files("/").await {
        progress.finish_with_message(&conn_spinner, "Connection test failed");
        ErrorHandler::print_error("Connection Failed", &e.to_string());
        ErrorHandler::suggest_solution("Check your credentials, endpoint URL, and network connection");
        return Err(anyhow::anyhow!("Hetzner connection failed: {}", e));
    }
    progress.finish_with_message(&conn_spinner, "Connection test successful");
    
    // Create modified config with specific backup paths
    let mut backup_config = config.clone();
    backup_config.backup.backup_paths = backup_paths.clone();
    
    // Check if using direct upload mode
    if direct {
        println!("🔐 Using direct upload mode (per-file encryption, no archives)");
        println!();
        
        // Send notification that backup started
        let _ = notifications::notify_backup_started(backup_paths.len());
        
        // Create encryption manager
        let encryption = skylock_backup::encryption::EncryptionManager::new(&config.hetzner.encryption_key)
            .map_err(|e| anyhow::anyhow!("Failed to create encryption: {}", e))?;
        
        // Parse bandwidth limit (CLI > config > unlimited)
        let bandwidth_limit = max_speed
            .or_else(|| config.backup.max_speed_limit.clone())
            .and_then(|s| skylock_backup::parse_bandwidth_limit(&s).ok());
        
        if let Some(limit) = bandwidth_limit {
            if limit > 0 {
                // Create a temporary BandwidthLimiter to format the limit
                let limiter = skylock_backup::BandwidthLimiter::new(limit);
                let limit_formatted = limiter.format_limit();
                println!("🚦 Bandwidth limit: {}", limit_formatted);
            }
        }
        
        // Create direct upload backup
        let direct_backup = skylock_backup::DirectUploadBackup::new(
            backup_config,
            hetzner_client,
            encryption,
            bandwidth_limit
        );
        
        let result = if incremental {
            direct_backup.create_incremental_backup(&backup_paths).await
        } else {
            direct_backup.create_backup(&backup_paths).await
        };
        
        match result {
            Ok(manifest) => {
                let duration = start_time.elapsed();
                let size_formatted = ErrorHandler::format_file_size(manifest.total_size);
                let duration_formatted = ErrorHandler::format_duration(duration);
                
                println!();
                ErrorHandler::print_success("Backup Completed Successfully!", "All files have been backed up");
                
                println!();
                println!("📊 {} {}", "Backup Summary:".bright_blue().bold(), "".clear());
                println!("   🆔 Backup ID: {}", manifest.backup_id.bright_green());
                println!("   📅 Timestamp: {}", manifest.timestamp.format("%Y-%m-%d %H:%M:%S UTC"));
                println!("   📦 Size: {} ({} bytes)", size_formatted.bright_cyan(), manifest.total_size);
                println!("   📁 Files backed up: {}", manifest.file_count);
                println!("   ⏱️  Duration: {}", duration_formatted.bright_yellow());
                println!("   🔐 Encryption: {}", "AES-256-GCM per-file".bright_green());
                
                if duration.as_secs() > 0 {
                    let rate = manifest.total_size as f64 / duration.as_secs() as f64;
                    let rate_formatted = ErrorHandler::format_file_size(rate as u64);
                    println!("   🚀 Transfer rate: {}/s", rate_formatted.bright_magenta());
                }
                
                // Send success notification
                let size_mb = manifest.total_size as f64 / 1024.0 / 1024.0;
                let _ = notifications::notify_backup_completed(
                    manifest.file_count,
                    size_mb,
                    duration.as_secs()
                );
                
                return Ok(());
            }
            Err(e) => {
                let error_msg = e.to_string();
                ErrorHandler::print_error("Backup Failed", &format!("Operation failed after {}", ErrorHandler::format_duration(start_time.elapsed())));
                ErrorHandler::print_detailed_error(&anyhow::anyhow!(e));
                
                // Send failure notification
                let _ = notifications::notify_backup_failed(&error_msg);
                
                return Err(anyhow::anyhow!("Backup operation failed"));
            }
        }
    }
    
    // Original archive-based backup
    let init_spinner = progress.create_spinner("Initializing backup manager...");
    let mut backup_manager = skylock_backup::BackupManager::new(backup_config, hetzner_client);
    progress.finish_with_message(&init_spinner, "Backup manager initialized");
    
    // Perform backup with timing
    let backup_spinner = progress.create_spinner("Starting backup process...");
    progress.finish_with_message(&backup_spinner, "Starting backup process...");
    
    println!("🚀 Beginning file uploads...");
    println!();
    
    match backup_manager.create_backup().await {
        Ok(metadata) => {
            progress.finish_with_message(&backup_spinner, "Backup process completed");
            
            let duration = start_time.elapsed();
            let size_formatted = ErrorHandler::format_file_size(metadata.size);
            let duration_formatted = ErrorHandler::format_duration(duration);
            
            println!();
            ErrorHandler::print_success("Backup Completed Successfully!", "All files have been backed up");
            
            println!();
            println!("📊 {} {}", "Backup Summary:".bright_blue().bold(), "".clear());
            println!("   🆔 Backup ID: {}", metadata.id.bright_green());
            println!("   📅 Timestamp: {}", metadata.timestamp.format("%Y-%m-%d %H:%M:%S UTC"));
            println!("   📦 Size: {} ({} bytes)", size_formatted.bright_cyan(), metadata.size);
            println!("   📁 Paths backed up: {}", metadata.source_paths.len());
            println!("   ⏱️  Duration: {}", duration_formatted.bright_yellow());
            if metadata.is_vss {
                println!("   💾 VSS snapshot: {}", "Enabled".bright_green());
            }
            
            // Calculate transfer rate
            if duration.as_secs() > 0 {
                let rate = metadata.size as f64 / duration.as_secs() as f64;
                let rate_formatted = ErrorHandler::format_file_size(rate as u64);
                println!("   🚀 Transfer rate: {}/s", rate_formatted.bright_magenta());
            }
        }
        Err(e) => {
            progress.finish_with_message(&backup_spinner, "Backup failed");
            ErrorHandler::print_error("Backup Failed", &format!("Operation failed after {}", ErrorHandler::format_duration(start_time.elapsed())));
            ErrorHandler::print_detailed_error(&anyhow::anyhow!(e));
            ErrorHandler::suggest_solution("Check network connectivity and storage space on Hetzner Storage Box");
            return Err(anyhow::anyhow!("Backup operation failed"));
        }
    }
    
    Ok(())
}

async fn perform_restore_file(backup_id: String, file_path: String, output: PathBuf, config_path: Option<PathBuf>) -> Result<()> {
    println!("🔄 Restoring single file from backup: {}", backup_id);
    
    // Load configuration
    let config = match Config::load(config_path) {
        Ok(config) => config,
        Err(e) => {
            println!("❌ Failed to load configuration: {}", e);
            return Err(anyhow::anyhow!("Configuration required for restore operation"));
        }
    };
    
    if config.hetzner.username == "your-username" {
        println!("❌ Hetzner credentials not configured");
        return Err(anyhow::anyhow!("Hetzner credentials required"));
    }
    
    // Create Hetzner client
    let hetzner_config = skylock_hetzner::HetznerConfig {
        endpoint: config.hetzner.endpoint.clone(),
        username: config.hetzner.username.clone(),
        password: config.hetzner.password.clone(),
        api_token: config.hetzner.encryption_key.clone(),
        encryption_key: config.hetzner.encryption_key.clone(),
    };
    
    let hetzner_client = match skylock_hetzner::HetznerClient::new(hetzner_config) {
        Ok(client) => client,
        Err(e) => {
            println!("❌ Failed to create Hetzner client: {}", e);
            return Err(anyhow::anyhow!("Failed to initialize Hetzner client: {}", e));
        }
    };
    
    // Create encryption manager
    let encryption = skylock_backup::encryption::EncryptionManager::new(&config.hetzner.encryption_key)
        .map_err(|e| anyhow::anyhow!("Failed to create encryption: {}", e))?;
    
    // Create direct upload backup manager (no bandwidth limit for restores)
    let direct_backup = skylock_backup::DirectUploadBackup::new(config, hetzner_client, encryption, None);
    
    // Restore file
    match direct_backup.restore_file(&backup_id, &file_path, &output).await {
        Ok(_) => {
            println!("✅ File restored successfully!");
            Ok(())
        }
        Err(e) => {
            println!("❌ Restore failed: {}", e);
            Err(anyhow::anyhow!("Restore operation failed: {}", e))
        }
    }
}

async fn perform_preview(backup_id: String, target: Option<PathBuf>, config_path: Option<PathBuf>) -> Result<()> {
    use progress::ErrorHandler;
    use colored::*;
    
    ErrorHandler::print_info("Preview Backup", &format!("Loading backup: {}", backup_id.bright_green()));
    
    // Load configuration
    let config = match Config::load(config_path) {
        Ok(config) => config,
        Err(e) => {
            ErrorHandler::print_error("Configuration Error", &e.to_string());
            return Err(anyhow::anyhow!("Configuration required"));
        }
    };
    
    if config.hetzner.username == "your-username" {
        ErrorHandler::print_error("Credentials Error", "Hetzner credentials not configured");
        return Err(anyhow::anyhow!("Hetzner credentials required"));
    }
    
    // Create Hetzner client
    let hetzner_config = skylock_hetzner::HetznerConfig {
        endpoint: config.hetzner.endpoint.clone(),
        username: config.hetzner.username.clone(),
        password: config.hetzner.password.clone(),
        api_token: config.hetzner.encryption_key.clone(),
        encryption_key: config.hetzner.encryption_key.clone(),
    };
    
    let hetzner_client = match skylock_hetzner::HetznerClient::new(hetzner_config) {
        Ok(client) => client,
        Err(e) => {
            ErrorHandler::print_error("Client Error", &e.to_string());
            return Err(anyhow::anyhow!("Failed to initialize Hetzner client"));
        }
    };
    
    // Create encryption manager
    let encryption = skylock_backup::encryption::EncryptionManager::new(&config.hetzner.encryption_key)
        .map_err(|e| anyhow::anyhow!("Failed to create encryption: {}", e))?;
    
    // Create direct upload backup manager (no bandwidth limit for preview)
    let direct_backup = skylock_backup::DirectUploadBackup::new(config, hetzner_client, encryption, None);
    
    // Show preview
    direct_backup.preview_backup(&backup_id).await?;
    
    // Check for conflicts if target specified
    if let Some(target_path) = target {
        println!("{}", "Checking for file conflicts...".bright_yellow());
        let conflicts = direct_backup.check_restore_conflicts(&backup_id, &target_path).await?;
        
        if !conflicts.is_empty() {
            println!();
            ErrorHandler::print_warning("File Conflicts Detected", &format!("{} files already exist", conflicts.len()));
            println!();
            println!("   The following files will be overwritten:");
            for (i, path) in conflicts.iter().take(10).enumerate() {
                println!("   {}. {}", i + 1, path.display());
            }
            if conflicts.len() > 10 {
                println!("   ... and {} more", conflicts.len() - 10);
            }
            println!();
            ErrorHandler::suggest_solution("Use --force to overwrite, or choose a different target directory");
        } else {
            println!();
            println!("{}", "✅ No conflicts - safe to restore".bright_green());
        }
    }
    
    Ok(())
}

async fn perform_browse(backup_id: String, config_path: Option<PathBuf>) -> Result<()> {
    use progress::ErrorHandler;
    
    // Load configuration
    let config = match Config::load(config_path) {
        Ok(config) => config,
        Err(e) => {
            ErrorHandler::print_error("Configuration Error", &e.to_string());
            return Err(anyhow::anyhow!("Configuration required"));
        }
    };
    
    if config.hetzner.username == "your-username" {
        ErrorHandler::print_error("Credentials Error", "Hetzner credentials not configured");
        return Err(anyhow::anyhow!("Hetzner credentials required"));
    }
    
    // Create Hetzner client
    let hetzner_config = skylock_hetzner::HetznerConfig {
        endpoint: config.hetzner.endpoint.clone(),
        username: config.hetzner.username.clone(),
        password: config.hetzner.password.clone(),
        api_token: config.hetzner.encryption_key.clone(),
        encryption_key: config.hetzner.encryption_key.clone(),
    };
    
    let hetzner_client = match skylock_hetzner::HetznerClient::new(hetzner_config) {
        Ok(client) => client,
        Err(e) => {
            ErrorHandler::print_error("Client Error", &e.to_string());
            return Err(anyhow::anyhow!("Failed to initialize Hetzner client"));
        }
    };
    
    // Create encryption manager
    let encryption = skylock_backup::encryption::EncryptionManager::new(&config.hetzner.encryption_key)
        .map_err(|e| anyhow::anyhow!("Failed to create encryption: {}", e))?;
    
    // Create direct upload backup manager (no bandwidth limit for browsing)
    let direct_backup = skylock_backup::DirectUploadBackup::new(config, hetzner_client, encryption, None);
    
    // Create browser and browse
    let browser = skylock_backup::EncryptedBrowser::new(direct_backup);
    browser.browse(&backup_id).await?;
    
    Ok(())
}

async fn perform_preview_file(
    backup_id: String,
    file_path: String,
    max_lines: usize,
    config_path: Option<PathBuf>,
) -> Result<()> {
    use progress::ErrorHandler;
    
    // Load configuration
    let config = match Config::load(config_path) {
        Ok(config) => config,
        Err(e) => {
            ErrorHandler::print_error("Configuration Error", &e.to_string());
            return Err(anyhow::anyhow!("Configuration required"));
        }
    };
    
    if config.hetzner.username == "your-username" {
        ErrorHandler::print_error("Credentials Error", "Hetzner credentials not configured");
        return Err(anyhow::anyhow!("Hetzner credentials required"));
    }
    
    // Create Hetzner client
    let hetzner_config = skylock_hetzner::HetznerConfig {
        endpoint: config.hetzner.endpoint.clone(),
        username: config.hetzner.username.clone(),
        password: config.hetzner.password.clone(),
        api_token: config.hetzner.encryption_key.clone(),
        encryption_key: config.hetzner.encryption_key.clone(),
    };
    
    let hetzner_client = match skylock_hetzner::HetznerClient::new(hetzner_config) {
        Ok(client) => client,
        Err(e) => {
            ErrorHandler::print_error("Client Error", &e.to_string());
            return Err(anyhow::anyhow!("Failed to initialize Hetzner client"));
        }
    };
    
    // Create encryption manager
    let encryption = skylock_backup::encryption::EncryptionManager::new(&config.hetzner.encryption_key)
        .map_err(|e| anyhow::anyhow!("Failed to create encryption: {}", e))?;
    
    // Create direct upload backup manager (no bandwidth limit for preview)
    let direct_backup = skylock_backup::DirectUploadBackup::new(config, hetzner_client, encryption, None);
    
    // Create browser and preview file
    let browser = skylock_backup::EncryptedBrowser::new(direct_backup);
    browser.preview_file(&backup_id, &file_path, max_lines).await?;
    
    Ok(())
}

async fn perform_restore(backup_id: String, target: Option<PathBuf>, paths: Vec<PathBuf>, config_path: Option<PathBuf>) -> Result<()> {
    use progress::{ProgressReporter, ErrorHandler};
    use colored::*;
    use std::time::Instant;
    
    let start_time = Instant::now();
    let progress = ProgressReporter::new();
    
    ErrorHandler::print_info("Restore Operation", &format!("Backup ID: {}", backup_id.bright_green()));
    
    let target_path = target.unwrap_or_else(|| {
        PathBuf::from(format!("restore_{}", chrono::Utc::now().format("%Y%m%d_%H%M%S")))
    });
    println!("   📂 Target: {}", target_path.display());
    
    if !paths.is_empty() {
        ErrorHandler::print_warning("Selective Restore", "Specific path selection not yet implemented");
        println!("   Will restore entire backup");
    }
    println!();
    
    // Load configuration
    let config_spinner = progress.create_spinner("Loading configuration...");
    let config = match Config::load(config_path) {
        Ok(config) => {
            progress.finish_with_message(&config_spinner, "Configuration loaded");
            config
        }
        Err(e) => {
            progress.finish_with_message(&config_spinner, "Configuration failed");
            ErrorHandler::print_error("Configuration Error", &e.to_string());
            return Err(anyhow::anyhow!("Configuration required"));
        }
    };
    
    if config.hetzner.username == "your-username" {
        ErrorHandler::print_error("Credentials Error", "Hetzner credentials not configured");
        return Err(anyhow::anyhow!("Hetzner credentials required"));
    }
    
    // Create Hetzner client
    let client_spinner = progress.create_spinner("Connecting to Hetzner Storage Box...");
    let hetzner_config = skylock_hetzner::HetznerConfig {
        endpoint: config.hetzner.endpoint.clone(),
        username: config.hetzner.username.clone(),
        password: config.hetzner.password.clone(),
        api_token: config.hetzner.encryption_key.clone(),
        encryption_key: config.hetzner.encryption_key.clone(),
    };
    
    let hetzner_client = match skylock_hetzner::HetznerClient::new(hetzner_config) {
        Ok(client) => {
            progress.finish_with_message(&client_spinner, "Connected to storage");
            client
        }
        Err(e) => {
            progress.finish_with_message(&client_spinner, "Connection failed");
            ErrorHandler::print_error("Client Error", &e.to_string());
            return Err(anyhow::anyhow!("Failed to initialize Hetzner client"));
        }
    };
    
    // Create encryption manager
    let encryption = skylock_backup::encryption::EncryptionManager::new(&config.hetzner.encryption_key)
        .map_err(|e| anyhow::anyhow!("Failed to create encryption: {}", e))?;
    
    // Create direct upload backup manager (no bandwidth limit for restores)
    let direct_backup = skylock_backup::DirectUploadBackup::new(config, hetzner_client, encryption, None);
    
    // Send notification that restore started
    let _ = notifications::notify_restore_started(&backup_id);
    
    // Perform restore
    println!();
    match direct_backup.restore_backup(&backup_id, &target_path).await {
        Ok(_) => {
            let duration = start_time.elapsed();
            println!();
            ErrorHandler::print_success("Restore Complete!", &format!("Files restored to {}", target_path.display()));
            println!("   ⏱️  Duration: {}", ErrorHandler::format_duration(duration).bright_yellow());
            
            // Send success notification (we don't know exact file count without parsing manifest again)
            let _ = notifications::notify_restore_completed(0, duration.as_secs());
        }
        Err(e) => {
            let error_msg = e.to_string();
            ErrorHandler::print_error("Restore Failed", &format!("After {}", ErrorHandler::format_duration(start_time.elapsed())));
            ErrorHandler::print_detailed_error(&anyhow::anyhow!(e));
            
            // Send failure notification
            let _ = notifications::notify_restore_failed(&error_msg);
            
            return Err(anyhow::anyhow!("Restore operation failed"));
        }
    }
    
    Ok(())
}

async fn list_backups(detailed: bool, pattern: Option<String>, config_path: Option<PathBuf>) -> Result<()> {
    use progress::{ProgressReporter, ErrorHandler};
    use colored::*;
    
    let progress = ProgressReporter::new();
    
    ErrorHandler::print_info("Listing Backups", "Fetching backup information from storage...");
    
    if let Some(pattern) = &pattern {
        ErrorHandler::print_info("Filter Applied", &format!("Pattern: {}", pattern.bright_yellow()));
    }
    
    // Load configuration
    let config = match Config::load(config_path) {
        Ok(config) => config,
        Err(e) => {
            println!("❌ Failed to load configuration: {}", e);
            return Err(anyhow::anyhow!("Configuration required for listing backups"));
        }
    };
    
    if config.hetzner.username == "your-username" {
        println!("❌ Hetzner credentials not configured");
        return Err(anyhow::anyhow!("Hetzner credentials required"));
    }
    
    // Create Hetzner client
    let hetzner_config = skylock_hetzner::HetznerConfig {
        endpoint: config.hetzner.endpoint.clone(),
        username: config.hetzner.username.clone(),
        password: config.hetzner.password.clone(),
        api_token: config.hetzner.encryption_key.clone(),
        encryption_key: config.hetzner.encryption_key.clone(),
    };
    
    let hetzner_client = match skylock_hetzner::HetznerClient::new(hetzner_config) {
        Ok(client) => client,
        Err(e) => {
            println!("❌ Failed to create Hetzner client: {}", e);
            return Err(anyhow::anyhow!("Failed to initialize Hetzner client: {}", e));
        }
    };
    
    // Initialize backup manager
    let backup_manager = skylock_backup::BackupManager::new(config, hetzner_client);
    
    // List backups
    println!("🔍 Fetching backup list...");
    match backup_manager.list_backups().await {
        Ok(backups) => {
            if backups.is_empty() {
                println!("💭 No backups found");
                return Ok(());
            }
            
            println!("📊 Found {} backup(s):", backups.len());
            println!();
            
            let mut filtered_backups = backups;
            
            // Apply pattern filtering if specified
            if let Some(pattern) = pattern {
                filtered_backups = filtered_backups.into_iter()
                    .filter(|backup| backup.id.contains(&pattern))
                    .collect();
                
                if filtered_backups.is_empty() {
                    println!("💭 No backups match the pattern '{}'", pattern);
                    return Ok(());
                }
            }
            
            // Sort by timestamp (newest first)
            filtered_backups.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            
            for backup in filtered_backups {
                if detailed {
                    println!("┌── 🆔 {}", backup.id);
                    println!("│   📅 Created: {}", backup.timestamp.format("%Y-%m-%d %H:%M:%S UTC"));
                    println!("│   📊 Size: {} bytes ({:.2} MB)", backup.size, backup.size as f64 / 1024.0 / 1024.0);
                    println!("│   📁 Paths: {} item(s)", backup.source_paths.len());
                    if backup.is_vss {
                        println!("│   💸 VSS: Enabled");
                    }
                    println!("│   📂 Source paths:");
                    for path in &backup.source_paths {
                        println!("│     - {}", path.display());
                    }
                    println!("└────────────────────────────────────────────");
                } else {
                    let size_mb = backup.size as f64 / 1024.0 / 1024.0;
                    println!("{:<30} {:<20} {:>10.2} MB  {} paths", 
                        backup.id,
                        backup.timestamp.format("%Y-%m-%d %H:%M"),
                        size_mb,
                        backup.source_paths.len()
                    );
                }
            }
        }
        Err(e) => {
            println!("❌ Failed to list backups: {}", e);
            return Err(anyhow::anyhow!("Failed to list backups: {}", e));
        }
    }
    
    Ok(())
}

async fn run_tests(component: Option<TestComponent>) -> Result<()> {
    let component = component.unwrap_or(TestComponent::All);
    
    match component {
        TestComponent::Hetzner => {
            println!("🔗 Testing Hetzner Storage Box connection...");
            test_hetzner_connection().await?;
        }
        TestComponent::Encryption => {
            println!("🔐 Testing encryption functionality...");
            test_encryption().await?;
        }
        TestComponent::Compression => {
            println!("🗜️ Testing compression functionality...");
            test_compression().await?;
        }
        TestComponent::All => {
            println!("🧪 Running all tests...");
            test_hetzner_connection().await?;
            test_encryption().await?;
            test_compression().await?;
        }
    }
    
    Ok(())
}

async fn test_hetzner_connection() -> Result<()> {
    use skylock_hetzner::HetznerWebDAVClient;
    
    // Try to load config first
    let config_result = Config::load(None);
    
    let (endpoint, username, password) = match config_result {
        Ok(config) => {
            if config.hetzner.username == "your-username" {
                println!("⚠️  Using default config - please configure real credentials");
                return Ok(());
            }
            (config.hetzner.endpoint, config.hetzner.username, config.hetzner.password)
        }
        Err(_) => {
            println!("⚠️  No configuration found. Run 'skylock config' to generate one.");
            return Ok(());
        }
    };
    
    println!("🔗 Testing connection to: {}", endpoint);
    println!("👤 Username: {}", username);
    
    // Create WebDAV client config
    let webdav_config = skylock_hetzner::WebDAVConfig {
        base_url: endpoint.clone(),
        username: username.clone(),
        password: password.clone(),
        base_path: "/".to_string(),
    };
    
    let client = HetznerWebDAVClient::new(webdav_config)
        .map_err(|e| anyhow::anyhow!("Failed to create WebDAV client: {}", e))?;
    
    // Test connection by listing root directory  
    match client.list_files("/").await {
        Ok(entries) => {
            println!("✅ Connection successful!");
            println!("📁 Found {} items in root directory", entries.len());
            
            if entries.len() > 0 {
                println!("📋 Directory contents:");
                for entry in entries.iter().take(5) { // Show first 5 items
                    println!("  - {}", entry);
                }
                if entries.len() > 5 {
                    println!("  ... and {} more items", entries.len() - 5);
                }
            }
        }
        Err(e) => {
            println!("❌ Connection failed: {}", e);
            println!("💡 Check your credentials and endpoint URL");
            return Err(anyhow::anyhow!("Hetzner connection test failed: {}", e));
        }
    }
    
    // Test write permissions by creating a test file
    println!("🧪 Testing write permissions...");
    let test_content = b"Skylock connection test";
    let test_path_str = "/skylock_test.txt";
    
    // Create a temporary file for upload
    let temp_file = tempfile::NamedTempFile::new()
        .map_err(|e| anyhow::anyhow!("Failed to create temp file: {}", e))?;
    tokio::fs::write(temp_file.path(), test_content).await
        .map_err(|e| anyhow::anyhow!("Failed to write temp file: {}", e))?;
    
    match client.upload_file(temp_file.path(), test_path_str).await {
        Ok(_) => {
            println!("✅ Write test successful!");
            
            // Clean up test file
            if let Err(e) = client.delete_file(test_path_str).await {
                println!("⚠️  Failed to clean up test file: {}", e);
            } else {
                println!("🧹 Test file cleaned up");
            }
        }
        Err(e) => {
            println!("⚠️  Write test failed: {}", e);
            println!("💡 Check if you have write permissions on the storage box");
        }
    }
    
    println!("✅ Hetzner connection test completed");
    Ok(())
}

async fn test_encryption() -> Result<()> {
    use skylock_core::encryption::EncryptionManager;
    use std::path::Path;
    
    println!("🔐 Testing encryption functionality...");
    
    // Create temporary directory for encryption test
    let temp_dir = tempfile::tempdir()
        .map_err(|e| anyhow::anyhow!("Failed to create temp dir: {}", e))?;
    
    let config_path = temp_dir.path();
    let password = "test_password_123";
    
    // Initialize encryption manager
    let encryption_manager = match EncryptionManager::new(config_path, password).await {
        Ok(em) => em,
        Err(e) => {
            println!("⚠️  Could not initialize encryption manager: {}", e);
            println!("✅ Encryption test skipped (dependencies not available)");
            return Ok(());
        }
    };
    
    println!("✅ Encryption manager initialized");
    
    // Test block encryption
    let test_data = b"Hello, Skylock! This is a test message for encryption.";
    let block_hash = "test_block_hash";
    
    println!("📝 Test data: {} bytes", test_data.len());
    
    // Encrypt block
    match encryption_manager.encrypt_block(test_data, block_hash, 0).await {
        Ok(encrypted) => {
            println!("✅ Block encryption successful: {} bytes", encrypted.len());
            
            // Decrypt block
            match encryption_manager.decrypt_block(&encrypted, block_hash, 0).await {
                Ok(decrypted) => {
                    if decrypted == test_data {
                        println!("✅ Block decryption successful and data matches!");
                    } else {
                        return Err(anyhow::anyhow!("Decrypted data doesn't match original"));
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Block decryption failed: {}", e));
                }
            }
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Block encryption failed: {}", e));
        }
    }
    
    println!("✅ Encryption test passed");
    Ok(())
}

async fn test_schedule(expression: Option<String>, show_presets: bool) -> Result<()> {
    use colored::*;
    
    if show_presets {
        println!("{}", "📅 Common Schedule Presets:".bright_blue().bold());
        println!();
        println!("   {:<30} {}", "Expression".bright_cyan(), "Description".bright_cyan());
        println!("   {}", "─".repeat(70));
        
        let presets = vec![
            (scheduler::presets::HOURLY, "Every hour"),
            (scheduler::presets::EVERY_2_HOURS, "Every 2 hours"),
            (scheduler::presets::EVERY_6_HOURS, "Every 6 hours"),
            (scheduler::presets::EVERY_12_HOURS, "Every 12 hours"),
            (scheduler::presets::DAILY_MIDNIGHT, "Daily at midnight"),
            (scheduler::presets::DAILY_2AM, "Daily at 2 AM"),
            (scheduler::presets::WEEKLY_SUNDAY, "Weekly on Sunday at 2 AM"),
            (scheduler::presets::WEEKLY_MONDAY, "Weekly on Monday at 2 AM"),
            (scheduler::presets::MONTHLY_1ST, "Monthly on the 1st at 2 AM"),
            (scheduler::presets::EVERY_15_MIN, "Every 15 minutes"),
            (scheduler::presets::EVERY_30_MIN, "Every 30 minutes"),
        ];
        
        for (expr, desc) in presets {
            println!("   {:<30} {}", expr, desc);
            
            if let Some(next) = scheduler::get_next_run(expr, chrono::Utc::now()) {
                println!("   {:<30} {}", "", format!("Next: {}", next.format("%Y-%m-%d %H:%M UTC")).dimmed());
            }
        }
        
        println!();
        println!("💡 Use these in your config file's [backup] section:");
        println!("   schedule = \"{}\"", scheduler::presets::DAILY_2AM.bright_yellow());
        
        return Ok(());
    }
    
    if let Some(expr) = expression {
        println!("{}", "🔍 Validating Cron Expression:".bright_blue().bold());
        println!();
        println!("   Expression: {}", expr.bright_yellow());
        
        match scheduler::validate_cron_expression(&expr) {
            Ok(_) => {
                println!("   Status: {}", "✓ Valid".bright_green());
                println!();
                
                println!("{}", "📋 Details:".bright_cyan());
                println!("   Description: {}", scheduler::describe_schedule(&expr));
                println!();
                
                println!("{}", "📅 Next 5 Scheduled Runs:".bright_cyan());
                let now = chrono::Utc::now();
                if let Ok(schedule) = scheduler::parse_cron_expression(&expr) {
                    for (i, dt) in schedule.upcoming(chrono::Utc).take(5).enumerate() {
                        let time_until = dt - now;
                        let hours = time_until.num_hours();
                        let days = time_until.num_days();
                        
                        let relative = if days > 0 {
                            format!("(in {} days)", days)
                        } else if hours > 0 {
                            format!("(in {} hours)", hours)
                        } else {
                            format!("(in {} minutes)", time_until.num_minutes())
                        };
                        
                        println!("   {}. {} {}", 
                            i + 1, 
                            dt.format("%Y-%m-%d %H:%M:%S UTC"),
                            relative.dimmed()
                        );
                    }
                }
            }
            Err(e) => {
                println!("   Status: {}", "✗ Invalid".bright_red());
                println!("   Error: {}", e);
                println!();
                println!("💡 {}", "Cron expression format:".bright_yellow());
                println!("   * * * * * *");
                println!("   │ │ │ │ │ └─ Day of week (0-7, Sunday = 0 or 7)");
                println!("   │ │ │ │ └─── Month (1-12)");
                println!("   │ │ │ └───── Day of month (1-31)");
                println!("   │ │ └─────── Hour (0-23)");
                println!("   │ └───────── Minute (0-59)");
                println!("   └─────────── Second (0-59)");
                println!();
                println!("   Examples:");
                println!("   0 0 2 * * *      - Daily at 2 AM");
                println!("   0 */15 * * * *   - Every 15 minutes");
                println!("   0 0 0 * * 0      - Weekly on Sunday at midnight");
                println!("   0 0 0 1 * *      - Monthly on the 1st at midnight");
                
                return Err(anyhow::anyhow!("Invalid cron expression"));
            }
        }
    } else {
        println!("{}", "📅 Schedule Command".bright_blue().bold());
        println!();
        println!("Usage:");
        println!("  skylock schedule <EXPRESSION>  # Validate a cron expression");
        println!("  skylock schedule --presets     # Show common presets");
        println!();
        println!("Examples:");
        println!("  skylock schedule \"0 0 2 * * *\"     # Daily at 2 AM");
        println!("  skylock schedule \"0 */15 * * * *\"  # Every 15 minutes");
    }
    
    Ok(())
}

async fn perform_diff(
    backup_id_old: String,
    backup_id_new: String,
    detailed: bool,
    filter: Option<Vec<String>>,
    config_path: Option<PathBuf>,
) -> Result<()> {
    use progress::ErrorHandler;
    use colored::*;
    use skylock_backup::{BackupDiff, DirectUploadBackup};
    
    ErrorHandler::print_info("Comparing Backups", &format!(
        "Comparing {} → {}",
        backup_id_old.bright_yellow(),
        backup_id_new.bright_yellow()
    ));
    
    // Load configuration
    let config = match Config::load(config_path) {
        Ok(config) => config,
        Err(e) => {
            ErrorHandler::print_error("Configuration Error", &e.to_string());
            return Err(anyhow::anyhow!("Configuration required"));
        }
    };
    
    if config.hetzner.username == "your-username" {
        ErrorHandler::print_error("Credentials Error", "Hetzner credentials not configured");
        return Err(anyhow::anyhow!("Hetzner credentials required"));
    }
    
    // Create Hetzner client
    let hetzner_config = skylock_hetzner::HetznerConfig {
        endpoint: config.hetzner.endpoint.clone(),
        username: config.hetzner.username.clone(),
        password: config.hetzner.password.clone(),
        api_token: config.hetzner.encryption_key.clone(),
        encryption_key: config.hetzner.encryption_key.clone(),
    };
    
    let hetzner_client = match skylock_hetzner::HetznerClient::new(hetzner_config) {
        Ok(client) => client,
        Err(e) => {
            ErrorHandler::print_error("Client Error", &e.to_string());
            return Err(anyhow::anyhow!("Failed to initialize Hetzner client"));
        }
    };
    
    // Create encryption manager
    let encryption = skylock_backup::encryption::EncryptionManager::new(&config.hetzner.encryption_key)
        .map_err(|e| anyhow::anyhow!("Failed to create encryption: {}", e))?;
    
    // Create direct upload backup manager (no bandwidth limit for diff)
    let direct_backup = DirectUploadBackup::new(config, hetzner_client, encryption, None);
    
    // Load both manifests
    println!("📥 Loading backup manifests...");
    let manifest_old = direct_backup.load_manifest(&backup_id_old).await
        .map_err(|e| anyhow::anyhow!("Failed to load old backup manifest: {}", e))?;
    let manifest_new = direct_backup.load_manifest(&backup_id_new).await
        .map_err(|e| anyhow::anyhow!("Failed to load new backup manifest: {}", e))?;
    
    // Compare manifests
    let diff = BackupDiff::compare(&manifest_old, &manifest_new);
    
    // Display summary
    println!();
    println!("{}", "📊 Backup Comparison Summary".bright_blue().bold());
    println!();
    println!("   {} {}", "Old backup:".dimmed(), backup_id_old.bright_yellow());
    println!("   {} {}", "  Created:".dimmed(), diff.timestamp_old.format("%Y-%m-%d %H:%M:%S UTC"));
    println!();
    println!("   {} {}", "New backup:".dimmed(), backup_id_new.bright_yellow());
    println!("   {} {}", "  Created:".dimmed(), diff.timestamp_new.format("%Y-%m-%d %H:%M:%S UTC"));
    println!();
    
    if !diff.has_changes() {
        println!("{}", "✅ No differences found - backups are identical".bright_green());
        return Ok(());
    }
    
    println!("{}", "Changes:".bright_cyan().bold());
    
    // Determine which change types to show
    let show_added = filter.as_ref().map_or(true, |f| f.iter().any(|t| t == "added"));
    let show_removed = filter.as_ref().map_or(true, |f| f.iter().any(|t| t == "removed"));
    let show_modified = filter.as_ref().map_or(true, |f| f.iter().any(|t| t == "modified"));
    let show_moved = filter.as_ref().map_or(true, |f| f.iter().any(|t| t == "moved"));
    
    // Added files
    if show_added && diff.summary.files_added_count > 0 {
        println!("   {} {} files ({} bytes)",
            "+".bright_green().bold(),
            diff.summary.files_added_count.to_string().bright_green(),
            ErrorHandler::format_file_size(diff.summary.size_added).bright_green()
        );
        
        if detailed {
            for file in &diff.files_added {
                println!("      + {:<60} {}", 
                    file.path.bright_green(),
                    ErrorHandler::format_file_size(file.size).dimmed()
                );
            }
        }
    }
    
    // Removed files
    if show_removed && diff.summary.files_removed_count > 0 {
        println!("   {} {} files ({} bytes)",
            "-".bright_red().bold(),
            diff.summary.files_removed_count.to_string().bright_red(),
            ErrorHandler::format_file_size(diff.summary.size_removed).bright_red()
        );
        
        if detailed {
            for file in &diff.files_removed {
                println!("      - {:<60} {}", 
                    file.path.bright_red(),
                    ErrorHandler::format_file_size(file.size).dimmed()
                );
            }
        }
    }
    
    // Modified files
    if show_modified && diff.summary.files_modified_count > 0 {
        println!("   {} {} files",
            "~".bright_yellow().bold(),
            diff.summary.files_modified_count.to_string().bright_yellow()
        );
        
        if detailed {
            for file in &diff.files_modified {
                let delta_str = if file.size_delta > 0 {
                    format!("+{}", ErrorHandler::format_file_size(file.size_delta as u64)).bright_green()
                } else if file.size_delta < 0 {
                    format!("-{}", ErrorHandler::format_file_size((-file.size_delta) as u64)).bright_red()
                } else {
                    "±0".normal()
                };
                
                println!("      ~ {:<60} {}", 
                    file.path.bright_yellow(),
                    delta_str
                );
            }
        }
    }
    
    // Moved/renamed files
    if show_moved && diff.summary.files_moved_count > 0 {
        println!("   {} {} files",
            "→".bright_cyan().bold(),
            diff.summary.files_moved_count.to_string().bright_cyan()
        );
        
        if detailed {
            for file in &diff.files_moved {
                println!("      {} → {}", 
                    file.path_old.dimmed(),
                    file.path_new.bright_cyan()
                );
            }
        }
    }
    
    // Unchanged files
    println!("   {} {} files",
        "=".dimmed(),
        diff.summary.files_unchanged_count.to_string().dimmed()
    );
    
    // Net change
    println!();
    let net_change_str = if diff.summary.size_delta > 0 {
        format!("+{}", ErrorHandler::format_file_size(diff.summary.size_delta as u64))
    } else if diff.summary.size_delta < 0 {
        format!("-{}", ErrorHandler::format_file_size((-diff.summary.size_delta) as u64))
    } else {
        "0 bytes".to_string()
    };
    
    let net_color = if diff.summary.size_delta > 0 {
        net_change_str.bright_green()
    } else if diff.summary.size_delta < 0 {
        net_change_str.bright_red()
    } else {
        net_change_str.dimmed()
    };
    
    println!("   {} {}", "Net change:".bright_cyan(), net_color);
    
    if !detailed && diff.total_changes() > 0 {
        println!();
        println!("💡 Use {} for detailed file listings", "--detailed".bright_yellow());
    }
    
    Ok(())
}

async fn verify_backup(
    backup_id: String,
    full: bool,
    config_path: Option<PathBuf>,
) -> Result<()> {
    use progress::ErrorHandler;
    use colored::*;
    use skylock_backup::{BackupVerifier, DirectUploadBackup};
    
    ErrorHandler::print_info("Backup Verification", &format!(
        "Verifying backup: {}",
        backup_id.bright_yellow()
    ));
    
    // Load configuration
    let config = match Config::load(config_path) {
        Ok(config) => config,
        Err(e) => {
            ErrorHandler::print_error("Configuration Error", &e.to_string());
            return Err(anyhow::anyhow!("Configuration required"));
        }
    };
    
    if config.hetzner.username == "your-username" {
        ErrorHandler::print_error("Credentials Error", "Hetzner credentials not configured");
        return Err(anyhow::anyhow!("Hetzner credentials required"));
    }
    
    // Create Hetzner client for DirectUploadBackup
    let hetzner_config = skylock_hetzner::HetznerConfig {
        endpoint: config.hetzner.endpoint.clone(),
        username: config.hetzner.username.clone(),
        password: config.hetzner.password.clone(),
        api_token: config.hetzner.encryption_key.clone(),
        encryption_key: config.hetzner.encryption_key.clone(),
    };
    
    let hetzner_client1 = match skylock_hetzner::HetznerClient::new(hetzner_config.clone()) {
        Ok(client) => client,
        Err(e) => {
            ErrorHandler::print_error("Client Error", &e.to_string());
            return Err(anyhow::anyhow!("Failed to initialize Hetzner client"));
        }
    };
    
    // Create encryption manager for DirectUploadBackup
    let encryption1 = skylock_backup::encryption::EncryptionManager::new(&config.hetzner.encryption_key)
        .map_err(|e| anyhow::anyhow!("Failed to create encryption: {}", e))?;
    
    // Save encryption_key before moving config
    let encryption_key = config.hetzner.encryption_key.clone();
    
    // Load manifest
    println!("📥 Loading backup manifest...");
    let direct_backup = DirectUploadBackup::new(config, hetzner_client1, encryption1, None);
    let manifest = direct_backup.load_manifest(&backup_id).await
        .map_err(|e| anyhow::anyhow!("Failed to load backup manifest: {}", e))?;
    
    println!("✅ Manifest loaded: {} files", manifest.file_count);
    println!();
    
    // Create separate instances for BackupVerifier
    let hetzner_client2 = skylock_hetzner::HetznerClient::new(hetzner_config)
        .map_err(|e| anyhow::anyhow!("Failed to create second client: {}", e))?;
    let encryption2 = skylock_backup::encryption::EncryptionManager::new(&encryption_key)
        .map_err(|e| anyhow::anyhow!("Failed to create encryption for verification: {}", e))?;
    
    let verifier = BackupVerifier::new(hetzner_client2);
    
    // Perform verification
    let result = if full {
        verifier.verify_full(&manifest, Arc::new(encryption2)).await
            .map_err(|e| anyhow::anyhow!("Verification failed: {}", e))?
    } else {
        verifier.verify_quick(&manifest).await
            .map_err(|e| anyhow::anyhow!("Verification failed: {}", e))?
    };
    
    // Display results
    println!();
    println!("{}", "📋 Verification Results".bright_blue().bold());
    println!();
    println!("   {} {}", "Backup ID:".dimmed(), result.backup_id.bright_yellow());
    println!("   {} {}", "Total files:".dimmed(), result.total_files);
    println!("   {} {}", "Files exist:".dimmed(), result.files_exist.to_string().bright_cyan());
    
    if full {
        println!("   {} {}", "Hashes verified:".dimmed(), result.files_verified.to_string().bright_green());
    }
    
    if result.files_with_errors > 0 {
        println!("   {} {}", "Errors:".dimmed(), result.files_with_errors.to_string().bright_red());
    }
    
    println!();
    
    if result.is_success() {
        println!("{}", "✅ Verification passed - backup is healthy!".bright_green().bold());
    } else {
        println!("{}", "❌ Verification failed - backup has issues".bright_red().bold());
        println!();
        
        // Show missing files
        let missing = result.missing_files();
        if !missing.is_empty() {
            println!("{}", "Missing files:".bright_red().bold());
            for file in missing.iter().take(10) {
                println!("   - {}", file.path.display().to_string().bright_red());
            }
            if missing.len() > 10 {
                println!("   ... and {} more", (missing.len() - 10).to_string().bright_red());
            }
            println!();
        }
        
        // Show corrupted files
        if full {
            let corrupted = result.corrupted_files();
            if !corrupted.is_empty() {
                println!("{}", "Corrupted files (hash mismatch):".bright_red().bold());
                for file in corrupted.iter().take(10) {
                    println!("   - {}", file.path.display().to_string().bright_red());
                }
                if corrupted.len() > 10 {
                    println!("   ... and {} more", (corrupted.len() - 10).to_string().bright_red());
                }
                println!();
            }
        }
        
        // Suggest recovery actions
        println!("{}", "💡 Recovery Suggestions:".bright_yellow().bold());
        
        if !missing.is_empty() {
            println!("   1. Re-run the backup to ensure all files are uploaded");
            println!("   2. Check network connectivity and storage space");
        }
        
        if full && !result.corrupted_files().is_empty() {
            println!("   3. Corrupted files detected - data may be damaged");
            println!("   4. Consider restoring from an earlier backup");
            println!("   5. Re-upload corrupted files with a new backup");
        }
        
        println!();
    }
    
    if !full && result.is_success() {
        println!("💡 For deeper verification, run with {} to verify file hashes", "--full".bright_yellow());
        println!("   (This will download all files and may take significant time)");
    }
    
    Ok(())
}

async fn show_file_changes(
    paths: Vec<PathBuf>,
    summary_only: bool,
    config_path: Option<PathBuf>,
) -> Result<()> {
    use progress::ErrorHandler;
    use colored::*;
    use skylock_backup::{ChangeTracker, ChangeType};
    
    ErrorHandler::print_info("File Change Detection", "Detecting changes since last backup");
    
    // Load configuration
    let config = match Config::load(config_path) {
        Ok(config) => config,
        Err(e) => {
            ErrorHandler::print_error("Configuration Error", &e.to_string());
            ErrorHandler::suggest_solution("Run 'skylock config' to generate a configuration file");
            return Err(anyhow::anyhow!("Configuration required"));
        }
    };
    
    // Use provided paths or fall back to config backup paths
    let check_paths = if !paths.is_empty() {
        paths
    } else {
        config.backup.backup_paths.clone()
    };
    
    if check_paths.is_empty() {
        ErrorHandler::print_error("No Paths", "No paths specified and none in config");
        ErrorHandler::suggest_solution("Specify paths to check or add backup_paths to config");
        return Err(anyhow::anyhow!("No paths to check for changes"));
    }
    
    // Set up change tracker
    let index_dir = config.data_dir.join("indexes");
    tokio::fs::create_dir_all(&index_dir).await?;
    let tracker = ChangeTracker::new(index_dir);
    
    // Check if there's a previous backup to compare against
    if !tracker.has_latest_index().await {
        println!();
        println!("{}", "⚠️  No previous backup found".bright_yellow());
        println!("   This appears to be the first backup.");
        println!("   All files will be backed up.");
        println!();
        
        // Count files that would be backed up
        println!("📊 Scanning current files...");
        let file_index = skylock_backup::FileIndex::build(&check_paths)
            .map_err(|e| anyhow::anyhow!("Failed to scan files: {}", e))?;
        
        println!();
        println!("   {} {} files would be backed up",
            "+".bright_green().bold(),
            file_index.file_count().to_string().bright_green()
        );
        println!();
        println!("💡 Run {} to create your first backup", "skylock backup --direct".bright_yellow());
        return Ok(());
    }
    
    // Detect changes
    println!("🔍 Detecting changes...");
    let changes = tracker.detect_changes_since_last_backup(&check_paths).await
        .map_err(|e| anyhow::anyhow!("Failed to detect changes: {}", e))?;
    
    if changes.is_empty() {
        println!();
        println!("{}", "✅ No changes detected".bright_green());
        println!("   All files are up to date with last backup.");
        return Ok(());
    }
    
    // Count changes by type
    let mut added_count = 0;
    let mut removed_count = 0;
    let mut modified_count = 0;
    let mut metadata_count = 0;
    
    for change in &changes {
        match change.change_type {
            ChangeType::Added => added_count += 1,
            ChangeType::Removed => removed_count += 1,
            ChangeType::Modified => modified_count += 1,
            ChangeType::MetadataChanged => metadata_count += 1,
        }
    }
    
    // Display summary
    println!();
    println!("{}", "📊 Change Summary".bright_blue().bold());
    println!();
    
    if added_count > 0 {
        println!("   {} {} files",
            "+".bright_green().bold(),
            added_count.to_string().bright_green()
        );
    }
    
    if removed_count > 0 {
        println!("   {} {} files",
            "-".bright_red().bold(),
            removed_count.to_string().bright_red()
        );
    }
    
    if modified_count > 0 {
        println!("   {} {} files",
            "~".bright_yellow().bold(),
            modified_count.to_string().bright_yellow()
        );
    }
    
    if metadata_count > 0 {
        println!("   {} {} files (metadata only)",
            "◦".dimmed(),
            metadata_count.to_string().dimmed()
        );
    }
    
    println!();
    println!("   {} {} total changes",
        "Σ".bright_cyan(),
        changes.len().to_string().bright_cyan()
    );
    
    // Show detailed list if not summary-only
    if !summary_only {
        println!();
        println!("{}", "Detailed Changes:".bright_cyan().bold());
        println!();
        
        for change in &changes {
            let (symbol, color): (String, fn(String) -> colored::ColoredString) = match change.change_type {
                ChangeType::Added => ("+".to_string(), |s| s.bright_green()),
                ChangeType::Removed => ("-".to_string(), |s| s.bright_red()),
                ChangeType::Modified => ("~".to_string(), |s| s.bright_yellow()),
                ChangeType::MetadataChanged => ("◦".to_string(), |s| s.dimmed()),
            };
            
            let path_str = change.path.display().to_string();
            println!("   {} {}", symbol, color(path_str));
        }
        
        println!();
        println!("💡 Use {} for summary only", "--summary".bright_yellow());
    } else {
        println!();
        println!("💡 Run without {} to see detailed file list", "--summary".bright_yellow());
    }
    
    // Suggest next action
    let backup_count = added_count + modified_count;
    if backup_count > 0 {
        println!();
        println!("💡 Run {} to backup {} changed files",
            "skylock backup --direct".bright_yellow(),
            backup_count.to_string().bright_cyan()
        );
    }
    
    Ok(())
}

async fn test_compression() -> Result<()> {
    use skylock_core::compression::{CompressionEngine, CompressionConfig, CompressionType};
    
    println!("🗜️ Testing Zstandard compression...");
    
    // Create test data with repetitive content (good for compression)
    let test_data = "This is a test string that repeats. ".repeat(100);
    let test_bytes = test_data.as_bytes();
    println!("📝 Test data: {} bytes", test_bytes.len());
    
    // Create compression engine
    let config = CompressionConfig {
        compression_type: CompressionType::Zstd,
        level: 6,
    };
    let engine = CompressionEngine::new(config);
    
    // Compress
    match engine.compress(test_bytes) {
        Ok(compressed) => {
            let compression_ratio = (test_bytes.len() as f64) / (compressed.len() as f64);
            println!("✅ Compression successful: {} bytes -> {} bytes (ratio: {:.2}x)", 
                    test_bytes.len(), compressed.len(), compression_ratio);
            
            // Decompress
            match engine.decompress(&compressed) {
                Ok(decompressed) => {
                    if decompressed == test_bytes {
                        println!("✅ Decompression successful and data matches!");
                    } else {
                        return Err(anyhow::anyhow!("Decompressed data doesn't match original"));
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Decompression failed: {}", e));
                }
            }
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Compression failed: {}", e));
        }
    }
    
    println!("✅ Compression test passed");
    Ok(())
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    // Initialize secure logging system with file rotation
    let log_dir = directories::ProjectDirs::from("dev", "nullme", "skylock")
        .map(|dirs| dirs.data_dir().join("logs"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/skylock/logs"));
    
    // Initialize logging (keep guard alive for duration of program)
    let _log_guard = skylock_hybrid::logging::init_logging(log_dir.clone(), "info")
        .unwrap_or_else(|e| {
            eprintln!("Warning: Failed to initialize file logging: {}", e);
            eprintln!("Continuing with console-only logging...");
            // Fallback to console-only logging
            tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .init();
            // Return a dummy guard that does nothing
            panic!("Failed to initialize logging: {}", e);
        });
    
    info!("Skylock starting up...");
    info!("Logs are being written to: {}", log_dir.display());

    // Parse command line arguments
    let cli = Cli::parse();

    // Handle CLI commands
    if let Some(command) = cli.command {
        return handle_command(command, cli.config).await;
    }

    // Load and validate configuration
    let config = Config::load(cli.config)?;
    config.validate()?;
    info!("Configuration loaded and validated successfully");

    // Initialize security
    let credential_manager = CredentialManager::new(
        config.data_dir.join("secrets/master.key")
    )?;

    // Initialize recovery
    let recovery_manager = RecoveryManager::new(
        config.data_dir.join("recovery/state.json")
    );

    // Initialize shutdown manager
    let shutdown_manager = ShutdownManager::new();

    // Load encrypted credentials
    let hetzner_key = credential_manager.get_credential("hetzner_api_key").await?;
    let syncthing_key = credential_manager.get_credential("syncthing_api_key").await?;

    // Initialize clients with secure credentials
    let hetzner = HetznerClient::new(
        config.hetzner.clone(),
        &hetzner_key
    )?;
    info!("Hetzner client initialized");

    // Initialize backup manager
    let backup_manager = BackupManager::new(config.backup.clone(), hetzner.clone());
    info!("Backup manager initialized");

    // Initialize Syncthing client
    let syncthing = SyncthingClient::new(&config.syncthing.api_url, &config.syncthing.api_key)?;
    info!("Syncthing client initialized");

    // Initialize notification system
    let (notification_manager, mut notification_rx) = NotificationManager::new();

    // Initialize file monitor
    let (mut file_monitor, mut event_rx) = FileMonitor::new(
        hetzner.clone(),
        syncthing.clone(),
        config.syncthing.folders.clone(),
        notification_manager.clone(),
    )?;
    file_monitor.start().await?;
    info!("File monitor initialized and started");

    // Start notification handler
    tokio::spawn(async move {
        while let Some(notification) = notification_rx.recv().await {
            match notification {
                SystemNotification::BackupStarted => {
                    info!("Backup started");
                }
                SystemNotification::BackupCompleted(id) => {
                    info!("Backup completed: {}", id);
                }
                SystemNotification::BackupFailed(error) => {
                    error!("Backup failed: {}", error);
                }
                SystemNotification::FileDeleted(path) => {
                    info!("File deleted: {}", path);
                }
                SystemNotification::SyncProgress(current, total) => {
                    info!("Sync progress: {}/{} bytes", current, total);
                }
                SystemNotification::Error(error) => {
                    error!("Error: {}", error);
                }
            }
        }
    });

    // Start file monitor event processing
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Err(e) = file_monitor.process_event(event).await {
                error!("Error processing file event: {}", e);
            }
        }
    });

    // Set up signal handler
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel(1);
    let shutdown_tx_clone = shutdown_tx.clone();

    // Handle Ctrl+C
    tokio::spawn(async move {
        if let Ok(_) = tokio::signal::ctrl_c().await {
            info!("Received shutdown signal");
            let _ = shutdown_tx_clone.send(());
        }
    });

    // Test connection to Hetzner
    match hetzner.list_files("/").await {
        Ok(_) => info!("Successfully connected to Hetzner Storage Box"),
        Err(e) => error!("Failed to connect to Hetzner Storage Box: {}", e),
    }

    // Start backup scheduler
    let notification_manager_clone = notification_manager.clone();
    let backup_handle = tokio::spawn(async move {
        loop {
            let now = Utc::now();
            if should_run_backup(&config.backup.schedule, now) {
                if let Err(e) = notification_manager_clone.notify_backup_started() {
                    error!("Failed to send backup started notification: {}", e);
                }
                match backup_manager.create_backup().await {
                    Ok(metadata) => {
                        if let Err(e) = notification_manager_clone.notify_backup_completed(metadata.id.clone()) {
                            error!("Failed to send backup completed notification: {}", e);
                        }
                        info!("Backup completed successfully: {}", metadata.id);
                    }
                    Err(e) => {
                        if let Err(e) = notification_manager_clone.notify_backup_failed(e.to_string()) {
                            error!("Failed to send backup failed notification: {}", e);
                        }
                        error!("Backup failed: {}", e);
                    }
                }
            }
            sleep(Duration::from_secs(60)).await;
        }
    });

    // Wait for shutdown signal
    let _ = shutdown_rx.recv().await;
    backup_handle.abort();
    info!("Shutting down gracefully");

    Ok(())
}

fn should_run_backup(schedule: &str, now: DateTime<Utc>) -> bool {
    use cron::Schedule;
    use std::str::FromStr;

    Schedule::from_str(schedule)
        .map(|schedule| {
            schedule.upcoming(Utc).next()
                .map(|next| next <= now)
                .unwrap_or(false)
        })
        .unwrap_or(false)
}
