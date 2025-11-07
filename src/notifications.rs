use anyhow::Result;

#[cfg(target_os = "linux")]
use notify_rust::{Notification, Timeout, Urgency};

/// Send a desktop notification about backup status
pub fn send_notification(title: &str, body: &str, success: bool) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let urgency = if success {
            Urgency::Normal
        } else {
            Urgency::Critical
        };
        
        let timeout = if success {
            Timeout::Milliseconds(5000) // 5 seconds for success
        } else {
            Timeout::Milliseconds(10000) // 10 seconds for errors
        };
        
        Notification::new()
            .summary(title)
            .body(body)
            .icon(if success { "emblem-default" } else { "dialog-error" })
            .appname("Skylock")
            .urgency(urgency)
            .timeout(timeout)
            .show()?;
    }
    
    #[cfg(not(target_os = "linux"))]
    {
        // Fallback for other platforms
        println!("📢 {} - {}", title, body);
    }
    
    Ok(())
}

/// Notify backup started
pub fn notify_backup_started(paths_count: usize) -> Result<()> {
    send_notification(
        "🚀 Skylock Backup Started",
        &format!("Backing up {} path(s)...", paths_count),
        true,
    )
}

/// Notify backup completed
pub fn notify_backup_completed(files: usize, size_mb: f64, duration_secs: u64) -> Result<()> {
    let body = format!(
        "✅ Backed up {} files ({:.2} MB) in {}s",
        files, size_mb, duration_secs
    );
    send_notification("Skylock Backup Complete", &body, true)
}

/// Notify backup failed
pub fn notify_backup_failed(error: &str) -> Result<()> {
    let body = format!("❌ Backup failed: {}", error);
    send_notification("Skylock Backup Failed", &body, false)
}

/// Notify restore started
pub fn notify_restore_started(backup_id: &str) -> Result<()> {
    send_notification(
        "🔄 Skylock Restore Started",
        &format!("Restoring backup: {}", backup_id),
        true,
    )
}

/// Notify restore completed
pub fn notify_restore_completed(files: usize, duration_secs: u64) -> Result<()> {
    let body = format!("✅ Restored {} files in {}s", files, duration_secs);
    send_notification("Skylock Restore Complete", &body, true)
}

/// Notify restore failed
pub fn notify_restore_failed(error: &str) -> Result<()> {
    let body = format!("❌ Restore failed: {}", error);
    send_notification("Skylock Restore Failed", &body, false)
}

/// Notify scheduled backup
pub fn notify_scheduled_backup() -> Result<()> {
    send_notification(
        "⏰ Skylock Scheduled Backup",
        "Automated backup is running...",
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Only run manually to avoid spamming notifications during tests
    fn test_notifications() {
        notify_backup_started(2).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
        
        notify_backup_completed(100, 50.5, 30).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
        
        notify_backup_failed("Network error").unwrap();
    }
}
