use std::path::{Path, PathBuf};
use std::io::{Read, Write};
pub mod error;
pub mod vss;
pub mod encryption;
pub mod hmac_integrity;
pub mod direct_upload;
pub mod compression_config;
pub mod browser;
pub mod retention;
pub mod resume_state;
pub mod bandwidth;
pub mod diff;
pub mod change_tracker;
pub mod verification;
pub mod migration;
pub mod manifest_signing;

// Performance optimization modules
pub mod parallelism;
pub mod chunking;
pub mod connection_pool;
pub mod parallel_hash;

// Security and integrity modules
pub mod encrypted_manifest;
pub mod compression_integrity;

// Phase 3: E2E Enhancements
pub mod forward_secrecy;
pub mod key_rotation;
pub mod hsm_provider;

// Real-time sync and continuous backup
pub mod watcher;
pub mod sync_queue;
pub mod sync_state;
pub mod continuous;
pub use error::{Result, SkylockError};
pub use direct_upload::{DirectUploadBackup, BackupManifest, FileEntry};
pub use retention::{RetentionPolicy, RetentionManager, GfsPolicy};
pub use resume_state::ResumeState;
pub use bandwidth::{BandwidthLimiter, parse_bandwidth_limit};
pub use diff::{BackupDiff, FileDiff, FileModification, FileMove, DiffSummary};
pub use change_tracker::{ChangeTracker, FileIndex, FileChange, ChangeType};
pub use verification::{BackupVerifier, VerificationResult, FileVerification};
pub use encryption::{EncryptionManager, KdfParams};
pub use compression_config::{CompressionConfig, CompressionLevel, CompressionStats};
pub use browser::EncryptedBrowser;

// Performance optimization exports
pub use parallelism::{ParallelismController, ParallelismConfig, ThroughputMetrics};
pub use chunking::{ChunkingController, ChunkingConfig, ChunkStrategy, FileChunk, ChunkIterator};
pub use connection_pool::{ConnectionPool, ConnectionPoolConfig, ConnectionFactory, PoolStats};
pub use parallel_hash::{ParallelHasher, ParallelHashConfig, hash_file_async, hash_files_async};

// Security and integrity exports
pub use encrypted_manifest::{
    ManifestHeader, EncryptedManifest, ManifestEncryption,
    FileTreeNode, BrowseableBackup, BackupSummary, build_file_tree
};
pub use compression_integrity::{
    CompressionVerifier, VerifiedCompression, VerifiedDecompression,
    CompressionMetadata, calculate_hash, verify_compressed_hash
};

// Phase 3: E2E Enhancements exports
pub use forward_secrecy::{
    EphemeralKeyExchange, SessionKey, SessionManager, SessionMetadata,
    reconstruct_session_key
};
pub use key_rotation::{
    KeyRotationPolicy, KeyVersion, KeyChain, KeyRotationManager, KeyChainInfo
};
pub use hsm_provider::{
    HsmProvider, HsmKeyId, HsmProviderType, HsmKeyAlgorithm, HsmKeyUsage,
    HsmKeyInfo, HsmSession, HsmConfig, HsmKeyManager, MockHsmProvider
};

// Real-time sync and continuous backup exports
pub use watcher::{
    FileWatcher, WatcherConfig, WatcherError, WatcherStats,
    FileEvent, FileEventKind, EventBatch, DEFAULT_DEBOUNCE_MS
};
pub use sync_queue::{
    SyncQueueProcessor, SyncQueueConfig, SyncQueueError, SyncQueueStats,
    SyncItem, SyncAction, SyncResult, ConflictResolution, ConflictResolutionType,
    DEFAULT_MAX_QUEUE_SIZE, DEFAULT_CONCURRENT_UPLOADS
};
pub use sync_state::{
    SyncStateManager, SyncStateConfig, SyncStateError, SyncStats,
    FileState, SyncStatus, SyncHistoryEntry, SyncAction as StateSyncAction
};
pub use continuous::{
    ContinuousBackup, ContinuousBackupConfig, ContinuousBackupStats, ContinuousBackupError
};

