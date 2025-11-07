use std::collections::HashMap;
use chrono::{DateTime, Utc, Duration, Datelike, Timelike, IsoWeek};
use serde::{Serialize, Deserialize};

use crate::error::{Result, SkylockError};
use crate::direct_upload::BackupManifest;

/// Retention policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Keep the last N backups regardless of age
    pub keep_last: Option<usize>,
    
    /// Keep backups newer than this many days
    pub keep_days: Option<u32>,
    
    /// GFS (Grandfather-Father-Son) rotation settings
    pub gfs: Option<GfsPolicy>,
    
    /// Minimum number of backups to always keep (safety feature)
    pub minimum_keep: usize,
}

/// Grandfather-Father-Son rotation policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GfsPolicy {
    /// Keep hourly backups for this many hours
    pub keep_hourly: Option<usize>,
    
    /// Keep daily backups for this many days
    pub keep_daily: Option<usize>,
    
    /// Keep weekly backups for this many weeks
    pub keep_weekly: Option<usize>,
    
    /// Keep monthly backups for this many months
    pub keep_monthly: Option<usize>,
    
    /// Keep yearly backups for this many years
    pub keep_yearly: Option<usize>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            keep_last: Some(30),  // Keep last 30 backups
            keep_days: Some(90),  // Keep backups from last 90 days
            gfs: None,
            minimum_keep: 3,  // Always keep at least 3 backups
        }
    }
}

/// Retention manager for backup lifecycle management
pub struct RetentionManager {
    policy: RetentionPolicy,
}

impl RetentionManager {
    pub fn new(policy: RetentionPolicy) -> Self {
        Self { policy }
    }
    
    /// Analyze backups and determine which should be deleted
    pub fn calculate_deletions(&self, manifests: &[BackupManifest]) -> Vec<String> {
        if manifests.len() <= self.policy.minimum_keep {
            // Never delete if we're at or below minimum
            return Vec::new();
        }
        
        let mut manifests_sorted = manifests.to_vec();
        manifests_sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        let mut to_delete = Vec::new();
        let mut to_keep: Vec<&BackupManifest> = Vec::new();
        
        // Apply retention rules
        for manifest in &manifests_sorted {
            let should_keep = self.should_keep_backup(manifest, &to_keep);
            
            if should_keep {
                to_keep.push(manifest);
            } else {
                // Only mark for deletion if we have enough backups left
                if manifests_sorted.len() - to_delete.len() > self.policy.minimum_keep {
                    to_delete.push(manifest.backup_id.clone());
                }
            }
        }
        
        to_delete
    }
    
    /// Check if a backup should be kept based on retention policy
    fn should_keep_backup(&self, manifest: &BackupManifest, already_kept: &[&BackupManifest]) -> bool {
        let now = Utc::now();
        
        // Rule 1: Keep last N backups
        if let Some(keep_last) = self.policy.keep_last {
            if already_kept.len() < keep_last {
                return true;
            }
        }
        
        // Rule 2: Keep backups within age threshold
        if let Some(keep_days) = self.policy.keep_days {
            let cutoff = now - Duration::days(keep_days as i64);
            if manifest.timestamp > cutoff {
                return true;
            }
        }
        
        // Rule 3: GFS rotation
        if let Some(ref gfs) = self.policy.gfs {
            if self.should_keep_for_gfs(manifest, already_kept, gfs, now) {
                return true;
            }
        }
        
        false
    }
    
    /// Check if backup should be kept for GFS rotation
    fn should_keep_for_gfs(
        &self,
        manifest: &BackupManifest,
        already_kept: &[&BackupManifest],
        gfs: &GfsPolicy,
        now: DateTime<Utc>,
    ) -> bool {
        let age = now - manifest.timestamp;
        
        // Hourly retention
        if let Some(hours) = gfs.keep_hourly {
            if age < Duration::hours(hours as i64) {
                // Keep first backup of each hour
                let hour_start = manifest.timestamp
                    .date_naive()
                    .and_hms_opt(manifest.timestamp.hour(), 0, 0)
                    .unwrap();
                
                let has_earlier_in_hour = already_kept.iter().any(|m| {
                    let m_hour = m.timestamp
                        .date_naive()
                        .and_hms_opt(m.timestamp.hour(), 0, 0)
                        .unwrap();
                    m_hour == hour_start && m.timestamp > manifest.timestamp
                });
                
                if !has_earlier_in_hour {
                    return true;
                }
            }
        }
        
        // Daily retention
        if let Some(days) = gfs.keep_daily {
            if age < Duration::days(days as i64) {
                // Keep first backup of each day
                let day = manifest.timestamp.date_naive();
                let has_earlier_in_day = already_kept.iter().any(|m| {
                    m.timestamp.date_naive() == day && m.timestamp > manifest.timestamp
                });
                
                if !has_earlier_in_day {
                    return true;
                }
            }
        }
        
        // Weekly retention
        if let Some(weeks) = gfs.keep_weekly {
            if age < Duration::weeks(weeks as i64) {
                // Keep first backup of each week
                let week = manifest.timestamp.iso_week();
                let has_earlier_in_week = already_kept.iter().any(|m| {
                    m.timestamp.iso_week() == week && m.timestamp > manifest.timestamp
                });
                
                if !has_earlier_in_week {
                    return true;
                }
            }
        }
        
        // Monthly retention
        if let Some(months) = gfs.keep_monthly {
            let cutoff = now - Duration::days((months * 30) as i64);
            if manifest.timestamp > cutoff {
                // Keep first backup of each month
                let month = (manifest.timestamp.year(), manifest.timestamp.month());
                let has_earlier_in_month = already_kept.iter().any(|m| {
                    (m.timestamp.year(), m.timestamp.month()) == month 
                        && m.timestamp > manifest.timestamp
                });
                
                if !has_earlier_in_month {
                    return true;
                }
            }
        }
        
        // Yearly retention
        if let Some(years) = gfs.keep_yearly {
            let cutoff = now - Duration::days((years * 365) as i64);
            if manifest.timestamp > cutoff {
                // Keep first backup of each year
                let year = manifest.timestamp.year();
                let has_earlier_in_year = already_kept.iter().any(|m| {
                    m.timestamp.year() == year && m.timestamp > manifest.timestamp
                });
                
                if !has_earlier_in_year {
                    return true;
                }
            }
        }
        
        false
    }
    
