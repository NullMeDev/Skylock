use std::path::{Path, PathBuf};
use std::collections::HashMap;
use tokio::fs;
use chrono::{DateTime, Utc};
use walkdir::WalkDir;
use skylock_core::{Result, SkylockError};
use serde::{Serialize, Deserialize};
use mime_guess::MimeGuess;
use sha2::{Sha256, Digest};
use tokio::sync::mpsc;
use tracing::{info, warn, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub path: PathBuf,
    pub size: u64,
    pub modified: DateTime<Utc>,
    pub created: DateTime<Utc>,
    pub mime_type: String,
    pub hash: String,
    pub category: FileCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum FileCategory {
    Document,
    Image,
    Video,
    Audio,
    Archive,
    Code,
    Database,
    System,
    Other,
}

pub struct MassSync {
    source_drives: Vec<PathBuf>,
    destination: PathBuf,
    organization_rules: OrganizationRules,
    metadata_store: MetadataStore,
    deduplication_enabled: bool,
    progress_tx: mpsc::Sender<SyncProgress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationRules {
    pub categorization: HashMap<String, FileCategory>,
    pub exclusions: Vec<String>,
    pub custom_rules: Vec<CustomRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRule {
    pub name: String,
    pub pattern: String,
    pub destination: String,
}

#[derive(Debug, Clone)]
pub struct SyncProgress {
    pub files_processed: u64,
    pub total_files: u64,
    pub current_file: PathBuf,
    pub phase: SyncPhase,
}

#[derive(Debug, Clone)]
pub enum SyncPhase {
    Scanning,
    Organizing,
    Deduplicating,
    Copying,
    Verifying,
}

impl MassSync {
    pub async fn new(
        source_drives: Vec<PathBuf>,
        destination: PathBuf,
        progress_tx: mpsc::Sender<SyncProgress>,
    ) -> Result<Self> {
        let organization_rules = OrganizationRules::default();
        let metadata_store = MetadataStore::new(&destination).await?;

        Ok(Self {
            source_drives,
            destination,
            organization_rules,
            metadata_store,
            deduplication_enabled: true,
            progress_tx,
        })
    }

    pub async fn start_sync(&mut self) -> Result<()> {
        // Phase 1: Scan all drives
        let mut files = self.scan_drives().await?;

        // Phase 2: Organize files by category
        self.organize_files(&mut files).await?;

        // Phase 3: Deduplicate if enabled
        if self.deduplication_enabled {
            self.deduplicate_files(&mut files).await?;
        }

        // Phase 4: Copy files to destination
        self.copy_files(&files).await?;

        // Phase 5: Verify copied files
        self.verify_files(&files).await?;

        Ok(())
    }

    async fn scan_drives(&self) -> Result<Vec<FileMetadata>> {
        let mut files = Vec::new();
        let mut total_files = 0;

        // Count total files first
        for drive in &self.source_drives {
            for entry in WalkDir::new(drive)
                .into_iter()
                .filter_entry(|e| !self.should_exclude(e.path()))
            {
                total_files += 1;
            }
        }

        // Now scan with progress updates
        let mut files_processed = 0;
        for drive in &self.source_drives {
            for entry in WalkDir::new(drive)
                .into_iter()
                .filter_entry(|e| !self.should_exclude(e.path()))
            {
                let entry = entry?;
                if entry.file_type().is_file() {
                    let metadata = self.get_file_metadata(entry.path()).await?;
                    files.push(metadata);
                }

                files_processed += 1;
                self.progress_tx.send(SyncProgress {
                    files_processed,
                    total_files,
                    current_file: entry.path().to_path_buf(),
                    phase: SyncPhase::Scanning,
                }).await?;
            }
        }

        Ok(files)
    }

    async fn organize_files(&self, files: &mut Vec<FileMetadata>) -> Result<()> {
        let total_files = files.len() as u64;

        for (i, file) in files.iter_mut().enumerate() {
            // Apply organization rules
            file.category = self.determine_category(&file.path);

            // Update progress
            self.progress_tx.send(SyncProgress {
                files_processed: i as u64,
                total_files,
                current_file: file.path.clone(),
                phase: SyncPhase::Organizing,
            }).await?;
        }

        Ok(())
    }

    async fn deduplicate_files(&self, files: &mut Vec<FileMetadata>) -> Result<()> {
        let total_files = files.len() as u64;
        let mut seen_hashes = HashMap::new();
        let mut duplicates = Vec::new();

        for (i, file) in files.iter().enumerate() {
            if let Some(original) = seen_hashes.get(&file.hash) {
                duplicates.push((file.path.clone(), original.clone()));
            } else {
                seen_hashes.insert(file.hash.clone(), file.path.clone());
            }

            // Update progress
            self.progress_tx.send(SyncProgress {
                files_processed: i as u64,
                total_files,
                current_file: file.path.clone(),
                phase: SyncPhase::Deduplicating,
            }).await?;
        }

        // Remove duplicates from the file list
        files.retain(|f| seen_hashes.get(&f.hash).map_or(false, |p| p == &f.path));

        // Create hard links for duplicates
        for (duplicate, original) in duplicates {
            if let Some(parent) = duplicate.parent() {
                fs::create_dir_all(parent).await?;
            }
            fs::hard_link(original, duplicate).await?;
        }

        Ok(())
    }

    async fn copy_files(&self, files: &[FileMetadata]) -> Result<()> {
        let total_files = files.len() as u64;

        for (i, file) in files.iter().enumerate() {
            let dest_path = self.get_destination_path(file);

            // Create destination directory
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).await?;
            }

            // Copy file
            fs::copy(&file.path, &dest_path).await?;

            // Update metadata store
            self.metadata_store.add_file(file.clone()).await?;

            // Update progress
            self.progress_tx.send(SyncProgress {
                files_processed: i as u64,
                total_files,
                current_file: file.path.clone(),
                phase: SyncPhase::Copying,
            }).await?;
        }

        Ok(())
    }

    async fn verify_files(&self, files: &[FileMetadata]) -> Result<()> {
        let total_files = files.len() as u64;

        for (i, file) in files.iter().enumerate() {
            let dest_path = self.get_destination_path(file);

            // Verify file exists
            if !dest_path.exists() {
                error!("Verification failed: file not found at {:?}", dest_path);
                continue;
            }

            // Verify size
            let metadata = fs::metadata(&dest_path).await?;
            if metadata.len() != file.size {
                error!("Verification failed: size mismatch for {:?}", dest_path);
                continue;
            }

            // Verify hash
            let hash = self.compute_file_hash(&dest_path).await?;
            if hash != file.hash {
                error!("Verification failed: hash mismatch for {:?}", dest_path);
                continue;
            }

            // Update progress
            self.progress_tx.send(SyncProgress {
                files_processed: i as u64,
                total_files,
                current_file: file.path.clone(),
                phase: SyncPhase::Verifying,
            }).await?;
        }

        Ok(())
    }

    fn should_exclude(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        self.organization_rules.exclusions.iter().any(|pattern| {
            if let Ok(regex) = regex::Regex::new(pattern) {
                regex.is_match(&path_str)
            } else {
                false
            }
        })
    }

    async fn get_file_metadata(&self, path: &Path) -> Result<FileMetadata> {
        let metadata = fs::metadata(path).await?;
        let hash = self.compute_file_hash(path).await?;

        Ok(FileMetadata {
            path: path.to_path_buf(),
            size: metadata.len(),
            modified: metadata.modified()?.into(),
            created: metadata.created()?.into(),
            mime_type: MimeGuess::from_path(path)
                .first_raw()
                .unwrap_or("application/octet-stream")
                .to_string(),
            hash,
            category: self.determine_category(path),
        })
    }

    async fn compute_file_hash(&self, path: &Path) -> Result<String> {
        let mut file = fs::File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 1024 * 1024]; // 1MB buffer

        while let Ok(n) = file.read(&mut buffer).await {
            if n == 0 { break; }
            hasher.update(&buffer[..n]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    fn determine_category(&self, path: &Path) -> FileCategory {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        self.organization_rules.categorization
            .get(&ext)
            .cloned()
            .unwrap_or(FileCategory::Other)
    }

    fn get_destination_path(&self, file: &FileMetadata) -> PathBuf {
        let mut dest = self.destination.clone();

        // Add category subdirectory
        dest.push(match file.category {
            FileCategory::Document => "Documents",
            FileCategory::Image => "Images",
            FileCategory::Video => "Videos",
            FileCategory::Audio => "Audio",
            FileCategory::Archive => "Archives",
            FileCategory::Code => "Code",
            FileCategory::Database => "Databases",
            FileCategory::System => "System",
            FileCategory::Other => "Other",
        });

        // Apply custom rules if any match
        for rule in &self.organization_rules.custom_rules {
            if let Ok(regex) = regex::Regex::new(&rule.pattern) {
                if regex.is_match(&file.path.to_string_lossy()) {
                    dest = PathBuf::from(&rule.destination);
                    break;
                }
            }
        }

        // Add original filename
        dest.push(file.path.file_name().unwrap_or_default());
        dest
    }
}
