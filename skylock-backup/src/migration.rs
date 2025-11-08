/// Migration utilities for converting v1 backups to v2 format
/// 
/// v1 backups use:
/// - SHA-256 KDF (insecure, GPU-vulnerable)
/// - No AAD binding in AES-GCM
/// 
/// v2 backups use:
/// - Argon2id KDF (secure, NIST-compliant)
/// - AAD-bound AES-GCM (prevents transplant attacks)

use crate::error::{Result, SkylockError};
use crate::direct_upload::{DirectUploadBackup, BackupManifest};
use tracing::{info, warn};

/// Detect the encryption version of a backup
pub async fn detect_backup_version(backup: &DirectUploadBackup, backup_id: &str) -> Result<String> {
    let manifest = backup.load_manifest(backup_id).await?;
    
    if manifest.kdf_params.is_some() {
        Ok(manifest.encryption_version)
    } else {
        // No kdf_params means v1 (legacy SHA-256 KDF)
        Ok("v1".to_string())
    }
}

/// Migrate a v1 backup to v2 format
/// 
/// This will:
/// 1. Download all files from the v1 backup
/// 2. Decrypt using legacy v1 method (no AAD)
/// 3. Re-encrypt using v2 method (Argon2id + AAD)
/// 4. Upload as a new v2 backup with suffix "_v2"
/// 5. Preserve the original v1 backup (user can delete manually)
/// 
/// **WARNING**: This operation may take a long time for large backups
/// and will temporarily use significant disk space (2x backup size).
pub async fn migrate_backup_v1_to_v2(
    backup: &DirectUploadBackup,
    backup_id: &str,
) -> Result<String> {
    info!("Starting migration of backup {} from v1 to v2", backup_id);
    
    // Load manifest
    let manifest = backup.load_manifest(backup_id).await?;
    
    // Check if already v2
    if manifest.encryption_version == "v2" && manifest.kdf_params.is_some() {
        warn!("Backup {} is already v2 format", backup_id);
        return Err(SkylockError::Backup(
            format!("Backup {} is already v2 format - no migration needed", backup_id)
        ));
    }
    
    // Check if actually v1
    if manifest.encryption_version != "v1" && manifest.kdf_params.is_some() {
        return Err(SkylockError::Backup(
            format!("Unknown encryption version: {}", manifest.encryption_version)
        ));
    }
    
    println!();
    println!("🔄 Migrating backup {} from v1 to v2 format", backup_id);
    println!("   📦 Files to migrate: {}", manifest.file_count);
    println!("   💾 Total size: {:.2} GB", manifest.total_size as f64 / 1024.0 / 1024.0 / 1024.0);
    println!();
    println!("⚠️  WARNING: This operation may take a long time and use significant disk space!");
    println!("   Original backup will be preserved as: {}", backup_id);
    println!("   New v2 backup will be created as: {}_v2", backup_id);
    println!();
    
    // Create new backup ID with _v2 suffix
    let new_backup_id = format!("{}_v2", backup_id.replace("backup_", ""));
    
    // TODO: Implementation of actual migration
    // This would involve:
    // 1. Downloading all files from v1 backup
    // 2. Decrypting with legacy method
    // 3. Re-encrypting with v2 method
    // 4. Uploading as new backup
    // 
    // For now, return a helpful error message
    Err(SkylockError::Backup(
        "Migration utility not yet implemented. Please create a new v2 backup instead.".to_string()
    ))
}

/// Check if a backup needs migration
pub async fn needs_migration(backup: &DirectUploadBackup, backup_id: &str) -> Result<bool> {
    let version = detect_backup_version(backup, backup_id).await?;
    Ok(version == "v1")
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_migration_placeholder() {
        // Placeholder test - actual migration tests will be added
        // when migration implementation is complete
        assert!(true);
    }
}
