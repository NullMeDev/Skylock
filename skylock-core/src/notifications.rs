use skylock_core::Result;
use std::sync::mpsc;
use tokio::sync::broadcast;
use tracing::info;

#[derive(Debug, Clone)]
pub enum SystemNotification {
    BackupStarted,
    BackupCompleted(String),
    BackupFailed(String),
    FileDeleted(String),
    SyncProgress(u64, u64),
    Error(String),
}

pub struct NotificationManager {
    tx: broadcast::Sender<SystemNotification>,
}

impl NotificationManager {
    pub fn new() -> (Self, broadcast::Receiver<SystemNotification>) {
        let (tx, rx) = broadcast::channel(100);
        (Self { tx }, rx)
    }

    pub fn notify(&self, notification: SystemNotification) -> Result<()> {
        self.tx.send(notification).map_err(|e| {
            skylock_core::SkylockError::Config(format!("Failed to send notification: {}", e))
        })?;
        Ok(())
    }

    pub fn notify_backup_started(&self) -> Result<()> {
        self.notify(SystemNotification::BackupStarted)
    }

    pub fn notify_backup_completed(&self, backup_id: String) -> Result<()> {
        self.notify(SystemNotification::BackupCompleted(backup_id))
    }

    pub fn notify_backup_failed(&self, error: String) -> Result<()> {
        self.notify(SystemNotification::Error(error))
    }

    pub fn notify_file_deleted(&self, path: String) -> Result<()> {
        self.notify(SystemNotification::FileDeleted(path))
    }

    pub fn notify_sync_progress(&self, current: u64, total: u64) -> Result<()> {
        self.notify(SystemNotification::SyncProgress(current, total))
    }
}
