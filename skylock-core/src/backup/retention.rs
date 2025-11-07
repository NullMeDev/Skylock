use chrono::{DateTime, Utc, Duration};
use crate::Result;
use super::{BackupSet, BackupStorage, RetentionPolicy};

pub struct RetentionManager {
    policy: RetentionPolicy,
}

impl RetentionManager {
    pub fn new(policy: RetentionPolicy) -> Self {
        Self { policy }
    }

    pub async fn prune_backups(&self, storage: &BackupStorage) -> Result<()> {
        let metadata = storage.get_metadata("index").await?
            .unwrap_or_default();

        let mut backups: Vec<BackupSet> = metadata.iter()
            .filter_map(|(_, value)| serde_json::from_str(value).ok())
            .collect();

        // Sort backups by timestamp
        backups.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Mark backups to keep
        let mut keep = vec![false; backups.len()];

        // Keep last N backups
        for i in 0..std::cmp::min(self.policy.keep_last_n, backups.len()) {
            keep[i] = true;
        }

        // Keep daily backups
        self.mark_periodic_backups(&mut keep, &backups, Duration::days(1), self.policy.keep_daily);

        // Keep weekly backups
        self.mark_periodic_backups(&mut keep, &backups, Duration::weeks(1), self.policy.keep_weekly);

        // Keep monthly backups
        self.mark_periodic_backups(&mut keep, &backups, Duration::days(30), self.policy.keep_monthly);

        // Keep yearly backups
        self.mark_periodic_backups(&mut keep, &backups, Duration::days(365), self.policy.keep_yearly);

        // Delete backups that are not marked to keep
        for (i, backup) in backups.iter().enumerate() {
            if !keep[i] {
                // Check minimum age
                let age = Utc::now() - backup.timestamp;
                if age.num_days() >= self.policy.min_age_days as i64 {
                    // Delete backup blocks and metadata
                    if let Some(metadata) = storage.get_metadata(&backup.id).await? {
                        for block_hash in metadata.keys() {
                            storage.delete_block(block_hash).await?;
                        }
                        // Delete backup metadata
                        tokio::fs::remove_file(
                            storage.metadata_path.join(format!("{}.json", backup.id))
                        ).await?;
                    }
                }
            }
        }

        Ok(())
    }

    fn mark_periodic_backups(
        &self,
        keep: &mut [bool],
        backups: &[BackupSet],
        period: Duration,
        count: usize,
    ) {
        let mut last_period = Utc::now();
        let mut kept = 0;

        for (i, backup) in backups.iter().enumerate() {
            if kept >= count {
                break;
            }

            if backup.timestamp <= last_period - period {
                keep[i] = true;
                last_period = backup.timestamp;
                kept += 1;
            }
        }
    }
}
