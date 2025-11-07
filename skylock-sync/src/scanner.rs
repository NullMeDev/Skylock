use std::path::{Path, PathBuf};
use tokio::fs;
use walkdir::WalkDir;
use async_stream::stream;
use futures::Stream;
use chrono::{DateTime, Utc};
use sha2::{Sha256, Digest};
use mime_guess::MimeGuess;
use skylock_core::Result;
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub extension: Option<String>,
    pub size: u64,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub mime_type: String,
    pub metadata: FileMetadata,
    pub hash: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub is_hidden: bool,
    pub is_system: bool,
    pub attributes: HashMap<String, String>,
    pub content_type: ContentType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ContentType {
    Document,
    Image,
    Video,
    Audio,
    Archive,
    Code,
    Database,
    Configuration,
    Binary,
    Unknown,
}

pub struct FileScanner {
    ignore_patterns: Vec<String>,
    hash_threshold: u64,  // Maximum file size to compute hash for
    progress_tx: mpsc::Sender<ScanProgress>,
}

#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub files_processed: u64,
    pub total_size_processed: u64,
    pub current_file: PathBuf,
    pub phase: ScanPhase,
}

#[derive(Debug, Clone)]
pub enum ScanPhase {
    Discovery,
    Analysis,
    Hashing,
    Metadata,
}

impl FileScanner {
    pub fn new(progress_tx: mpsc::Sender<ScanProgress>) -> Self {
        Self {
            ignore_patterns: vec![
                String::from("^\\..+"),  // Hidden files
                String::from("^thumbs\\.db$"),
                String::from("^desktop\\.ini$"),
                String::from("~$"),  // Temp files
                String::from("\\.tmp$"),
                String::from("\\.temp$"),
            ],
            hash_threshold: 100 * 1024 * 1024,  // 100MB
            progress_tx,
        }
    }

    pub fn scan_directory(&self, path: PathBuf) -> impl Stream<Item = Result<FileInfo>> + '_ {
        stream! {
            let mut total_files = 0;
            let mut total_size = 0;

            // First pass: count files and total size
            for entry in WalkDir::new(&path)
                .into_iter()
                .filter_entry(|e| !self.should_ignore(e.path()))
            {
                if let Ok(entry) = entry {
                    if entry.file_type().is_file() {
                        total_files += 1;
                        if let Ok(metadata) = entry.metadata() {
                            total_size += metadata.len();
                        }
                    }
                }
            }

            let mut files_processed = 0;
            let mut size_processed = 0;

            // Second pass: detailed scanning
            for entry in WalkDir::new(&path)
                .into_iter()
                .filter_entry(|e| !self.should_ignore(e.path()))
            {
                if let Ok(entry) = entry {
                    if entry.file_type().is_file() {
                        match self.scan_file(entry.path()).await {
                            Ok(file_info) => {
                                files_processed += 1;
                                size_processed += file_info.size;

                                // Report progress
                                let _ = self.progress_tx.send(ScanProgress {
                                    files_processed,
                                    total_size_processed: size_processed,
                                    current_file: file_info.path.clone(),
                                    phase: ScanPhase::Analysis,
                                }).await;

                                yield Ok(file_info);
                            }
                            Err(e) => {
                                yield Err(e);
                            }
                        }
                    }
                }
            }
        }
    }

    async fn scan_file(&self, path: &Path) -> Result<FileInfo> {
        let metadata = fs::metadata(path).await?;
        let modified: DateTime<Utc> = metadata.modified()?.into();
        let created: DateTime<Utc> = metadata.created()?.into();

        // Basic file info
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let extension = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());

        // Compute hash for files under threshold
        let hash = if metadata.len() <= self.hash_threshold {
            Some(self.compute_file_hash(path).await?)
        } else {
            None
        };

        // Get MIME type
        let mime_type = MimeGuess::from_path(path)
            .first_raw()
            .unwrap_or("application/octet-stream")
            .to_string();

        // Determine content type
        let content_type = self.determine_content_type(&mime_type, &extension);

        // Get file attributes
        let file_metadata = self.get_file_metadata(path, &metadata).await?;

        Ok(FileInfo {
            path: path.to_path_buf(),
            name,
            extension,
            size: metadata.len(),
            created,
            modified,
            mime_type,
            metadata: file_metadata,
            hash,
            tags: Vec::new(),
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

    fn determine_content_type(&self, mime_type: &str, extension: &Option<String>) -> ContentType {
        if let Some(ext) = extension {
            match ext.as_str() {
                "doc" | "docx" | "pdf" | "txt" | "rtf" => ContentType::Document,
                "jpg" | "jpeg" | "png" | "gif" | "bmp" => ContentType::Image,
                "mp4" | "avi" | "mov" | "wmv" => ContentType::Video,
                "mp3" | "wav" | "flac" | "m4a" => ContentType::Audio,
                "zip" | "rar" | "7z" | "tar" | "gz" => ContentType::Archive,
                "cpp" | "h" | "rs" | "py" | "js" | "java" => ContentType::Code,
                "db" | "sqlite" | "mdb" => ContentType::Database,
                "conf" | "config" | "ini" | "json" | "xml" | "yaml" => ContentType::Configuration,
                _ => {
                    if mime_type.starts_with("application/") {
                        ContentType::Binary
                    } else {
                        ContentType::Unknown
                    }
                }
            }
        } else {
            ContentType::Unknown
        }
    }

    async fn get_file_metadata(&self, path: &Path, metadata: &fs::Metadata) -> Result<FileMetadata> {
        let mut attributes = HashMap::new();

        #[cfg(windows)]
        {
            use windows::Win32::Storage::FileSystem::{GetFileAttributesW, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM};
            let path_wide: Vec<u16> = path.as_os_str().encode_wide().collect();
            let attrs = unsafe { GetFileAttributesW(path_wide.as_ptr()) };

            let is_hidden = attrs & FILE_ATTRIBUTE_HIDDEN.0 != 0;
            let is_system = attrs & FILE_ATTRIBUTE_SYSTEM.0 != 0;

            attributes.insert("windows_attributes".to_string(), attrs.to_string());

            Ok(FileMetadata {
                is_hidden,
                is_system,
                attributes,
                content_type: ContentType::Unknown, // Will be set later
            })
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let mode = metadata.mode();
            let is_hidden = path.file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.starts_with('.'))
                .unwrap_or(false);

            attributes.insert("unix_mode".to_string(), mode.to_string());

            Ok(FileMetadata {
                is_hidden,
                is_system: false,
                attributes,
                content_type: ContentType::Unknown, // Will be set later
            })
        }
    }

    fn should_ignore(&self, path: &Path) -> bool {
        if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
            self.ignore_patterns.iter().any(|pattern| {
                regex::Regex::new(pattern)
                    .map(|re| re.is_match(file_name))
                    .unwrap_or(false)
            })
        } else {
            false
        }
    }

    pub fn add_ignore_pattern(&mut self, pattern: String) {
        self.ignore_patterns.push(pattern);
    }

    pub fn set_hash_threshold(&mut self, threshold: u64) {
        self.hash_threshold = threshold;
    }
}
