use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use console::{style, Term, Key};
use dialoguer::{Confirm, Select, Input, Password, MultiSelect, theme::ColorfulTheme};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Modifier},
    text::{Span, Spans},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Tabs},
    Terminal,
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use tokio::time::{sleep, Duration};
use chrono::{DateTime, Utc};

#[derive(Parser)]
#[command(name = "skylock")]
#[command(author = "Skylock Team")]
#[command(version = "0.1.0")]
#[command(about = "Secure, encrypted, deduplicated backup solution")]
#[command(long_about = "Skylock Hybrid is a cross-platform backup system with military-grade encryption, adaptive compression, and intelligent deduplication")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Configuration file path
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    /// Verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Quiet mode (minimal output)
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// JSON output format
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize Skylock configuration and encryption keys
    Init {
        /// Force reinitialize even if already configured
        #[arg(long)]
        force: bool,
        
        /// Interactive configuration wizard
        #[arg(long)]
        wizard: bool,
    },
    
    /// Create a backup
    Backup {
        /// Paths to backup
        paths: Vec<PathBuf>,
        
        /// Backup name/label
        #[arg(short, long)]
        name: Option<String>,
        
        /// Exclude patterns
        #[arg(short, long)]
        exclude: Vec<String>,
        
        /// Dry run (don't actually backup)
        #[arg(long)]
        dry_run: bool,
        
        /// Force full backup (ignore incremental)
        #[arg(long)]
        full: bool,
    },
    
    /// List backups
    List {
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
        
        /// Filter by backup name pattern
        #[arg(short, long)]
        filter: Option<String>,
        
        /// Limit number of results
        #[arg(short, long)]
        limit: Option<usize>,
    },
    
    /// Restore files from backup
    Restore {
        /// Backup ID or name
        backup: String,
        
        /// Destination path
        #[arg(short, long)]
        destination: PathBuf,
        
        /// Specific files/patterns to restore
        #[arg(short, long)]
        files: Vec<String>,
        
        /// Restore to original locations
        #[arg(long)]
        in_place: bool,
        
        /// Overwrite existing files
        #[arg(long)]
        overwrite: bool,
    },
    
    /// Verify backup integrity
    Verify {
        /// Backup ID or name (verify all if not specified)
        backup: Option<String>,
        
        /// Quick verification (metadata only)
        #[arg(short, long)]
        quick: bool,
        
        /// Deep verification (decrypt and validate all data)
        #[arg(long)]
        deep: bool,
    },
    
    /// Show system status and statistics
    Status {
        /// Refresh interval in seconds for live monitoring
        #[arg(short, long)]
        watch: Option<u64>,
        
        /// Show detailed statistics
        #[arg(short, long)]
        detailed: bool,
    },
    
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },
    
    /// Run as daemon/service
    Daemon {
        /// Fork to background
        #[arg(short, long)]
        fork: bool,
        
        /// PID file path
        #[arg(long)]
        pid_file: Option<PathBuf>,
    },
    
    /// Interactive terminal UI
    Tui,
    
    /// Web dashboard server
    Web {
        /// Port to bind to
        #[arg(short, long, default_value = "8080")]
        port: u16,
        
        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        
        /// Open browser automatically
        #[arg(long)]
        open: bool,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current configuration
    Show,
    
    /// Edit configuration interactively
    Edit,
    
    /// Validate configuration
    Validate,
    
    /// Reset to defaults
    Reset {
        #[arg(long)]
        confirm: bool,
    },
    
    /// Export configuration
    Export {
        /// Output file path
        #[arg(short, long)]
        output: PathBuf,
    },
    
    /// Import configuration
    Import {
        /// Input file path
        #[arg(short, long)]
        input: PathBuf,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupProgress {
    pub total_files: u64,
    pub processed_files: u64,
    pub total_bytes: u64,
    pub processed_bytes: u64,
    pub compression_ratio: f64,
    pub deduplication_ratio: f64,
    pub current_file: String,
    pub speed_mbps: f64,
    pub eta_seconds: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemStatus {
    pub version: String,
    pub uptime_seconds: u64,
    pub total_backups: u32,
    pub total_files: u64,
    pub total_size_bytes: u64,
    pub deduplicated_size_bytes: u64,
    pub last_backup: Option<DateTime<Utc>>,
    pub next_scheduled_backup: Option<DateTime<Utc>>,
    pub storage_health: StorageHealth,
    pub recent_errors: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StorageHealth {
    pub status: String, // "healthy", "warning", "error"
    pub free_space_bytes: u64,
    pub total_space_bytes: u64,
    pub connection_status: String,
    pub last_verified: Option<DateTime<Utc>>,
}

pub struct CliInterface {
    progress_bars: MultiProgress,
    theme: ColorfulTheme,
}

impl CliInterface {
    pub fn new() -> Self {
        Self {
            progress_bars: MultiProgress::new(),
            theme: ColorfulTheme::default(),
        }
    }

    pub async fn run_interactive_init(&self) -> Result<()> {
        println!("{}", style("🔐 Skylock Hybrid Setup Wizard").cyan().bold());
        println!();
        
        // Welcome message
        let welcome = format!(
            "Welcome to Skylock! This wizard will help you set up secure backups with:\n\
             {} Military-grade encryption (AES-256-GCM + RSA-4096)\n\
             {} Intelligent compression (LZ4, ZSTD, Brotli)\n\
             {} Advanced deduplication for space savings\n\
             {} Cross-platform support (Linux, Windows, macOS)",
            style("•").green(), style("•").green(), style("•").green(), style("•").green()
        );
        println!("{}", welcome);
        println!();

        // Confirm to proceed
        if !Confirm::with_theme(&self.theme)
            .with_prompt("Ready to begin setup?")
            .default(true)
            .interact()? {
            println!("Setup cancelled.");
            return Ok(());
        }

        // Master password setup
        println!("{}", style("\n📋 Step 1: Master Password").yellow().bold());
        let password = self.setup_master_password()?;
        
        // Backup locations
        println!("{}", style("\n📁 Step 2: Backup Sources").yellow().bold());
        let backup_paths = self.setup_backup_paths()?;
        
        // Storage configuration  
        println!("{}", style("\n☁️  Step 3: Storage Configuration").yellow().bold());
        let storage_config = self.setup_storage_config()?;
        
        // Schedule configuration
        println!("{}", style("\n⏰ Step 4: Backup Schedule").yellow().bold());
        let schedule_config = self.setup_schedule_config()?;
        
        // Summary and confirmation
        self.show_config_summary(&backup_paths, &storage_config, &schedule_config)?;
        
        if Confirm::with_theme(&self.theme)
            .with_prompt("Save configuration and initialize encryption keys?")
            .default(true)
            .interact()? {
            
            self.initialize_with_progress(&password, backup_paths, storage_config, schedule_config).await?;
            
            println!("\n{}", style("✅ Setup complete! Skylock is ready to protect your data.").green().bold());
            println!("\nNext steps:");
            println!("  {} Run your first backup: {}", style("•").cyan(), style("skylock backup").bold());
            println!("  {} Start the daemon: {}", style("•").cyan(), style("skylock daemon").bold());
            println!("  {} Open web dashboard: {}", style("•").cyan(), style("skylock web").bold());
        }
        
        Ok(())
    }

    fn setup_master_password(&self) -> Result<String> {
        loop {
            let password = Password::with_theme(&self.theme)
                .with_prompt("Enter master password (min 12 characters)")
                .interact()?;
                
            if password.len() < 12 {
                println!("{}", style("Password must be at least 12 characters long").red());
                continue;
            }
            
            let confirm = Password::with_theme(&self.theme)
                .with_prompt("Confirm master password")
                .interact()?;
                
            if password != confirm {
                println!("{}", style("Passwords don't match. Please try again.").red());
                continue;
            }
            
            // Password strength check
            let strength = self.check_password_strength(&password);
            println!("Password strength: {}", strength);
            
            if Confirm::with_theme(&self.theme)
                .with_prompt("Use this password?")
                .default(true)
                .interact()? {
                return Ok(password);
            }
        }
    }

    fn setup_backup_paths(&self) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        
        println!("Select directories to backup:");
        
        let common_paths = vec![
            format!("{}/Documents", std::env::var("HOME")?),
            format!("{}/Pictures", std::env::var("HOME")?),
            format!("{}/Desktop", std::env::var("HOME")?),
            format!("{}/Downloads", std::env::var("HOME")?),
            "/etc".to_string(),
            "Custom path...".to_string(),
        ];
        
        let selection = MultiSelect::with_theme(&self.theme)
            .with_prompt("Choose paths (use Space to select, Enter to confirm)")
            .items(&common_paths)
            .interact()?;
            
        for &i in &selection {
            if i == common_paths.len() - 1 {
                // Custom path
                let custom = Input::<String>::with_theme(&self.theme)
                    .with_prompt("Enter custom path")
                    .interact()?;
                paths.push(PathBuf::from(custom));
            } else {
                paths.push(PathBuf::from(&common_paths[i]));
            }
        }
        
        Ok(paths)
    }

    fn setup_storage_config(&self) -> Result<StorageConfig> {
        let options = vec![
            "Local storage only",
            "Hetzner Storage Box (SFTP)",
            "Hetzner Storage Box (WebDAV)", 
            "Custom SFTP server",
        ];
        
        let selection = Select::with_theme(&self.theme)
            .with_prompt("Choose storage backend")
            .items(&options)
            .default(0)
            .interact()?;
            
        match selection {
            0 => Ok(StorageConfig::Local {
                path: Input::<String>::with_theme(&self.theme)
                    .with_prompt("Backup storage path")
                    .default(format!("{}/skylock-backups", std::env::var("HOME")?))
                    .interact()?,
            }),
            1 => self.setup_hetzner_sftp(),
            2 => self.setup_hetzner_webdav(),
            3 => self.setup_custom_sftp(),
            _ => unreachable!(),
        }
    }

    fn setup_schedule_config(&self) -> Result<ScheduleConfig> {
        let options = vec![
            "Real-time (backup on file changes)",
            "Hourly",
            "Daily at midnight", 
            "Daily at specific time",
            "Weekly",
            "Custom cron schedule",
            "Manual only",
        ];
        
        let selection = Select::with_theme(&self.theme)
            .with_prompt("Choose backup schedule")
            .items(&options)
            .default(2)
            .interact()?;
            
        match selection {
            0 => Ok(ScheduleConfig::RealTime),
            1 => Ok(ScheduleConfig::Cron("0 * * * *".to_string())),
            2 => Ok(ScheduleConfig::Cron("0 0 * * *".to_string())),
            3 => {
                let hour = Input::<u8>::with_theme(&self.theme)
                    .with_prompt("Hour (0-23)")
                    .default(2)
                    .interact()?;
                Ok(ScheduleConfig::Cron(format!("0 {} * * *", hour)))
            },
            4 => Ok(ScheduleConfig::Cron("0 2 * * 0".to_string())),
            5 => {
                let cron = Input::<String>::with_theme(&self.theme)
                    .with_prompt("Enter cron schedule (e.g., '0 2 * * *')")
                    .interact()?;
                Ok(ScheduleConfig::Cron(cron))
            },
            6 => Ok(ScheduleConfig::Manual),
            _ => unreachable!(),
        }
    }

    pub async fn show_backup_progress(&self, mut progress_rx: tokio::sync::mpsc::Receiver<BackupProgress>) -> Result<()> {
        let pb = self.progress_bars.add(ProgressBar::new(100));
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>7}/{len:7} files ({percent}%) {msg}")?
            .progress_chars("#>-"));

        let detail_pb = self.progress_bars.add(ProgressBar::new(100));
        detail_pb.set_style(ProgressStyle::default_bar()
            .template("                  {wide_bar:.yellow/blue} {bytes}/{total_bytes} ({bytes_per_sec}) ETA: {eta}")?
            .progress_chars("=>-"));

        while let Some(progress) = progress_rx.recv().await {
            let file_percent = if progress.total_files > 0 {
                (progress.processed_files * 100) / progress.total_files
            } else { 0 };
            
            let byte_percent = if progress.total_bytes > 0 {
                (progress.processed_bytes * 100) / progress.total_bytes
            } else { 0 };

            pb.set_position(file_percent);
            pb.set_length(100);
            pb.set_message(format!("Compressing {}", progress.current_file));

            detail_pb.set_position(byte_percent);
            detail_pb.set_length(100);

            if let Some(eta) = progress.eta_seconds {
                pb.set_message(format!("ETA: {}m{}s - {}", eta / 60, eta % 60, progress.current_file));
            }
        }

        pb.finish_with_message("Backup completed!");
        detail_pb.finish();
        
        Ok(())
    }

    pub async fn launch_tui(&self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Create app state
        let mut app = TuiApp::new().await?;
        
        let res = self.run_tui_loop(&mut terminal, &mut app).await;

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        if let Err(err) = res {
            println!("{:?}", err);
        }

        Ok(())
    }

    async fn run_tui_loop<B>(&self, terminal: &mut Terminal<B>, app: &mut TuiApp) -> Result<()>
    where
        B: tui::backend::Backend,
    {
        loop {
            terminal.draw(|f| self.render_tui(f, app))?;

            if event::poll(Duration::from_millis(250))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Tab => app.next_tab(),
                        KeyCode::BackTab => app.previous_tab(),
                        KeyCode::Up => app.scroll_up(),
                        KeyCode::Down => app.scroll_down(),
                        KeyCode::Enter => app.handle_enter().await?,
                        _ => {}
                    }
                }
            }

            app.update().await?;
        }
    }

    fn render_tui<B>(&self, f: &mut tui::Frame<B>, app: &TuiApp)
    where
        B: tui::backend::Backend,
    {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Min(0),     // Main content
                Constraint::Length(3),  // Footer
            ].as_ref())
            .split(f.size());

        // Header with title and tabs
        let titles = app.tab_titles().iter().cloned().map(Spans::from).collect();
        let tabs = Tabs::new(titles)
            .block(Block::default().borders(Borders::ALL).title("Skylock"))
            .select(app.current_tab())
            .style(Style::default().fg(Color::Cyan))
            .highlight_style(Style::default().add_modifier(Modifier::BOLD).bg(Color::Black));
        f.render_widget(tabs, chunks[0]);

        // Main content area
        match app.current_tab() {
            0 => self.render_status_tab(f, chunks[1], app),
            1 => self.render_backups_tab(f, chunks[1], app),
            2 => self.render_restore_tab(f, chunks[1], app),
            3 => self.render_monitoring_tab(f, chunks[1], app),
            _ => {}
        }

        // Footer with help
        let help = Paragraph::new("Press 'q' to quit, Tab/Shift+Tab to switch tabs, ↑/↓ to navigate")
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .style(Style::default().fg(Color::Gray));
        f.render_widget(help, chunks[2]);
    }

    fn check_password_strength(&self, password: &str) -> String {
        let mut score = 0;
        
        if password.len() >= 12 { score += 1; }
        if password.len() >= 16 { score += 1; }
        if password.chars().any(|c| c.is_lowercase()) { score += 1; }
        if password.chars().any(|c| c.is_uppercase()) { score += 1; }
        if password.chars().any(|c| c.is_numeric()) { score += 1; }
        if password.chars().any(|c| !c.is_alphanumeric()) { score += 1; }
        
        match score {
            0..=2 => style("Weak").red().to_string(),
            3..=4 => style("Medium").yellow().to_string(),
            5..=6 => style("Strong").green().to_string(),
            _ => style("Very Strong").bright_green().to_string(),
        }
    }

    // Additional helper methods would go here...
}

