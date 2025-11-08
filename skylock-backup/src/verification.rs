//! Backup verification module
//!
//! Verifies backup integrity by checking manifests, file existence, and optionally
//! downloading files to verify their content hashes.

use crate::error::{Result, SkylockError};
use crate::direct_upload::{BackupManifest, FileEntry};
use skylock_hetzner::HetznerClient;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use indicatif::{ProgressBar, ProgressStyle};

/// Verification result for a single file
#[derive(Debug, Clone)]
pub struct FileVerification {
    /// File path
    pub path: PathBuf,
    /// Whether file exists on remote storage
    pub exists: bool,
    /// Whether hash was verified (if downloaded)
    pub hash_verified: Option<bool>,
    /// Error message if verification failed
    pub error: Option<String>,
}

/// Overall verification result
#[derive(Debug)]
pub struct VerificationResult {
    /// Backup ID that was verified
    pub backup_id: String,
    /// Whether manifest is valid
    pub manifest_valid: bool,
    /// Number of files in manifest
    pub total_files: usize,
    /// Number of files that exist on remote
    pub files_exist: usize,
    /// Number of files with verified hashes (if full verification)
    pub files_verified: usize,
    /// Number of files with errors
    pub files_with_errors: usize,
    /// List of file verifications
    pub file_results: Vec<FileVerification>,
    /// Overall verification passed
    pub passed: bool,
}

impl VerificationResult {
    /// Check if verification passed
    pub fn is_success(&self) -> bool {
        self.passed && self.files_with_errors == 0
    }
    
    /// Get list of missing files
    pub fn missing_files(&self) -> Vec<&FileVerification> {
        self.file_results
            .iter()
            .filter(|f| !f.exists)
            .collect()
    }
    
    /// Get list of files with hash mismatches
    pub fn corrupted_files(&self) -> Vec<&FileVerification> {
        self.file_results
            .iter()
            .filter(|f| matches!(f.hash_verified, Some(false)))
            .collect()
    }
}

/// Backup verifier
pub struct BackupVerifier {
    hetzner: Arc<HetznerClient>,
    max_parallel: usize,
}

impl BackupVerifier {
    /// Create new backup verifier
    pub fn new(hetzner: HetznerClient) -> Self {
        let max_parallel = std::thread::available_parallelism()
            .map(|n| n.get().min(4))
            .unwrap_or(2);
        
        Self {
            hetzner: Arc::new(hetzner),
            max_parallel,
        }
    }
    
    /// Verify a backup (quick check - existence only)
    pub async fn verify_quick(&self, manifest: &BackupManifest) -> Result<VerificationResult> {
        println!("🔍 Running quick verification (checking file existence)...");
        println!();
        
        let total_files = manifest.files.len();
        let pb = ProgressBar::new(total_files as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{msg}\n{bar:40.cyan/blue} {pos}/{len} files ({percent}%)")
                .unwrap()
                .progress_chars("█▓▒░ ")
        );
        pb.set_message("📂 Checking files...");
        
        let semaphore = Arc::new(Semaphore::new(self.max_parallel));
        let mut tasks = Vec::new();
        
        for file in &manifest.files {
            let sem = semaphore.clone();
            let hetzner = self.hetzner.clone();
            let remote_path = PathBuf::from(&file.remote_path);
            let local_path = file.local_path.clone();
            let pb_clone = pb.clone();
            
            let task = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                
                // Check if file exists by attempting to get metadata
                // Create a temp file path for testing
                let temp_test = tempfile::NamedTempFile::new()
                    .map_err(|_| "Failed to create temp file".to_string());
                
                let exists = match temp_test {
                    Ok(temp) => {
                        hetzner.download_file(&remote_path, &temp.path().to_path_buf())
                            .await
                            .is_ok()
                    }
                    Err(_) => false,
                };
                
                pb_clone.inc(1);
                
                FileVerification {
                    path: local_path,
                    exists,
                    hash_verified: None,
                    error: if !exists {
                        Some(format!("File not found on remote: {}", remote_path.display()))
                    } else {
                        None
                    },
                }
            });
            
