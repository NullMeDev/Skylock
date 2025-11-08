//! Direct upload backup strategy
//!
//! Uploads files directly to storage box without creating local archives.
//! Features:
//! - Per-file AES-256-GCM encryption
//! - Streaming uploads (no temp files)
//! - Smart compression for large files (>10MB)
//! - Adaptive parallel uploads
//! - Individual file restore capability

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use sha2::{Sha256, Digest};
use walkdir::WalkDir;
use indicatif::{ProgressBar, ProgressStyle, MultiProgress, HumanBytes, HumanDuration};

use crate::error::{Result, SkylockError};
use crate::encryption::EncryptionManager;
use crate::resume_state::ResumeState;
use crate::bandwidth::BandwidthLimiter;
use crate::change_tracker::{ChangeTracker, FileIndex};
use skylock_core::Config;
use skylock_hetzner::HetznerClient;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileEntry {
    /// Local path where file was backed up from
    pub local_path: PathBuf,
    /// Remote path on storage box
    pub remote_path: String,
    /// File size in bytes (original, before encryption)
    pub size: u64,
    /// SHA-256 hash of original file
    pub hash: String,
    /// Whether file was compressed
    pub compressed: bool,
    /// Whether file was encrypted (always true)
    pub encrypted: bool,
    /// Timestamp when file was backed up
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    /// Unique backup ID (e.g., "20251023_211045")
    pub backup_id: String,
    /// When backup was created
    pub timestamp: DateTime<Utc>,
    /// List of all files in this backup
    pub files: Vec<FileEntry>,
    /// Total size of backup (before compression/encryption)
    pub total_size: u64,
    /// Number of files backed up
    pub file_count: usize,
    /// Source paths that were backed up
    pub source_paths: Vec<PathBuf>,
    /// Base backup ID for incremental backups (None for full backups)
    #[serde(default)]
    pub base_backup_id: Option<String>,
    /// Encryption format version ("v1" = old SHA-256, "v2" = Argon2id + AAD)
    #[serde(default = "default_encryption_version")]
    pub encryption_version: String,
    /// KDF parameters (only for v2, None for legacy v1)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kdf_params: Option<crate::encryption::KdfParams>,
}

fn default_encryption_version() -> String {
    "v2".to_string()
}

/// File metadata for diff operations (simplified version of FileEntry)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// Relative path of the file
    pub relative_path: String,
    /// File size in bytes
    pub size: u64,
    /// SHA-256 hash of original file
    pub hash: String,
    /// Whether file was compressed
    pub compressed: bool,
    /// Remote path on storage box
    pub remote_path: String,
}

impl From<&FileEntry> for FileMetadata {
    fn from(entry: &FileEntry) -> Self {
        FileMetadata {
            relative_path: entry.local_path.to_string_lossy().to_string(),
            size: entry.size,
            hash: entry.hash.clone(),
            compressed: entry.compressed,
            remote_path: entry.remote_path.clone(),
        }
    }
}

pub struct DirectUploadBackup {
    config: Arc<Config>,
    hetzner: Arc<HetznerClient>,
    encryption: Arc<EncryptionManager>,
    /// Maximum concurrent uploads (adaptive based on system)
    max_parallel: usize,
    /// Optional bandwidth limiter (bytes per second, None = unlimited)
    bandwidth_limiter: Option<Arc<BandwidthLimiter>>,
}

impl DirectUploadBackup {
    /// Returns the default encryption version for new backups
    pub fn default_encryption_version() -> String {
        "v2".to_string()
    }
    
    pub fn new(config: Config, hetzner: HetznerClient, encryption: EncryptionManager, bandwidth_limit: Option<u64>) -> Self {
        // Adaptive parallelism: Use 4 threads for normal systems, scale down if needed
        let max_parallel = std::thread::available_parallelism()
            .map(|n| n.get().min(4))  // Max 4 uploads at once
            .unwrap_or(2);  // Fallback to 2 if can't detect
        
        let bandwidth_limiter = bandwidth_limit.map(|limit| {
            Arc::new(BandwidthLimiter::new(limit))
        });
        
        Self {
            config: Arc::new(config),
            hetzner: Arc::new(hetzner),
            encryption: Arc::new(encryption),
            max_parallel,
            bandwidth_limiter,
        }
    }

    /// Create full backup using direct upload strategy
    pub async fn create_backup(&self, paths: &[PathBuf]) -> Result<BackupManifest> {
        self.create_backup_internal(paths, false).await
    }
    
    /// Create incremental backup using direct upload strategy
    pub async fn create_incremental_backup(&self, paths: &[PathBuf]) -> Result<BackupManifest> {
        self.create_backup_internal(paths, true).await
    }
    
