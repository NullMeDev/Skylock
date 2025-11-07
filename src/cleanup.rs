use anyhow::Result;
use std::path::PathBuf;
use skylock_core::Config;
use skylock_backup::{RetentionPolicy, RetentionManager};
use colored::*;

use crate::progress::{ProgressReporter, ErrorHandler};

pub async fn perform_cleanup(dry_run: bool, force: bool, config_path: Option<PathBuf>) -> Result<()> {
    use std::io::{self, Write};
    
    let progress = ProgressReporter::new();
    
    if dry_run {
        ErrorHandler::print_info("Cleanup Mode", "DRY RUN - No backups will be deleted");
    } else {
        ErrorHandler::print_info("Cleanup Mode", "Will delete old backups based on retention policy");
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
    
    // Create direct upload backup manager
    let direct_backup = skylock_backup::DirectUploadBackup::new(config.clone(), hetzner_client, encryption);
    
    // List all backups
    let list_spinner = progress.create_spinner("Fetching backup list...");
    let manifests = direct_backup.list_backups().await?;
    progress.finish_with_message(&list_spinner, &format!("Found {} backups", manifests.len()));
    
    if manifests.is_empty() {
        println!();
        ErrorHandler::print_info("No Backups", "No backups found to clean up");
        return Ok(());
    }
    
    // Create retention policy from config
    let retention_policy = RetentionPolicy {
        keep_last: Some(30),
        keep_days: Some(config.backup.retention_days as u32),
        gfs: None, // Can be configured later
        minimum_keep: 3,
    };
    
    let retention_manager = RetentionManager::new(retention_policy);
    
    // Show retention policy
    println!();
    println!("{}", "📋 Retention Policy:".bright_blue().bold());
    println!("   {}", retention_manager.summarize());
    println!();
    
    // Calculate deletions
    let to_delete = retention_manager.calculate_deletions(&manifests);
    
    if to_delete.is_empty() {
        println!();
        ErrorHandler::print_info("All Good", "No backups need to be deleted");
        println!("   All {} backups meet retention criteria", manifests.len());
        return Ok(());
    }
    
    // Show what will be deleted
    println!("{}", "🗑️  Backups to Delete:".bright_yellow().bold());
    println!();
    
    let mut total_size_to_delete = 0u64;
    for backup_id in &to_delete {
        if let Some(manifest) = manifests.iter().find(|m| &m.backup_id == backup_id) {
            let age_days = (chrono::Utc::now() - manifest.timestamp).num_days();
            let size_mb = manifest.total_size as f64 / 1024.0 / 1024.0;
            total_size_to_delete += manifest.total_size;
            
            println!("   • {} - {:.2} MB, {} files, {} days old",
                backup_id.bright_red(),
                size_mb,
                manifest.file_count,
                age_days
            );
        }
    }
    
    println!();
    println!("   Total to delete: {} backups, {:.2} MB",
        to_delete.len(),
        total_size_to_delete as f64 / 1024.0 / 1024.0
    );
    println!("   Will keep: {} backups", manifests.len() - to_delete.len());
    
    if dry_run {
        println!();
        ErrorHandler::print_info("Dry Run Complete", "No backups were deleted");
        println!("   Run without --dry-run to actually delete these backups");
        return Ok(());
    }
    
    // Confirmation prompt
    if !force {
        println!();
        print!("{} ", "⚠️  Are you sure you want to delete these backups? (yes/no):".bright_yellow().bold());
        io::stdout().flush()?;
        
        let mut response = String::new();
        io::stdin().read_line(&mut response)?;
        
        if response.trim().to_lowercase() != "yes" {
            println!();
            ErrorHandler::print_info("Cancelled", "Cleanup cancelled by user");
            return Ok(());
        }
    }
    
    // Delete backups
    println!();
    println!("{}", "🗑️  Deleting backups...".bright_red().bold());
    println!();
    
    let mut deleted_count = 0;
    let mut failed_count = 0;
    
    for backup_id in &to_delete {
        print!("   Deleting {}... ", backup_id);
        io::stdout().flush()?;
        
        match direct_backup.delete_backup(backup_id).await {
            Ok(_) => {
                println!("{}", "✓".bright_green());
                deleted_count += 1;
            }
            Err(e) => {
                println!("{} - {}", "✗".bright_red(), e);
                failed_count += 1;
            }
        }
    }
    
    println!();
    if failed_count == 0 {
        ErrorHandler::print_success("Cleanup Complete", &format!("Deleted {} backups", deleted_count));
    } else {
        ErrorHandler::print_warning("Cleanup Partial", 
            &format!("Deleted {} backups, {} failed", deleted_count, failed_count));
    }
    
    Ok(())
}