            tasks.push(task);
        }
        
        let mut file_results = Vec::new();
        for task in tasks {
            match task.await {
                Ok(result) => file_results.push(result),
                Err(e) => {
                    return Err(SkylockError::Backup(format!("Verification task failed: {}", e)));
                }
            }
        }
        
        pb.finish_and_clear();
        
        let files_exist = file_results.iter().filter(|f| f.exists).count();
        let files_with_errors = file_results.iter().filter(|f| f.error.is_some()).count();
        
        Ok(VerificationResult {
            backup_id: manifest.backup_id.clone(),
            manifest_valid: true,
            total_files,
            files_exist,
            files_verified: 0,
            files_with_errors,
            file_results,
            passed: files_exist == total_files,
        })
    }
    
    /// Verify a backup (full check - download and verify hashes)
    pub async fn verify_full(
        &self,
        manifest: &BackupManifest,
        encryption: Arc<crate::encryption::EncryptionManager>,
    ) -> Result<VerificationResult> {
        println!("🔍 Running full verification (downloading and verifying hashes)...");
        println!("⚠️  This will download all backup files and may take significant time.");
        println!();
        
        let total_files = manifest.files.len();
        let pb = ProgressBar::new(total_files as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{msg}\n{bar:40.cyan/blue} {pos}/{len} files ({percent}%)")
                .unwrap()
                .progress_chars("█▓▒░ ")
        );
        pb.set_message("🔐 Verifying files...");
        
        let semaphore = Arc::new(Semaphore::new(self.max_parallel));
        let mut tasks = Vec::new();
        
        for file in &manifest.files {
            let sem = semaphore.clone();
            let hetzner = self.hetzner.clone();
            let encryption = encryption.clone();
            let remote_path = PathBuf::from(&file.remote_path);
            let local_path = file.local_path.clone();
            let expected_hash = file.hash.clone();
            let pb_clone = pb.clone();
            
            let task = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                
                // Download and verify file
                let result = Self::verify_file_hash(
                    hetzner.as_ref(),
                    &remote_path,
                    &expected_hash,
                    encryption.as_ref(),
                ).await;
                
                pb_clone.inc(1);
                
                match result {
                    Ok(verified) => FileVerification {
                        path: local_path,
                        exists: true,
                        hash_verified: Some(verified),
                        error: if !verified {
                            Some("Hash mismatch - file may be corrupted".to_string())
                        } else {
                            None
                        },
                    },
                    Err(e) => FileVerification {
                        path: local_path,
                        exists: false,
                        hash_verified: Some(false),
                        error: Some(format!("Verification failed: {}", e)),
                    },
                }
            });
            
            tasks.push(task);
        }
        
        let mut file_results = Vec::new();
        for task in tasks {
            match task.await {
                Ok(result) => file_results.push(result),
                Err(e) => {
                    return Err(SkylockError::Backup(format!("Verification task failed: {}", e)));
                }
            }
        }
        
        pb.finish_and_clear();
        
        let files_exist = file_results.iter().filter(|f| f.exists).count();
        let files_verified = file_results.iter()
            .filter(|f| matches!(f.hash_verified, Some(true)))
            .count();
        let files_with_errors = file_results.iter().filter(|f| f.error.is_some()).count();
        
        Ok(VerificationResult {
            backup_id: manifest.backup_id.clone(),
            manifest_valid: true,
            total_files,
            files_exist,
            files_verified,
            files_with_errors,
            file_results,
            passed: files_verified == total_files,
        })
    }
    
    /// Download and verify a single file's hash
    async fn verify_file_hash(
        hetzner: &HetznerClient,
        remote_path: &PathBuf,
        expected_hash: &str,
        encryption: &crate::encryption::EncryptionManager,
    ) -> Result<bool> {
        // Create temp file for download
        let temp_file = tempfile::NamedTempFile::new()
            .map_err(|e| SkylockError::Backup(format!("Failed to create temp file: {}", e)))?;
        
        // Download file
        hetzner.download_file(remote_path, &temp_file.path().to_path_buf()).await?;
        
        // Read and decrypt
        let encrypted_data = tokio::fs::read(temp_file.path()).await?;
        let decrypted_data = encryption.decrypt(&encrypted_data)?;
        
        // Decompress if needed (check for zstd magic bytes)
        let data = if decrypted_data.len() >= 4 
            && decrypted_data[0..4] == [0x28, 0xb5, 0x2f, 0xfd] {
            // Zstd compressed
            zstd::decode_all(decrypted_data.as_slice())
                .map_err(|e| SkylockError::Backup(format!("Decompression failed: {}", e)))?
        } else {
            decrypted_data
        };
        
        // Compute hash
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let computed_hash = format!("{:x}", hasher.finalize());
        
        Ok(computed_hash == expected_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_verification_result_methods() {
        let result = VerificationResult {
            backup_id: "test_backup".to_string(),
            manifest_valid: true,
            total_files: 3,
            files_exist: 2,
            files_verified: 1,
            files_with_errors: 1,
            file_results: vec![
                FileVerification {
                    path: PathBuf::from("/test1.txt"),
                    exists: true,
                    hash_verified: Some(true),
                    error: None,
                },
                FileVerification {
                    path: PathBuf::from("/test2.txt"),
                    exists: false,
                    hash_verified: None,
                    error: Some("Not found".to_string()),
                },
                FileVerification {
                    path: PathBuf::from("/test3.txt"),
                    exists: true,
                    hash_verified: Some(false),
                    error: Some("Hash mismatch".to_string()),
                },
            ],
            passed: false,
        };
        
        assert!(!result.is_success());
        assert_eq!(result.missing_files().len(), 1);
        assert_eq!(result.corrupted_files().len(), 1);
    }
}