#[derive(Debug, Clone)]
pub enum StorageConfig {
    Local { path: String },
    HetznerSftp { endpoint: String, username: String, password: String },
    HetznerWebdav { endpoint: String, username: String, password: String },
    CustomSftp { host: String, port: u16, username: String, password: String, path: String },
}

#[derive(Debug, Clone)]
pub enum ScheduleConfig {
    RealTime,
    Manual,
    Cron(String),
}

pub struct TuiApp {
    current_tab: usize,
    // Add state fields for different tabs
}

impl TuiApp {
    async fn new() -> Result<Self> {
        Ok(Self {
            current_tab: 0,
        })
    }

    fn tab_titles(&self) -> Vec<&str> {
        vec!["Status", "Backups", "Restore", "Monitoring"]
    }

    fn current_tab(&self) -> usize {
        self.current_tab
    }

    fn next_tab(&mut self) {
        self.current_tab = (self.current_tab + 1) % self.tab_titles().len();
    }

    fn previous_tab(&mut self) {
        if self.current_tab > 0 {
            self.current_tab -= 1;
        } else {
            self.current_tab = self.tab_titles().len() - 1;
        }
    }

    fn scroll_up(&mut self) {
        // Implementation depends on current tab
    }

    fn scroll_down(&mut self) {
        // Implementation depends on current tab
    }

    async fn handle_enter(&mut self) -> Result<()> {
        // Handle enter key based on current tab and selection
        Ok(())
    }

    async fn update(&mut self) -> Result<()> {
        // Update app state (refresh data, etc.)
        Ok(())
    }
}