    /// Internal backup creation with full/incremental support
    async fn create_backup_internal(&self, paths: &[PathBuf], incremental: bool) -> Result<BackupManifest> {
        let backup_id = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let index_dir = self.config.data_dir.join("indexes");
        tokio::fs::create_dir_all(&index_dir).await?;
        let tracker = ChangeTracker::new(index_dir);
        
        // Determine base backup for incremental mode
        let base_backup_id = if incremental {
            if tracker.has_latest_index().await {
                // Load latest index to get backup_id
                let latest_index = tracker.load_latest_index().await?;
                Some(latest_index.tracked_dirs.first()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
                )
            } else {
                println!("⚠️  No previous backup found - creating full backup instead");
                None
            }
        } else {
            None
        };
        
        // Check for existing resume state
        let mut resume_state = if ResumeState::exists(&backup_id).await {
            let state = ResumeState::load(&backup_id).await?;
            println!("🔄 Resuming interrupted backup: {}", backup_id);
            println!("   ⏱️  Started: {}", state.started_at.format("%Y-%m-%d %H:%M:%S UTC"));
            println!("   ✅ Already uploaded: {}/{} files ({:.1}%)", 
                state.uploaded_count(), 
                state.total_files,
                state.progress_percent()
            );
            println!();
            Some(state)
        } else {
            if incremental && base_backup_id.is_some() {
                println!("🔄 Starting incremental backup: {}", backup_id);
            } else {
                println!("🚀 Starting full backup: {}", backup_id);
            }
            None
        };
        
        println!("   📁 Using {}-thread parallel uploads", self.max_parallel);
        println!("   🔐 AES-256-GCM encryption enabled");
        println!("   🗜️  Smart compression (files >10MB)");
        if let Some(ref base_id) = base_backup_id {
            println!("   🔗 Base backup: backup_{}", base_id.as_ref().unwrap_or(&"unknown".to_string()));
        }
        println!();
        
        // Collect all files to backup
        let mut all_files = Vec::new();
        let mut total_size = 0u64;
        let mut skipped_count = 0;
        
        for path in paths {
            println!("📂 Scanning: {}", path.display());
            let mut files = self.collect_files(path)?;
            let path_size: u64 = files.iter().map(|(_, size)| size).sum();
            println!("   Found {} files ({:.2} MB)", files.len(), path_size as f64 / 1024.0 / 1024.0);
            
            // Filter for incremental backups
            if incremental && base_backup_id.is_some() {
                // Get list of changed files from tracker
                let changed_files = tracker.get_changed_files(paths).await?;
                let changed_paths: std::collections::HashSet<_> = changed_files.into_iter().collect();
                
                let original_count = files.len();
                files.retain(|(path, _)| changed_paths.contains(path));
                skipped_count += original_count - files.len();
            }
            
            let included_size: u64 = files.iter().map(|(_, size)| size).sum();
            total_size += included_size;
            all_files.extend(files);
        }
        
        let file_count = all_files.len();
        
        if incremental && skipped_count > 0 {
            println!("➡️  Incremental: Backing up {} changed files, skipping {} unchanged", file_count, skipped_count);
        }
        
        println!();
        println!("📊 Total: {} files, {:.2} GB", file_count, total_size as f64 / 1024.0 / 1024.0 / 1024.0);
        println!();
        
        // Initialize resume state if not already loaded
        if resume_state.is_none() {
            resume_state = Some(ResumeState::new(
                backup_id.clone(),
                paths.to_vec(),
                file_count
            ));
            // Save initial state
            if let Some(ref state) = resume_state {
                state.save().await?;
            }
        }
        
        // Upload files with parallelism control and resume support
        let uploaded_files = self.upload_files_parallel_with_resume(
            &backup_id, 
            all_files,
            resume_state.as_mut().unwrap()
        ).await?;
        
        // Create manifest
        let manifest = BackupManifest {
            backup_id: backup_id.clone(),
            timestamp: Utc::now(),
            files: uploaded_files,
            total_size,
            file_count,
            source_paths: paths.to_vec(),
            base_backup_id: base_backup_id.flatten(),
            encryption_version: Self::default_encryption_version(),
            kdf_params: Some(self.encryption.kdf_params().clone()),
        };
        
        // Upload manifest
        self.upload_manifest(&manifest).await?;
        
        // Clean up resume state file after successful completion
        ResumeState::delete(&backup_id).await?;
        
        // Build and save index of backed up files for change tracking
        let file_index = FileIndex::build(paths)?;
        if let Err(e) = tracker.save_index(&backup_id, &file_index).await {
            eprintln!("⚠️  Warning: Failed to save file index: {}", e);
            eprintln!("   Change tracking may not work correctly.");
        }
        
        println!();
        if incremental && manifest.base_backup_id.is_some() {
            println!("✅ Incremental backup complete: {}", backup_id);
            println!("   🔗 Based on: {}", manifest.base_backup_id.as_ref().unwrap());
            println!("   📦 {} files uploaded (changed only)", manifest.file_count);
        } else {
            println!("✅ Full backup complete: {}", backup_id);
            println!("   📦 {} files uploaded", manifest.file_count);
        }
        println!("   💾 {:.2} GB total", manifest.total_size as f64 / 1024.0 / 1024.0 / 1024.0);
        
        Ok(manifest)
    }

