use colored::*;
use skylock_core::error_types::{StorageErrorType, NetworkErrorType, SecurityErrorType, BackupErrorType};

/// Enhanced error display with helpful suggestions and recovery steps
pub struct ErrorDisplay;

impl ErrorDisplay {
    /// Display an error with context, help text, and suggested actions
    pub fn display_storage_error(error: &StorageErrorType, context: Option<&str>) {
        match error {
            StorageErrorType::ConnectionFailed(msg) => {
                eprintln!("{}", "╔════════════════════════════════════════╗".red());
                eprintln!("{}", "║  CONNECTION FAILED                     ║".red().bold());
                eprintln!("{}", "╚════════════════════════════════════════╝".red());
                eprintln!();
                eprintln!("{} {}", "  Error:".red().bold(), msg);
                if let Some(ctx) = context {
                    eprintln!("{} {}", "  Context:".yellow(), ctx);
                }
                eprintln!();
                eprintln!("{}", "  Possible Causes:".cyan().bold());
                eprintln!("    • Network connectivity issues");
                eprintln!("    • Incorrect storage box endpoint");
                eprintln!("    • Firewall blocking connection");
                eprintln!("    • Storage box temporarily unavailable");
                eprintln!();
                eprintln!("{}", "  Suggested Actions:".green().bold());
                eprintln!("    1. Check your internet connection: {}", "ping google.com".italic());
                eprintln!("    2. Test Hetzner connection: {}", "skylock test hetzner".italic());
                eprintln!("    3. Verify endpoint in config: {}", "~/.config/skylock-hybrid/config.toml".italic());
                eprintln!("    4. Check Hetzner status: {}", "https://status.hetzner.com".italic());
            }
            
            StorageErrorType::AuthenticationFailed => {
                eprintln!("{}", "╔════════════════════════════════════════╗".red());
                eprintln!("{}", "║  AUTHENTICATION FAILED                 ║".red().bold());
                eprintln!("{}", "╚════════════════════════════════════════╝".red());
                eprintln!();
                eprintln!("{}", "  Your credentials were rejected by the storage box.".red());
                eprintln!();
                eprintln!("{}", "  Suggested Actions:".green().bold());
                eprintln!("    1. Verify username and password in config");
                eprintln!("       {}", "~/.config/skylock-hybrid/config.toml".italic());
                eprintln!("    2. Check for typos in credentials");
                eprintln!("    3. Log into Hetzner web interface to verify credentials");
                eprintln!("    4. If using SSH keys, ensure they're uploaded to Hetzner");
                eprintln!("       {}", "cat ~/.ssh/id_ed25519_hetzner.pub".italic());
            }
            
            StorageErrorType::PermissionDenied => {
                eprintln!("{}", "╔════════════════════════════════════════╗".yellow());
                eprintln!("{}", "║  PERMISSION DENIED                     ║".yellow().bold());
                eprintln!("{}", "╚════════════════════════════════════════╝".yellow());
                eprintln!();
                eprintln!("{}", "  You don't have permission to access this resource.".yellow());
                if let Some(ctx) = context {
                    eprintln!("{} {}", "  Path:".yellow(), ctx);
                }
                eprintln!();
                eprintln!("{}", "  Suggested Actions:".green().bold());
                eprintln!("    1. Check file/directory permissions: {}", "ls -la <path>".italic());
                eprintln!("    2. Verify you have write access to storage box");
                eprintln!("    3. On local files, try running with appropriate permissions");
                eprintln!("    4. Check if file is locked by another process");
            }
            
            StorageErrorType::SpaceExhausted | StorageErrorType::QuotaExceeded => {
                eprintln!("{}", "╔════════════════════════════════════════╗".red());
                eprintln!("{}", "║  STORAGE SPACE EXHAUSTED               ║".red().bold());
                eprintln!("{}", "╚════════════════════════════════════════╝".red());
                eprintln!();
                eprintln!("{}", "  Not enough storage space available.".red());
                eprintln!();
                eprintln!("{}", "  Suggested Actions:".green().bold());
                eprintln!("    1. Check storage box quota in Hetzner web interface");
                eprintln!("    2. Delete old backups: {}", "skylock list".italic());
                eprintln!("    3. Check local disk space: {}", "df -h ~/.local/share/skylock".italic());
                eprintln!("    4. Consider upgrading storage box plan");
                eprintln!("    5. Use compression to reduce backup size");
            }
            
            StorageErrorType::NetworkTimeout => {
                eprintln!("{}", "╔════════════════════════════════════════╗".yellow());
                eprintln!("{}", "║  NETWORK TIMEOUT                       ║".yellow().bold());
                eprintln!("{}", "╚════════════════════════════════════════╝".yellow());
                eprintln!();
                eprintln!("{}", "  The operation took too long and timed out.".yellow());
                eprintln!();
                eprintln!("{}", "  Suggested Actions:".green().bold());
                eprintln!("    1. Check network speed: {}", "speedtest-cli".italic());
                eprintln!("    2. Try again with fewer parallel uploads");
                eprintln!("    3. Check for network congestion");
                eprintln!("    4. Consider using direct mode for large files");
            }
            
            _ => {
                eprintln!("{} {:?}", "Storage Error:".red().bold(), error);
                if let Some(ctx) = context {
                    eprintln!("{} {}", "Context:".yellow(), ctx);
                }
            }
        }
        
        eprintln!();
        eprintln!("{}", "  Need more help? Check the logs:".cyan());
        eprintln!("    {}", "tail -50 ~/.local/share/skylock/logs/skylock.log".italic());
        eprintln!();
    }
    