    /// Generate retention policy summary
    pub fn summarize(&self) -> String {
        let mut summary = Vec::new();
        
        if let Some(keep_last) = self.policy.keep_last {
            summary.push(format!("Keep last {} backups", keep_last));
        }
        
        if let Some(keep_days) = self.policy.keep_days {
            summary.push(format!("Keep backups from last {} days", keep_days));
        }
        
        if let Some(ref gfs) = self.policy.gfs {
            let mut gfs_parts = Vec::new();
            if let Some(h) = gfs.keep_hourly {
                gfs_parts.push(format!("{}h hourly", h));
            }
            if let Some(d) = gfs.keep_daily {
                gfs_parts.push(format!("{}d daily", d));
            }
            if let Some(w) = gfs.keep_weekly {
                gfs_parts.push(format!("{}w weekly", w));
            }
            if let Some(m) = gfs.keep_monthly {
                gfs_parts.push(format!("{}m monthly", m));
            }
            if let Some(y) = gfs.keep_yearly {
                gfs_parts.push(format!("{}y yearly", y));
            }
            if !gfs_parts.is_empty() {
                summary.push(format!("GFS: {}", gfs_parts.join(", ")));
            }
        }
        
        summary.push(format!("Minimum keep: {} backups", self.policy.minimum_keep));
        
        summary.join(" | ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    
    fn create_test_manifest(backup_id: &str, days_ago: i64) -> BackupManifest {
        BackupManifest {
            backup_id: backup_id.to_string(),
            timestamp: Utc::now() - Duration::days(days_ago),
            files: vec![],
            total_size: 1000,
            file_count: 10,
            source_paths: vec![PathBuf::from("/test")],
            base_backup_id: None,
        }
    }
    
    #[test]
    fn test_keep_last_n() {
        let policy = RetentionPolicy {
            keep_last: Some(5),
            keep_days: None,
            gfs: None,
            minimum_keep: 2,
        };
        
        let manager = RetentionManager::new(policy);
        
        let manifests = vec![
            create_test_manifest("backup1", 1),
            create_test_manifest("backup2", 2),
            create_test_manifest("backup3", 3),
            create_test_manifest("backup4", 4),
            create_test_manifest("backup5", 5),
            create_test_manifest("backup6", 6),
            create_test_manifest("backup7", 7),
        ];
        
        let to_delete = manager.calculate_deletions(&manifests);
        assert_eq!(to_delete.len(), 2);  // Should delete 2 oldest
    }
    
    #[test]
    fn test_minimum_keep() {
        let policy = RetentionPolicy {
            keep_last: Some(1),
            keep_days: Some(1),
            gfs: None,
            minimum_keep: 3,
        };
        
        let manager = RetentionManager::new(policy);
        
        let manifests = vec![
            create_test_manifest("backup1", 10),
            create_test_manifest("backup2", 11),
            create_test_manifest("backup3", 12),
        ];
        
        let to_delete = manager.calculate_deletions(&manifests);
        assert_eq!(to_delete.len(), 0);  // Should keep all due to minimum_keep
    }
    
    #[test]
    fn test_keep_days() {
        let policy = RetentionPolicy {
            keep_last: None,
            keep_days: Some(7),
            gfs: None,
            minimum_keep: 1,
        };
        
        let manager = RetentionManager::new(policy);
        
        let manifests = vec![
            create_test_manifest("backup1", 1),
            create_test_manifest("backup2", 5),
            create_test_manifest("backup3", 10),
            create_test_manifest("backup4", 15),
        ];
        
        let to_delete = manager.calculate_deletions(&manifests);
        assert_eq!(to_delete.len(), 2);  // Should delete backups older than 7 days
    }
}