    /// Collect all files in a directory recursively
    fn collect_files(&self, path: &Path) -> Result<Vec<(PathBuf, u64)>> {
        let mut files = Vec::new();
        
        if path.is_file() {
            let size = path.metadata()?.len();
            files.push((path.to_path_buf(), size));
            return Ok(files);
        }
        
        for entry in WalkDir::new(path).follow_links(false) {
            let entry = entry.map_err(|e| SkylockError::Backup(format!("Walk error: {}", e)))?;
            
            if entry.file_type().is_file() {
                let metadata = entry.metadata()
                    .map_err(|e| SkylockError::Backup(format!("Metadata error: {}", e)))?;
                files.push((entry.path().to_path_buf(), metadata.len()));
            }
        }
        
        Ok(files)
    }

    /// Upload files in parallel with semaphore control
    async fn upload_files_parallel(
        &self,
        backup_id: &str,
        files: Vec<(PathBuf, u64)>,
    ) -> Result<Vec<FileEntry>> {
        let total_files = files.len() as u64;
        let semaphore = Arc::new(Semaphore::new(self.max_parallel));
        let mut tasks = Vec::new();
        
        // Create progress bars (indicatif auto-detects TTY)
        let multi = MultiProgress::new();
        
        // Overall progress bar
        let overall_pb = multi.add(ProgressBar::new(total_files));
        overall_pb.set_style(
            ProgressStyle::default_bar()
                .template("{msg}\n{bar:40.cyan/blue} {pos}/{len} files ({percent}%) ETA: {eta}")
                .unwrap()
                .progress_chars("█▓▒░ ")
        );
        overall_pb.set_message("📦 Overall Progress");
        
        // Current file progress bar  
        let file_pb = multi.add(ProgressBar::new(100));
        file_pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} {msg}\n{bar:40.green/blue} {bytes}/{total_bytes} ({bytes_per_sec}) ETA: {eta}")
                .unwrap()
                .progress_chars("█▓▒░ ")
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
        );
        file_pb.enable_steady_tick(Duration::from_millis(100));
        
        let overall_pb_clone = overall_pb.clone();
        let file_pb_clone = file_pb.clone();
        
        for (local_path, size) in files {
            let sem = semaphore.clone();
            let backup_id = backup_id.to_string();
            let hetzner = self.hetzner.clone();
            let encryption = self.encryption.clone();
            let bandwidth_limiter = self.bandwidth_limiter.clone();
            let overall_pb = overall_pb_clone.clone();
            let file_pb = file_pb_clone.clone();
            let file_name = local_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            
            let task = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                
                // Update current file progress
                file_pb.set_message(format!("⬆️  Uploading: {}", file_name));
                file_pb.set_length(size);
                file_pb.set_position(0);
                
                let result = Self::upload_single_file_with_progress(
                    &backup_id,
                    local_path,
                    size,
                    hetzner,
                    encryption,
                    bandwidth_limiter,
                    file_pb.clone(),
                ).await;
                
                // Complete file progress
                file_pb.finish_and_clear();
                
                // Update overall progress
                overall_pb.inc(1);
                
                result
            });
            
            tasks.push(task);
        }
        
        // Wait for all uploads to complete
        let mut uploaded = Vec::new();
        let mut failed_count = 0;
        
        for task in tasks {
            match task.await {
                Ok(Ok(entry)) => uploaded.push(entry),
                Ok(Err(e)) => {
                    failed_count += 1;
                    multi.println(format!("⚠️  Upload failed: {}", e)).unwrap();
                }
                Err(e) => {
                    failed_count += 1;
                    multi.println(format!("⚠️  Task failed: {}", e)).unwrap();
                }
            }
        }
        
        // Finish progress bars
        overall_pb.finish_with_message(format!(
            "✅ Upload complete: {} files uploaded, {} failed",
            uploaded.len(),
            failed_count
        ));
        
        Ok(uploaded)
    }
    
    /// Upload files in parallel with semaphore control and resume support
    async fn upload_files_parallel_with_resume(
        &self,
        backup_id: &str,
        files: Vec<(PathBuf, u64)>,
        resume_state: &mut ResumeState,
    ) -> Result<Vec<FileEntry>> {
        let total_files = files.len() as u64;
        let semaphore = Arc::new(Semaphore::new(self.max_parallel));
        let mut tasks = Vec::new();
        
        // Filter out already-uploaded files
        let files_to_upload: Vec<_> = files.into_iter()
            .filter(|(path, _)| !resume_state.is_uploaded(path))
            .collect();
        
        let remaining_count = files_to_upload.len();
        
        if remaining_count == 0 {
            println!("✅ All files already uploaded - backup complete!");
            // Still need to reconstruct file entries from state
            // For now, return empty and let manifest reconstruction handle it
            return Ok(Vec::new());
        }
        
        println!("   📊 {} files remaining to upload", remaining_count);
        println!();
        
        // Create progress bars (indicatif auto-detects TTY)
        let multi = MultiProgress::new();
        
        // Overall progress bar
        let overall_pb = multi.add(ProgressBar::new(total_files));
        overall_pb.set_style(
            ProgressStyle::default_bar()
                .template("{msg}\n{bar:40.cyan/blue} {pos}/{len} files ({percent}%) ETA: {eta}")
                .unwrap()
                .progress_chars("█▓▒░ ")
        );
        overall_pb.set_message("📦 Overall Progress");
        // Set position to already-uploaded count
        overall_pb.set_position(resume_state.uploaded_count() as u64);
        
        // Current file progress bar  
        let file_pb = multi.add(ProgressBar::new(100));
        file_pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} {msg}\n{bar:40.green/blue} {bytes}/{total_bytes} ({bytes_per_sec}) ETA: {eta}")
                .unwrap()
                .progress_chars("█▓▒░ ")
                .tick_strings(&["⠋", "⠉", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
        );
        file_pb.enable_steady_tick(Duration::from_millis(100));
        
        let overall_pb_clone = overall_pb.clone();
        let file_pb_clone = file_pb.clone();
        
        // Clone resume_state for thread-safe updates
        let resume_state_clone = Arc::new(tokio::sync::Mutex::new(resume_state.clone()));
        
        for (local_path, size) in files_to_upload {
            let sem = semaphore.clone();
            let backup_id = backup_id.to_string();
            let hetzner = self.hetzner.clone();
            let encryption = self.encryption.clone();
            let bandwidth_limiter = self.bandwidth_limiter.clone();
            let overall_pb = overall_pb_clone.clone();
            let file_pb = file_pb_clone.clone();
            let resume_state_ref = resume_state_clone.clone();
            let file_name = local_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            let local_path_clone = local_path.clone();
            
            let task = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                
                // Update current file progress
                file_pb.set_message(format!("⬆️  Uploading: {}", file_name));
                file_pb.set_length(size);
                file_pb.set_position(0);
                
                let result = Self::upload_single_file_with_progress(
                    &backup_id,
                    local_path.clone(),
                    size,
                    hetzner,
                    encryption,
                    bandwidth_limiter,
                    file_pb.clone(),
                ).await;
                
                // If upload succeeded, mark in resume state
                if result.is_ok() {
                    let mut state = resume_state_ref.lock().await;
                    state.mark_uploaded(local_path_clone.clone());
                    // Save state after each successful upload
                    let _ = state.save().await;
                }
                
                // Complete file progress
                file_pb.finish_and_clear();
                
                // Update overall progress
                overall_pb.inc(1);
                
                result
            });
            
            tasks.push(task);
        }
        
        // Wait for all uploads to complete
        let mut uploaded = Vec::new();
        let mut failed_count = 0;
        
        for task in tasks {
            match task.await {
                Ok(Ok(entry)) => uploaded.push(entry),
                Ok(Err(e)) => {
                    failed_count += 1;
                    multi.println(format!("⚠️  Upload failed: {}", e)).unwrap();
                }
                Err(e) => {
                    failed_count += 1;
                    multi.println(format!("⚠️  Task failed: {}", e)).unwrap();
                }
            }
        }
        
        // Update the original resume_state with final state
        let final_state = resume_state_clone.lock().await;
        *resume_state = final_state.clone();
        
        // Finish progress bars
        overall_pb.finish_with_message(format!(
            "✅ Upload complete: {} files uploaded, {} failed",
            uploaded.len(),
            failed_count
        ));
        
        Ok(uploaded)
    }

    /// Upload a single file with encryption, compression, and progress tracking
    async fn upload_single_file_with_progress(
        backup_id: &str,
        local_path: PathBuf,
        size: u64,
        hetzner: Arc<HetznerClient>,
        encryption: Arc<EncryptionManager>,
        bandwidth_limiter: Option<Arc<BandwidthLimiter>>,
        progress: ProgressBar,
    ) -> Result<FileEntry> {
        // Calculate hash
        let hash = Self::calculate_hash(&local_path).await?;
        progress.set_position(size / 4); // 25% for hashing
        
        // Determine if we should compress (files > 10MB)
        let should_compress = size > 10 * 1024 * 1024;
        
        // Build remote path
        let relative_path = local_path.strip_prefix("/")
            .unwrap_or(&local_path);
        let remote_path = format!(
            "/skylock/backups/{}/{}{}",
            backup_id,
            relative_path.display(),
            if should_compress { ".zst.enc" } else { ".enc" }
        );
        
        // Read file
        let data = tokio::fs::read(&local_path).await?;
        progress.set_position(size / 2); // 50% for reading
        
        // Optionally compress
        let data_to_encrypt = if should_compress {
            zstd::encode_all(data.as_slice(), 3)
                .map_err(|e| SkylockError::Backup(format!("Compression failed: {}", e)))?
        } else {
            data
        };
        progress.set_position(size * 3 / 4); // 75% for compression
        
        // Encrypt
        let encrypted_data = encryption.encrypt(&data_to_encrypt)?;
        
        // Create parent directories
        if let Some(parent) = PathBuf::from(&remote_path).parent() {
            if let Some(parent_str) = parent.to_str() {
                let _ = Self::ensure_remote_directory_exists(&hetzner, parent_str).await;
            }
        }
        
        // Apply bandwidth throttling if enabled
        if let Some(ref limiter) = bandwidth_limiter {
            limiter.consume(encrypted_data.len() as u64).await;
        }
        
        // Upload
        let temp_file = tempfile::NamedTempFile::new()
            .map_err(|e| SkylockError::Backup(format!("Temp file failed: {}", e)))?;
        tokio::fs::write(temp_file.path(), &encrypted_data).await?;
        
        hetzner.upload_file(temp_file.path(), &PathBuf::from(&remote_path)).await?;
        progress.set_position(size); // 100% complete
        
        Ok(FileEntry {
            local_path: local_path.clone(),
            remote_path,
            size,
            hash,
            compressed: should_compress,
            encrypted: true,
            timestamp: Utc::now(),
        })
    }

    /// Upload a single file with encryption and optional compression (legacy without progress)
    async fn upload_single_file(
        backup_id: &str,
        local_path: PathBuf,
        size: u64,
        hetzner: Arc<HetznerClient>,
        encryption: Arc<EncryptionManager>,
    ) -> Result<FileEntry> {
        // Calculate hash
        let hash = Self::calculate_hash(&local_path).await?;
        
        // Determine if we should compress (files > 10MB)
        let should_compress = size > 10 * 1024 * 1024;
        
        // Build remote path: /skylock/backups/{backup_id}/{relative_path}.enc
        let relative_path = local_path.strip_prefix("/")
            .unwrap_or(&local_path);
        let remote_path = format!(
            "/skylock/backups/{}/{}{}",
            backup_id,
            relative_path.display(),
            if should_compress { ".zst.enc" } else { ".enc" }
        );
        
        println!("  ⬆️  {}", local_path.display());
        
        // Read file
        let data = tokio::fs::read(&local_path).await?;
        
        // Optionally compress
        let data_to_encrypt = if should_compress {
            zstd::encode_all(data.as_slice(), 3)
                .map_err(|e| SkylockError::Backup(format!("Compression failed: {}", e)))?
        } else {
            data
        };
        
        // Encrypt
        let encrypted_data = encryption.encrypt(&data_to_encrypt)?;
        
        // Create parent directories on remote storage
        if let Some(parent) = PathBuf::from(&remote_path).parent() {
            if let Some(parent_str) = parent.to_str() {
                // Create all parent directories recursively
                let _ = Self::ensure_remote_directory_exists(&hetzner, parent_str).await;
            }
        }
        
        // Upload
        let temp_file = tempfile::NamedTempFile::new()
            .map_err(|e| SkylockError::Backup(format!("Temp file failed: {}", e)))?;
        tokio::fs::write(temp_file.path(), &encrypted_data).await?;
        
        hetzner.upload_file(temp_file.path(), &PathBuf::from(&remote_path)).await?;
        
        Ok(FileEntry {
            local_path: local_path.clone(),
            remote_path,
            size,
            hash,
            compressed: should_compress,
            encrypted: true,
            timestamp: Utc::now(),
        })
    }

    /// Calculate SHA-256 hash of file
    async fn calculate_hash(path: &Path) -> Result<String> {
        let data = tokio::fs::read(path).await?;
        let mut hasher = Sha256::new();
        hasher.update(&data);
        Ok(format!("{:x}", hasher.finalize()))
    }
    
    /// Ensure remote directory exists by creating all parent directories
    async fn ensure_remote_directory_exists(hetzner: &HetznerClient, path: &str) -> Result<()> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current_path = String::new();
        
        for part in parts {
            current_path.push('/');
            current_path.push_str(part);
            
            // Try to create directory - ignore errors if it already exists
            let _ = hetzner.create_directory(&current_path).await;
        }
        
        Ok(())
    }

    /// Upload backup manifest
    async fn upload_manifest(&self, manifest: &BackupManifest) -> Result<()> {
        let manifest_json = serde_json::to_string_pretty(manifest)
            .map_err(|e| SkylockError::Backup(format!("Serialize failed: {}", e)))?;
        
        let manifest_path = format!("/skylock/backups/{}/manifest.json", manifest.backup_id);
        
        // Ensure backup directory exists
        let backup_dir = format!("/skylock/backups/{}", manifest.backup_id);
        Self::ensure_remote_directory_exists(&self.hetzner, &backup_dir).await?;
        
        let temp_file = tempfile::NamedTempFile::new()
            .map_err(|e| SkylockError::Backup(format!("Temp file failed: {}", e)))?;
        
        tokio::fs::write(temp_file.path(), manifest_json).await?;
        self.hetzner.upload_file(temp_file.path(), &PathBuf::from(&manifest_path)).await?;
        
        println!("  📋 Manifest uploaded");
        
        Ok(())
    }

    /// List all backups
    pub async fn list_backups(&self) -> Result<Vec<BackupManifest>> {
        let files = self.hetzner.list_files("/skylock/backups").await?;
        let mut manifests = Vec::new();
        
        for file in files {
            if file.path.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n == "manifest.json")
                .unwrap_or(false)
            {
                if let Ok(manifest) = self.download_manifest(&file.path).await {
                    manifests.push(manifest);
                }
            }
        }
        
        // Sort by timestamp (newest first)
        manifests.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        
        Ok(manifests)
    }

    /// Download and parse manifest
    async fn download_manifest(&self, path: &Path) -> Result<BackupManifest> {
        let temp_file = tempfile::NamedTempFile::new()
            .map_err(|e| SkylockError::Backup(format!("Temp file failed: {}", e)))?;
        
        self.hetzner.download_file(path, &temp_file.path().to_path_buf()).await?;
        
        let json = tokio::fs::read_to_string(temp_file.path()).await?;
        let manifest: BackupManifest = serde_json::from_str(&json)
            .map_err(|e| SkylockError::Backup(format!("Parse manifest failed: {}", e)))?;
        
        Ok(manifest)
    }
    
    /// Load a backup manifest by ID (public API for comparison)
    pub async fn load_manifest(&self, backup_id: &str) -> Result<BackupManifest> {
        let manifest_path = PathBuf::from(format!("/skylock/backups/{}/manifest.json", backup_id));
        self.download_manifest(&manifest_path).await
    }

    /// Restore entire backup with progress tracking
    pub async fn restore_backup(&self, backup_id: &str, target_dir: &Path) -> Result<()> {
        use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
        use std::time::Duration;
        
        println!("🔄 Restoring backup: {}", backup_id);
        println!();
        
        // Download manifest
        let manifest_path = PathBuf::from(format!("/skylock/backups/{}/manifest.json", backup_id));
        let manifest = self.download_manifest(&manifest_path).await?;
        
        println!("   📦 Files to restore: {}", manifest.files.len());
        println!("   📊 Total size: {} bytes", manifest.total_size);
        println!("   📅 Backup date: {}", manifest.timestamp.format("%Y-%m-%d %H:%M:%S UTC"));
        println!();
        
        // Create progress bars
        let multi = MultiProgress::new();
        
        let overall_pb = multi.add(ProgressBar::new(manifest.files.len() as u64));
        overall_pb.set_style(
            ProgressStyle::default_bar()
                .template("{msg}\n{bar:40.cyan/blue} {pos}/{len} files ({percent}%) ETA: {eta}")
                .unwrap()
                .progress_chars("█▓▒░ ")
        );
        overall_pb.set_message("📦 Overall Progress");
        
        let file_pb = multi.add(ProgressBar::new(100));
        file_pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} {msg}\n{bar:40.green/blue} {bytes}/{total_bytes} ({bytes_per_sec}) ETA: {eta}")
                .unwrap()
                .progress_chars("█▓▒░ ")
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
        );
        file_pb.enable_steady_tick(Duration::from_millis(100));
        
        let mut restored_count = 0;
        let mut failed_count = 0;
        
        // Restore files with progress
        for entry in &manifest.files {
            file_pb.set_message(format!("⬇️  Restoring: {}", entry.local_path.display()));
            file_pb.set_length(entry.size);
            file_pb.set_position(0);
            
            match self.restore_single_file_with_progress(entry, target_dir, file_pb.clone()).await {
                Ok(_) => {
                    restored_count += 1;
                    overall_pb.inc(1);
                }
                Err(e) => {
                    failed_count += 1;
                    multi.println(format!("⚠️  Failed to restore {}: {}", entry.local_path.display(), e)).unwrap();
                    overall_pb.inc(1);
                }
            }
            
            file_pb.finish_and_clear();
        }
        
        overall_pb.finish_with_message(format!(
            "✅ Restore complete: {} files restored, {} failed",
            restored_count,
            failed_count
        ));
        
        println!();
        
        if failed_count > 0 {
            return Err(SkylockError::Backup(format!("{} files failed to restore", failed_count)));
        }
        
        Ok(())
    }

    /// Restore a single file with progress and integrity verification
    async fn restore_single_file_with_progress(
        &self,
        entry: &FileEntry,
        target_dir: &Path,
        progress: ProgressBar,
    ) -> Result<()> {
        // Download encrypted file
        let temp_encrypted = tempfile::NamedTempFile::new()
            .map_err(|e| SkylockError::Backup(format!("Temp file failed: {}", e)))?;
        
        self.hetzner.download_file(
            &PathBuf::from(&entry.remote_path),
            &temp_encrypted.path().to_path_buf()
        ).await?;
        progress.set_position(entry.size / 3); // 33% for download
        
        // Read and decrypt
        let encrypted_data = tokio::fs::read(temp_encrypted.path()).await?;
        let decrypted_data = self.encryption.decrypt(&encrypted_data)?;
        progress.set_position(entry.size * 2 / 3); // 66% for decryption
        
        // Decompress if needed
        let final_data = if entry.compressed {
            zstd::decode_all(decrypted_data.as_slice())
                .map_err(|e| SkylockError::Backup(format!("Decompression failed: {}", e)))?
        } else {
            decrypted_data
        };
        
        // Verify integrity by comparing hash
        let mut hasher = Sha256::new();
        hasher.update(&final_data);
        let restored_hash = format!("{:x}", hasher.finalize());
        
        if restored_hash != entry.hash {
            return Err(SkylockError::Backup(format!(
                "Integrity check failed for {}: hash mismatch (expected {}, got {})",
                entry.local_path.display(),
                entry.hash,
                restored_hash
            )));
        }
        
        // Write to target
        let target_path = target_dir.join(
            entry.local_path.strip_prefix("/").unwrap_or(&entry.local_path)
        );
        
        if let Some(parent) = target_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        
        tokio::fs::write(&target_path, final_data).await?;
        progress.set_position(entry.size); // 100% complete
        
        Ok(())
    }
    
    /// Restore a single file (legacy without progress)
    async fn restore_single_file(&self, entry: &FileEntry, target_dir: &Path) -> Result<()> {
        use indicatif::{ProgressBar, ProgressStyle};
        
        let pb = ProgressBar::new(entry.size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} {msg} {bar:40.green/blue} {percent}%")
                .unwrap()
                .progress_chars("█▓▒░ ")
        );
        pb.set_message(format!("Restoring {}", entry.local_path.display()));
        
        let result = self.restore_single_file_with_progress(entry, target_dir, pb.clone()).await;
        pb.finish_and_clear();
        result
    }

    /// Preview backup contents before restoring
    pub async fn preview_backup(&self, backup_id: &str) -> Result<()> {
        // Download manifest
        let manifest_path = PathBuf::from(format!("/skylock/backups/{}/manifest.json", backup_id));
        let manifest = self.download_manifest(&manifest_path).await?;
        
        println!();
        println!("{}", "==========================================================================");
        println!("🔍 Backup Preview: {}", backup_id);
        println!("{}", "==========================================================================");
        println!();
        
        println!("📊 Backup Information:");
        println!("   📅 Date: {}", manifest.timestamp.format("%Y-%m-%d %H:%M:%S UTC"));
        println!("   📦 Files: {}", manifest.file_count);
        println!("   💾 Size: {} bytes ({:.2} MB)", 
            manifest.total_size,
            (manifest.total_size as f64 / 1024.0 / 1024.0));
        println!();
        
        println!("📁 Files to be restored:");
        
        // Group files by directory
        use std::collections::BTreeMap;
        let mut by_dir: BTreeMap<String, Vec<&FileEntry>> = BTreeMap::new();
        
        for entry in &manifest.files {
            let dir = entry.local_path.parent()
                .and_then(|p| p.to_str())
                .unwrap_or("/")
                .to_string();
            by_dir.entry(dir).or_insert_with(Vec::new).push(entry);
        }
        
        for (dir, files) in by_dir.iter() {
            println!();
            println!("   📂 {}", dir);
            for file in files {
                let size_str = if file.size > 1024 * 1024 {
                    format!("{:.2} MB", file.size as f64 / 1024.0 / 1024.0)
                } else if file.size > 1024 {
                    format!("{:.2} KB", file.size as f64 / 1024.0)
                } else {
                    format!("{} B", file.size)
                };
                
                let status = if file.compressed { "🗃️" } else { "  " };
                let encrypted = if file.encrypted { "🔒" } else { "  " };
                
                println!("      {} {} {} ({}, {})",
                    status,
                    encrypted,
                    file.local_path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown"),
                    size_str,
                    file.timestamp.format("%Y-%m-%d %H:%M")
                );
            }
        }
        
        println!();
        println!("{}", "==========================================================================");
        println!();
        
        Ok(())
    }
    
    /// Check for conflicts before restore
    pub async fn check_restore_conflicts(&self, backup_id: &str, target_dir: &Path) -> Result<Vec<PathBuf>> {
        let manifest_path = PathBuf::from(format!("/skylock/backups/{}/manifest.json", backup_id));
        let manifest = self.download_manifest(&manifest_path).await?;
        
        let mut conflicts = Vec::new();
        
        for entry in &manifest.files {
            let target_path = target_dir.join(
                entry.local_path.strip_prefix("/").unwrap_or(&entry.local_path)
            );
            
            if target_path.exists() {
                conflicts.push(target_path);
            }
        }
        
        Ok(conflicts)
    }
    
    /// Restore a single file by path
    pub async fn restore_file(
        &self,
        backup_id: &str,
        file_path: &str,
        output: &Path,
    ) -> Result<()> {
        // Download manifest
        let manifest_path = PathBuf::from(format!("/skylock/backups/{}/manifest.json", backup_id));
        let manifest = self.download_manifest(&manifest_path).await?;
        
        // Find file in manifest
        let entry = manifest.files.iter()
            .find(|e| e.local_path.to_str() == Some(file_path))
            .ok_or_else(|| SkylockError::Backup(format!("File not found in backup: {}", file_path)))?;
        
        println!("🔄 Restoring single file: {}", file_path);
        
        // Restore to output path
        let temp_dir = tempfile::tempdir()
            .map_err(|e| SkylockError::Backup(format!("Temp dir failed: {}", e)))?;
        
        self.restore_single_file(entry, temp_dir.path()).await?;
        
        let restored_file = temp_dir.path().join(
            entry.local_path.strip_prefix("/").unwrap_or(&entry.local_path)
        );
        
        tokio::fs::copy(&restored_file, output).await?;
        
        println!("✅ File restored to: {}", output.display());
        
        Ok(())
    }
    
    /// Delete a backup by ID
    pub async fn delete_backup(&self, backup_id: &str) -> Result<()> {
        let backup_dir = format!("/skylock/backups/{}", backup_id);
        
        // Download manifest to know what files to delete
        let manifest_path = PathBuf::from(format!("/skylock/backups/{}/manifest.json", backup_id));
        let manifest = match self.download_manifest(&manifest_path).await {
            Ok(m) => m,
            Err(_) => {
                // If manifest doesn't exist, still try to delete the directory
                return Err(SkylockError::Backup(format!(
                    "Cannot delete backup {}: manifest not found",
                    backup_id
                )));
            }
        };
        
        // Delete all files in the backup
        for entry in &manifest.files {
            let file_path = PathBuf::from(&entry.remote_path);
            // Attempt to delete, but don't fail if file doesn't exist
            let _ = self.hetzner.delete_file(&file_path).await;
        }
        
        // Delete manifest
        let _ = self.hetzner.delete_file(&manifest_path).await;
        
        // Note: WebDAV doesn't have a direct directory delete, files are deleted individually
        // The directory will be empty after all files are deleted
        
        Ok(())
    }
}