    /// Display backup-related errors with recovery suggestions
    pub fn display_backup_error(error: &BackupErrorType, context: Option<&str>) {
        match error {
            BackupErrorType::BackupFailed => {
                eprintln!("{}", "╔════════════════════════════════════════╗".red());
                eprintln!("{}", "║  BACKUP FAILED                         ║".red().bold());
                eprintln!("{}", "╚════════════════════════════════════════╝".red());
                eprintln!();
                eprintln!("{}", "  The backup operation failed to complete.".red());
                if let Some(ctx) = context {
                    eprintln!("{} {}", "  Reason:".yellow(), ctx);
                }
                eprintln!();
                eprintln!("{}", "  Suggested Actions:".green().bold());
                eprintln!("    1. Check log files for detailed error information");
                eprintln!("    2. Verify storage box connectivity: {}", "skylock test hetzner".italic());
                eprintln!("    3. Ensure sufficient disk space locally and remotely");
                eprintln!("    4. Try backup with --direct flag for more reliable uploads");
                eprintln!("    5. Check if files are accessible: {}", "ls -la <backup_path>".italic());
            }
            
            BackupErrorType::CorruptBackup => {
                eprintln!("{}", "╔════════════════════════════════════════╗".red().bold());
                eprintln!("{}", "║  CORRUPT BACKUP DETECTED               ║".red().bold());
                eprintln!("{}", "╚════════════════════════════════════════╝".red().bold());
                eprintln!();
                eprintln!("{}", "  ⚠️  WARNING: This backup appears to be corrupted!".red().bold());
                eprintln!();
                eprintln!("{}", "  Suggested Actions:".green().bold());
                eprintln!("    1. Do NOT delete this backup yet");
                eprintln!("    2. Try verifying the backup: {}", "skylock verify <backup_id>".italic());
                eprintln!("    3. Check if partial restore is possible");
                eprintln!("    4. Create a new backup immediately");
                eprintln!("    5. Consider implementing backup verification in your workflow");
            }
            
            _ => {
                eprintln!("{} {:?}", "Backup Error:".red().bold(), error);
                if let Some(ctx) = context {
                    eprintln!("{} {}", "Context:".yellow(), ctx);
                }
            }
        }
        
        eprintln!();
    }
    
