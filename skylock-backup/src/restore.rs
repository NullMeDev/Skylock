use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use skylock_core::{Result, SkylockError};
use std::path::{Path, PathBuf};
use tokio::fs;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePoint {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub snapshot_path: PathBuf,
    pub backup_type: BackupType,
    pub size: u64,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupType {
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Manual,
}

pub struct RestoreManager {
    base_path: PathBuf,
    retention_policy: RetentionPolicy,
    restore_points: Vec<RestorePoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub hourly_count: u32,
    pub daily_count: u32,
    pub weekly_count: u32,
    pub monthly_count: u32,
    pub min_free_space_gb: u64,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            hourly_count: 24,    // Keep last 24 hours
            daily_count: 7,      // Keep last 7 days
            weekly_count: 4,     // Keep last 4 weeks
            monthly_count: 12,   // Keep last 12 months
            min_free_space_gb: 50, // Minimum 50GB free space
        }
    }
}

impl RestoreManager {
    pub async fn new(base_path: PathBuf) -> Result<Self> {
        let mut manager = Self {
            base_path,
            retention_policy: RetentionPolicy::default(),
            restore_points: Vec::new(),
        };
        manager.load_restore_points().await?;
        Ok(manager)
    }

    pub async fn create_restore_point(
        &mut self,
        description: String,
        backup_type: BackupType,
        vss_snapshot: &Path,
        encrypted: bool,
    ) -> Result<RestorePoint> {
        let id = Uuid::new_v4().to_string();
        let timestamp = Utc::now();

        // Create restore point directory
        let restore_path = self.get_restore_point_path(&id);
        fs::create_dir_all(&restore_path).await?;

        // Copy VSS snapshot to restore point
        let snapshot_path = restore_path.join("snapshot");
        fs::copy(vss_snapshot, &snapshot_path).await?;

        // Get size
        let size = fs::metadata(&snapshot_path).await?.len();

        let restore_point = RestorePoint {
            id,
            timestamp,
            description,
            snapshot_path,
            backup_type,
            size,
            encrypted,
        };

        // Save restore point metadata
        self.restore_points.push(restore_point.clone());
        self.save_restore_points().await?;

        // Apply retention policy
        self.apply_retention_policy().await?;

        Ok(restore_point)
    }

    pub async fn restore_from_point(&self, point_id: &str, target_path: &Path) -> Result<()> {
        let restore_point = self.restore_points
            .iter()
            .find(|p| p.id == point_id)
            .ok_or_else(|| SkylockError::Generic("Restore point not found".into()))?;

        // Verify restore point exists
        if !restore_point.snapshot_path.exists() {
            return Err(SkylockError::Generic("Restore point snapshot not found".into()));
        }

        // Create target directory if needed
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Restore files
        fs::copy(&restore_point.snapshot_path, target_path).await?;

        Ok(())
    }

    async fn apply_retention_policy(&mut self) -> Result<()> {
        let now = Utc::now();

        // Sort restore points by timestamp
        self.restore_points.sort_by_key(|p| p.timestamp);

        // Keep required number of each backup type
        let mut to_remove = Vec::new();

        // Process each backup type
        for backup_type in [BackupType::Hourly, BackupType::Daily, BackupType::Weekly, BackupType::Monthly] {
            let max_count = match backup_type {
                BackupType::Hourly => self.retention_policy.hourly_count,
                BackupType::Daily => self.retention_policy.daily_count,
                BackupType::Weekly => self.retention_policy.weekly_count,
                BackupType::Monthly => self.retention_policy.monthly_count,
                BackupType::Manual => continue, // Don't auto-remove manual restore points
            };

            let points: Vec<_> = self.restore_points
                .iter()
                .filter(|p| p.backup_type == backup_type)
                .collect();

            if points.len() > max_count as usize {
                to_remove.extend(
                    points
                        .iter()
                        .take(points.len() - max_count as usize)
                        .map(|p| p.id.clone())
                );
            }
        }

        // Remove old restore points
        for id in to_remove {
            self.remove_restore_point(&id).await?;
        }

        // Check free space
        if let Ok(available) = fs::metadata(&self.base_path).await?.len() {
            let min_required = self.retention_policy.min_free_space_gb * 1024 * 1024 * 1024;
            if available < min_required {
                // Remove oldest non-manual restore points until we have enough space
                let mut points: Vec<_> = self.restore_points
                    .iter()
                    .filter(|p| matches!(p.backup_type,
                        BackupType::Hourly |
                        BackupType::Daily |
                        BackupType::Weekly |
                        BackupType::Monthly))
                    .collect();

                points.sort_by_key(|p| p.timestamp);

                for point in points {
                    self.remove_restore_point(&point.id).await?;

                    if let Ok(available) = fs::metadata(&self.base_path).await?.len() {
                        if available >= min_required {
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn remove_restore_point(&mut self, id: &str) -> Result<()> {
        if let Some(index) = self.restore_points.iter().position(|p| p.id == id) {
            let point = &self.restore_points[index];
            fs::remove_dir_all(self.get_restore_point_path(&point.id)).await?;
            self.restore_points.remove(index);
            self.save_restore_points().await?;
        }
        Ok(())
    }

    fn get_restore_point_path(&self, id: &str) -> PathBuf {
        self.base_path.join("restore_points").join(id)
    }

    async fn load_restore_points(&mut self) -> Result<()> {
        let metadata_path = self.base_path.join("restore_points.json");
        if metadata_path.exists() {
            let data = fs::read_to_string(&metadata_path).await?;
            self.restore_points = serde_json::from_str(&data)?;
        }
        Ok(())
    }

    async fn save_restore_points(&self) -> Result<()> {
        let metadata_path = self.base_path.join("restore_points.json");
        let data = serde_json::to_string_pretty(&self.restore_points)?;
        fs::write(&metadata_path, data).await?;
        Ok(())
    }
}