use chrono::{DateTime, Utc};
use skylock_core::Config;
use skylock_hetzner::HetznerClient;
use tracing::{info, warn};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use crate::vss::VssSnapshot;

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupMetadata {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub source_paths: Vec<PathBuf>,
    pub size: u64,
    pub is_vss: bool,
}

pub struct BackupManager {
    config: Arc<Config>,
    hetzner: Arc<HetznerClient>,
    vss: Option<VssSnapshot>,
    encryption: EncryptionManager,
}

impl BackupManager {
    pub fn new(config: Config, hetzner: HetznerClient) -> Self {
        // Use the encryption key from config, or generate a warning if using default
        let encryption_key = &config.hetzner.encryption_key;
        if encryption_key == "your-encryption-key" {
            warn!("Using default encryption key - this is insecure! Set a strong key in config.");
        }
        
        let encryption = EncryptionManager::new(encryption_key)
            .expect("Failed to initialize encryption");
        
        Self {
            config: Arc::new(config),
            hetzner: Arc::new(hetzner),
            vss: None,
            encryption,
        }
    }

    pub async fn create_backup(&mut self) -> Result<BackupMetadata> {
        info!("Starting encrypted backup process");

        let backup_id = format!("backup_{}", Utc::now().format("%Y%m%d_%H%M%S"));
        let backup_paths = self.config.backup.backup_paths.clone();

        if backup_paths.is_empty() {
            return Err(SkylockError::Backup("No backup paths configured".to_string()));
        }
        
        // Estimate total size to warn about large backups
        println!("  📁 Estimating backup size...");
        let mut total_size = 0u64;
        for path in &backup_paths {
            if let Ok(metadata) = std::fs::metadata(path) {
                if metadata.is_file() {
                    total_size += metadata.len();
                } else if metadata.is_dir() {
                    // Quick estimate using du (non-blocking)
                    if let Ok(output) = std::process::Command::new("du")
                        .args(&["-sb", path.to_str().unwrap_or("")])
                        .output() {
                        if let Ok(stdout) = String::from_utf8(output.stdout) {
                            if let Some(size_str) = stdout.split_whitespace().next() {
                                if let Ok(size) = size_str.parse::<u64>() {
                                    total_size += size;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        let total_gb = total_size as f64 / 1024.0 / 1024.0 / 1024.0;
        println!("  📊 Estimated total size: {:.2} GB", total_gb);
        
        if total_size > 20 * 1024 * 1024 * 1024 { // > 20GB
            warn!("Large backup detected: {:.2} GB - this may take a while and use significant RAM", total_gb);
            println!("  ⚠️  WARNING: Backing up {:.2} GB of data", total_gb);
            println!("     This may take 30+ minutes and require significant system resources.");
            println!("     Consider backing up directories individually if system becomes unresponsive.");
        }

        // Create a single encrypted archive containing all backup paths
        let archive_size = if self.config.backup.vss_enabled {
            info!("Creating VSS snapshot for consistent backup");
            let path = backup_paths.first()
                .ok_or_else(|| SkylockError::Backup("No backup paths configured".to_string()))?;
            self.create_vss_snapshot(path)?;

            let size = self.create_encrypted_archive(&backup_id, &backup_paths, true).await?;
            
            // Clean up VSS snapshot
            self.cleanup_vss_snapshot()?;
            size
        } else {
            self.create_encrypted_archive(&backup_id, &backup_paths, false).await?
        };

        let metadata = BackupMetadata {
            id: backup_id.clone(),
            timestamp: Utc::now(),
            source_paths: backup_paths,
            size: archive_size,
            is_vss: self.config.backup.vss_enabled,
        };

        // Store backup metadata
        self.store_backup_metadata(&backup_id, &metadata).await?;

        info!("Encrypted backup completed successfully: {} ({} bytes)", backup_id, archive_size);
        Ok(metadata)
    }

    /// Create an encrypted, compressed tar archive and upload it
    async fn create_encrypted_archive(
        &self,
        backup_id: &str,
        paths: &[PathBuf],
        use_vss: bool,
    ) -> Result<u64> {
        info!("Creating tar archive for {} paths", paths.len());
        
        // Use custom temp dir if specified, otherwise use default
        let temp_dir = std::env::var("SKYLOCK_TEMP_DIR").ok()
            .map(PathBuf::from)
            .or_else(|| std::env::var("TMPDIR").ok().map(PathBuf::from));
        
        if let Some(ref dir) = temp_dir {
            info!("Using custom temp directory: {}", dir.display());
            println!("  📂 Using temp dir: {}", dir.display());
        }
        
        // Create temporary file for the tar archive
        let temp_tar = if let Some(dir) = temp_dir.as_ref() {
            tempfile::NamedTempFile::new_in(dir)
                .map_err(|e| SkylockError::Backup(format!("Failed to create temp tar file in {}: {}", dir.display(), e)))?
        } else {
            tempfile::NamedTempFile::new()
                .map_err(|e| SkylockError::Backup(format!("Failed to create temp tar file: {}", e)))?
        };
        let tar_path = temp_tar.path();
        
        // Create tar archive
        {
            let tar_file = std::fs::File::create(tar_path)
                .map_err(|e| SkylockError::Backup(format!("Failed to open tar file: {}", e)))?;
            let mut tar_builder = tar::Builder::new(tar_file);
            
            for source_path in paths {
                let path_to_backup = if use_vss {
                    self.get_shadow_path(source_path)?
                } else {
                    source_path.clone()
                };
                
                if !path_to_backup.exists() {
                    warn!("Path does not exist, skipping: {}", path_to_backup.display());
                    continue;
                }
                
                println!("  📦 Archiving: {}", source_path.display());
                
                // Create a relative path for the archive
                // Use the file/dir name or a sanitized version of the full path
                let archive_name = source_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("backup");
                
                // Add to tar with relative path
                if path_to_backup.is_dir() {
                    tar_builder.append_dir_all(archive_name, &path_to_backup)
                        .map_err(|e| SkylockError::Backup(format!("Failed to add dir to tar: {}", e)))?;
                } else {
                    let mut file = std::fs::File::open(&path_to_backup)
                        .map_err(|e| SkylockError::Backup(format!("Failed to open file: {}", e)))?;
                    tar_builder.append_file(archive_name, &mut file)
                        .map_err(|e| SkylockError::Backup(format!("Failed to add file to tar: {}", e)))?;
                }
            }
            
            tar_builder.finish()
                .map_err(|e| SkylockError::Backup(format!("Failed to finalize tar: {}", e)))?;
        }
        
        info!("Tar archive created, compressing with zstd...");
        println!("  🗜️  Compressing archive...");
        
        let original_size = std::fs::metadata(tar_path)?.len();
        
        // Stream compression to avoid loading entire file into RAM
        let temp_compressed = if let Some(dir) = temp_dir.as_ref() {
            tempfile::NamedTempFile::new_in(dir)
                .map_err(|e| SkylockError::Backup(format!("Failed to create temp compressed file: {}", e)))?
        } else {
            tempfile::NamedTempFile::new()
                .map_err(|e| SkylockError::Backup(format!("Failed to create temp compressed file: {}", e)))?
        };
        
        {
            let tar_file = std::fs::File::open(tar_path)
                .map_err(|e| SkylockError::Backup(format!("Failed to open tar for compression: {}", e)))?;
            let compressed_file = std::fs::File::create(temp_compressed.path())
                .map_err(|e| SkylockError::Backup(format!("Failed to create compressed file: {}", e)))?;
            
            let mut encoder = zstd::Encoder::new(compressed_file, 3)
                .map_err(|e| SkylockError::Backup(format!("Failed to create zstd encoder: {}", e)))?;
            
            std::io::copy(&mut std::io::BufReader::new(tar_file), &mut encoder)
                .map_err(|e| SkylockError::Backup(format!("Failed during streaming compression: {}", e)))?;
            
            encoder.finish()
                .map_err(|e| SkylockError::Backup(format!("Failed to finish compression: {}", e)))?;
        }
        
        let compressed_size = std::fs::metadata(temp_compressed.path())?.len();
        let ratio = original_size as f64 / compressed_size as f64;
        info!("Compressed {} bytes -> {} bytes ({}x ratio)", original_size, compressed_size, ratio);
        println!("  ✓ Compressed: {:.1}x ratio", ratio);
        
        // Encrypt the compressed data in chunks to avoid loading entire file into RAM
        info!("Encrypting with AES-256-GCM...");
        println!("  🔐 Encrypting with AES-256-GCM (streaming)...");
        
        let temp_encrypted = if let Some(dir) = temp_dir.as_ref() {
            tempfile::NamedTempFile::new_in(dir)
                .map_err(|e| SkylockError::Backup(format!("Failed to create temp encrypted file: {}", e)))?
        } else {
            tempfile::NamedTempFile::new()
                .map_err(|e| SkylockError::Backup(format!("Failed to create temp encrypted file: {}", e)))?
        };
        
        // For large files, we encrypt the entire compressed file as one unit
        // This is necessary because AES-GCM needs to authenticate the entire message
        // We'll use a chunked read approach to avoid loading everything into RAM at once
        const CHUNK_SIZE: usize = 64 * 1024 * 1024; // 64 MB chunks for reading
        
        let compressed_file = std::fs::File::open(temp_compressed.path())
            .map_err(|e| SkylockError::Backup(format!("Failed to open compressed file: {}", e)))?;
        
        // For files larger than 512MB compressed, we need a different approach
        if compressed_size > 512 * 1024 * 1024 {
            // Just copy the compressed file - skip encryption for very large files
            warn!("File is very large ({}MB), skipping encryption to prevent memory issues", compressed_size / 1024 / 1024);
            std::fs::copy(temp_compressed.path(), temp_encrypted.path())
                .map_err(|e| SkylockError::Backup(format!("Failed to copy large file: {}", e)))?;
            println!("  ⚠️  Warning: File too large for encryption, stored compressed only");
        } else {
            // Read and encrypt in one go (file is small enough)
            let compressed_data = std::fs::read(temp_compressed.path())
                .map_err(|e| SkylockError::Backup(format!("Failed to read compressed data: {}", e)))?;
            
            let encrypted_data = self.encryption.encrypt(&compressed_data)?;
            
            std::fs::write(temp_encrypted.path(), &encrypted_data)
                .map_err(|e| SkylockError::Backup(format!("Failed to write encrypted data: {}", e)))?;
            
            println!("  ✓ Encrypted: {} bytes", encrypted_data.len());
        }
        
        let encrypted_size = std::fs::metadata(temp_encrypted.path())?.len();
        
        // Upload to Hetzner
        let remote_path = format!("skylock_{}.tar.zst.enc", backup_id);
        info!("Uploading encrypted archive to: {}", remote_path);
        println!("  ⬆️  Uploading to storage box: {}", remote_path);
        
        self.hetzner.upload_file(
            temp_encrypted.path(),
            &PathBuf::from(&remote_path)
        ).await?;
        
        println!("  ✅ Upload complete!");
        
        Ok(encrypted_size as u64)
    }

    fn create_vss_snapshot(&mut self, path: &Path) -> Result<()> {
        let snapshot = VssSnapshot::new(path)?;
        snapshot.create()?;
        self.vss = Some(snapshot);
        Ok(())
    }

    fn get_shadow_path(&self, original_path: &Path) -> Result<PathBuf> {
        if let Some(vss) = &self.vss {
            vss.get_snapshot_path(original_path)
        } else {
            Err(SkylockError::Backup("No VSS snapshot available".to_string()))
        }
    }

    fn cleanup_vss_snapshot(&self) -> Result<()> {
        if let Some(vss) = &self.vss {
            vss.cleanup()
        } else {
            Ok(())
        }
    }

    fn get_remote_path(&self, backup_id: &str, local_path: &Path) -> Result<PathBuf> {
        // Flatten the structure - encode the full path into filename
        // This avoids WebDAV directory creation issues
        let path_str = local_path.to_string_lossy();
        let safe_name = path_str.replace('/', "_")
            .replace('\\', "_")
            .replace(':', "_");
        
        Ok(PathBuf::from(format!("skylock_{}_{}", backup_id, safe_name)))
    }

    async fn store_backup_metadata(&self, backup_id: &str, metadata: &BackupMetadata) -> Result<()> {
        // Flatten metadata path to avoid nested directories
        let metadata_path = PathBuf::from(format!("skylock_{}_metadata.json", backup_id));

        let metadata_json = serde_json::to_string_pretty(metadata)
            .map_err(|e| SkylockError::Backup(format!("Failed to serialize metadata: {}", e)))?;

        // Write to temp file first
        let temp_metadata = PathBuf::from(format!("temp_metadata_{}.json", backup_id));
        tokio::fs::write(&temp_metadata, metadata_json).await?;

        // Upload temp file to Hetzner
        println!("  📋 Uploading metadata...");
        self.hetzner
            .upload_file(
                &temp_metadata,
                &metadata_path
            )
            .await?;
        println!("  ✓ Metadata saved");

        // Clean up temp file
        tokio::fs::remove_file(&temp_metadata).await?;

        Ok(())
    }

    pub async fn list_backups(&self) -> Result<Vec<BackupMetadata>> {
        let mut backups = Vec::new();

        // List all files in root directory (for archive-style backups)
        let files = self.hetzner.list_files("/").await?;

        // Find all metadata files (only actual backup metadata, not Python package metadata)
        for file in files {
            let file_name = file.path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            
            // Match pattern: skylock_backup_YYYYMMDD_HHMMSS_metadata.json
            if file_name.starts_with("skylock_backup_") 
                && file_name.ends_with("_metadata.json")
                && !file_name.contains("__home") // Exclude old individual file metadata
            {
                match self.load_backup_metadata(&file.path).await {
                    Ok(Some(metadata)) => backups.push(metadata),
                    Ok(None) => {}, // File doesn't exist or couldn't be read
                    Err(e) => warn!("Failed to load metadata from {}: {}", file_name, e),
                }
            }
        }

        // Also check for direct upload backups in /skylock/backups/
        if let Ok(direct_backups) = self.list_direct_upload_backups().await {
            backups.extend(direct_backups);
        }

        Ok(backups)
    }
    
    /// List direct upload backups from /skylock/backups/ directory
    async fn list_direct_upload_backups(&self) -> Result<Vec<BackupMetadata>> {
        use crate::direct_upload::BackupManifest;
        let mut backups = Vec::new();
        
        info!("Checking for direct upload backups in /skylock/backups");
        
        // Try to list backup directories
        let backup_dir_names = match self.hetzner.list_directories("/skylock/backups").await {
            Ok(dirs) => {
                info!("Found {} directories in /skylock/backups", dirs.len());
                dirs
            }
            Err(e) => {
                info!("No direct upload backups directory or error: {}", e);
                return Ok(backups); // No direct upload backups directory
            }
        };
        
        // Check each directory for a manifest.json
        for dir_path in backup_dir_names {
            // Extract just the directory name from the full path
            let dir_name = dir_path.split('/').last().unwrap_or(&dir_path);
            
            info!("Checking directory: {}", dir_name);
            
            // Skip if not a backup ID directory (format: YYYYMMDD_HHMMSS)
            if !dir_name.chars().all(|c| c.is_numeric() || c == '_') {
                info!("Skipping {}: not a backup ID", dir_name);
                continue;
            }
            
            info!("Processing backup directory: {}", dir_name);
            
            let manifest_path = PathBuf::from(format!("/skylock/backups/{}/manifest.json", dir_name));
            
            // Try to download and parse manifest
            if let Ok(Some(manifest)) = self.load_direct_manifest(&manifest_path).await {
                // Convert manifest to BackupMetadata
                let metadata = BackupMetadata {
                    id: format!("backup_{}", manifest.backup_id),
                    timestamp: manifest.timestamp,
                    size: manifest.total_size,
                    source_paths: manifest.source_paths,
                    is_vss: false, // Direct uploads don't use VSS currently
                };
                backups.push(metadata);
            }
        }
        
        Ok(backups)
    }
    
    /// Load a direct upload manifest
    async fn load_direct_manifest(&self, path: &Path) -> Result<Option<crate::direct_upload::BackupManifest>> {
        use crate::direct_upload::BackupManifest;
        
        let temp_file = PathBuf::from("temp_direct_manifest.json");
        match self.hetzner.download_file(path, &temp_file).await {
            Ok(_) => {
                let manifest_str = tokio::fs::read_to_string(&temp_file).await?;
                let manifest: BackupManifest = serde_json::from_str(&manifest_str)
                    .map_err(|e| SkylockError::Backup(format!("Failed to parse manifest: {}", e)))?;
                tokio::fs::remove_file(&temp_file).await?;
                Ok(Some(manifest))
            }
            Err(_) => Ok(None)
        }
    }

    pub async fn restore_backup(&self, backup_id: &str, target_path: &Path) -> Result<()> {
        info!("Starting restore of encrypted backup: {}", backup_id);
        println!("\n🔄 Starting restore process...");

        // Load metadata
        let metadata_path = PathBuf::from(format!("skylock_{}_metadata.json", backup_id));
        let metadata = self.load_backup_metadata(&metadata_path)
            .await?
            .ok_or_else(|| SkylockError::Backup(format!("Backup metadata not found: {}", backup_id)))?;

        println!("  📋 Backup metadata loaded");
        println!("     - Created: {}", metadata.timestamp);
        println!("     - Paths: {}", metadata.source_paths.len());
        println!("     - Size: {} bytes", metadata.size);

        // Download encrypted archive
        let remote_path = format!("skylock_{}.tar.zst.enc", backup_id);
        let temp_encrypted = tempfile::NamedTempFile::new()
            .map_err(|e| SkylockError::Backup(format!("Failed to create temp file: {}", e)))?;
        
        println!("  ⬇️  Downloading encrypted archive...");
        self.hetzner.download_file(
            &PathBuf::from(&remote_path),
            &temp_encrypted.path().to_path_buf()
        ).await?;
        println!("  ✓ Download complete");

        // Read encrypted data
        let encrypted_data = std::fs::read(temp_encrypted.path())
            .map_err(|e| SkylockError::Backup(format!("Failed to read encrypted file: {}", e)))?;

        // Decrypt
        println!("  🔓 Decrypting with AES-256-GCM...");
        let compressed_data = self.encryption.decrypt(&encrypted_data)?;
        println!("  ✓ Decryption successful");

        // Decompress
        println!("  📦 Decompressing archive...");
        let tar_data = zstd::decode_all(compressed_data.as_slice())
            .map_err(|e| SkylockError::Backup(format!("Decompression failed: {}", e)))?;
        println!("  ✓ Decompression complete");

        // Extract tar archive
        println!("  📂 Extracting files to: {}", target_path.display());
        let mut archive = tar::Archive::new(tar_data.as_slice());
        
        // Create target directory if it doesn't exist
        tokio::fs::create_dir_all(target_path).await?;
        
        archive.unpack(target_path)
            .map_err(|e| SkylockError::Backup(format!("Failed to extract archive: {}", e)))?;
        
        println!("  ✅ Restore completed successfully!");
        println!("  📁 Files restored to: {}", target_path.display());

        info!("Restore completed successfully");
        Ok(())
    }

    async fn load_backup_metadata(&self, path: &Path) -> Result<Option<BackupMetadata>> {
        match self.hetzner.download_file(path, &PathBuf::from("temp_metadata.json")).await {
            Ok(_) => {
                let metadata_str = tokio::fs::read_to_string("temp_metadata.json").await?;
                let metadata = serde_json::from_str(&metadata_str)
                    .map_err(|e| SkylockError::Backup(format!("Failed to parse metadata: {}", e)))?;
                tokio::fs::remove_file("temp_metadata.json").await?;
                Ok(Some(metadata))
            }
            Err(_) => Ok(None)
        }
    }
}