    /// Display security-related errors
    pub fn display_security_error(error: &SecurityErrorType, context: Option<&str>) {
        match error {
            SecurityErrorType::EncryptionFailed | SecurityErrorType::DecryptionFailed => {
                eprintln!("{}", "╔════════════════════════════════════════╗".red().bold());
                eprintln!("{}", "║  ENCRYPTION/DECRYPTION FAILED          ║".red().bold());
                eprintln!("{}", "╚════════════════════════════════════════╝".red().bold());
                eprintln!();
                eprintln!("{}", "  Failed to encrypt or decrypt data.".red());
                if let Some(ctx) = context {
                    eprintln!("{} {}", "  Details:".yellow(), ctx);
                }
                eprintln!();
                eprintln!("{}", "  Possible Causes:".cyan().bold());
                eprintln!("    • Incorrect encryption key");
                eprintln!("    • Corrupted encrypted data");
                eprintln!("    • Key has been rotated");
                eprintln!();
                eprintln!("{}", "  Suggested Actions:".green().bold());
                eprintln!("    1. Verify encryption key in config is correct");
                eprintln!("    2. If key was rotated, use the original key for old backups");
                eprintln!("    3. Check if backup was created with different key");
                eprintln!("    4. Ensure encryption key is exactly as generated (no spaces/newlines)");
                eprintln!();
                eprintln!("{}", "  ⚠️  IMPORTANT: Save your encryption key securely!".yellow().bold());
                eprintln!("     Without it, your backups cannot be restored.");
            }
            
            SecurityErrorType::InvalidCredentials => {
                eprintln!("{}", "╔════════════════════════════════════════╗".red());
                eprintln!("{}", "║  INVALID CREDENTIALS                   ║".red().bold());
                eprintln!("{}", "╚════════════════════════════════════════╝".red());
                eprintln!();
                eprintln!("{}", "  The provided credentials are invalid.".red());
                eprintln!();
                eprintln!("{}", "  Suggested Actions:".green().bold());
                eprintln!("    1. Check credentials in: {}", "~/.config/skylock-hybrid/config.toml".italic());
                eprintln!("    2. Verify username and password/API key are correct");
                eprintln!("    3. Ensure no extra spaces or special characters");
                eprintln!("    4. Try regenerating credentials in Hetzner control panel");
            }
            
            _ => {
                eprintln!("{} {:?}", "Security Error:".red().bold(), error);
                if let Some(ctx) = context {
                    eprintln!("{} {}", "Context:".yellow(), ctx);
                }
            }
        }
        
        eprintln!();
    }
    
    /// Display network-related errors
    pub fn display_network_error(error: &NetworkErrorType, context: Option<&str>) {
        match error {
            NetworkErrorType::ConnectionFailed => {
                eprintln!("{}", "╔════════════════════════════════════════╗".red());
                eprintln!("{}", "║  NETWORK CONNECTION FAILED             ║".red().bold());
                eprintln!("{}", "╚════════════════════════════════════════╝".red());
                eprintln!();
                eprintln!("{}", "  Unable to establish network connection.".red());
                if let Some(ctx) = context {
                    eprintln!("{} {}", "  Details:".yellow(), ctx);
                }
                eprintln!();
                eprintln!("{}", "  Quick Diagnostics:".cyan().bold());
                eprintln!("    • Internet: {}", "ping 8.8.8.8".italic());
                eprintln!("    • DNS: {}", "ping google.com".italic());
                eprintln!("    • Hetzner: {}", "ping your-storagebox.de".italic());
                eprintln!();
                eprintln!("{}", "  Suggested Actions:".green().bold());
                eprintln!("    1. Check your internet connection");
                eprintln!("    2. Verify firewall isn't blocking connections");
                eprintln!("    3. Try switching networks (WiFi/Ethernet)");
                eprintln!("    4. Check if VPN is interfering");
            }
            
            NetworkErrorType::TimeoutError => {
                eprintln!("{}", "╔════════════════════════════════════════╗".yellow());
                eprintln!("{}", "║  OPERATION TIMED OUT                   ║".yellow().bold());
                eprintln!("{}", "╚════════════════════════════════════════╝".yellow());
                eprintln!();
                eprintln!("{}", "  The operation took too long to complete.".yellow());
                eprintln!();
                eprintln!("{}", "  Suggested Actions:".green().bold());
                eprintln!("    1. Check network stability");
                eprintln!("    2. Reduce concurrent upload threads");
                eprintln!("    3. Try uploading smaller batches of files");
                eprintln!("    4. Increase timeout in configuration if appropriate");
            }
            
            _ => {
                eprintln!("{} {:?}", "Network Error:".red().bold(), error);
                if let Some(ctx) = context {
                    eprintln!("{} {}", "Context:".yellow(), ctx);
                }
            }
        }
        
        eprintln!();
    }
    
    /// Display a generic error with basic formatting
    pub fn display_generic_error(title: &str, error: &str, suggestions: Option<Vec<&str>>) {
        eprintln!("{}", "╔════════════════════════════════════════╗".red());
        eprintln!("{}", format!("║  {:<38} ║", title.to_uppercase()).red().bold());
        eprintln!("{}", "╚════════════════════════════════════════╝".red());
        eprintln!();
        eprintln!("{} {}", "  Error:".red().bold(), error);
        
        if let Some(suggestions) = suggestions {
            eprintln!();
            eprintln!("{}", "  Suggested Actions:".green().bold());
            for (i, suggestion) in suggestions.iter().enumerate() {
                eprintln!("    {}. {}", i + 1, suggestion);
            }
        }
        
        eprintln!();
        eprintln!("{}", "  For detailed logs:".cyan());
        eprintln!("    {}", "tail -100 ~/.local/share/skylock/logs/skylock.log".italic());
        eprintln!();
    }
}
