use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub extension: Option<String>,
    pub size: u64,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub accessed: DateTime<Utc>,
    pub metadata: FileMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub content_type: ContentType,
    pub attributes: HashMap<String, String>,
    pub checksum: Option<String>,
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
    Email,
    Configuration,
    Log,
    Other(String),
}

impl FileInfo {
    pub async fn from_path(path: &PathBuf) -> skylock_core::Result<Self> {
        let metadata = fs::metadata(path).await?;
        let name = path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let extension = path.extension()
            .map(|e| e.to_string_lossy().into_owned());

        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;

        #[cfg(windows)]
        use std::os::windows::fs::MetadataExt;

        let created = metadata.created()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now());

        let modified = metadata.modified()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now());

        let accessed = metadata.accessed()
            .map(|t| DateTime::<Utc>::from(t))
            .unwrap_or_else(|_| Utc::now());

        let content_type = detect_content_type(path, &extension).await;

        Ok(Self {
            path: path.clone(),
            name,
            extension,
            size: metadata.len(),
            created,
            modified,
            accessed,
            metadata: FileMetadata {
                content_type,
                attributes: HashMap::new(),
                checksum: None,
            },
        })
    }

    pub async fn update_checksum(&mut self) -> skylock_core::Result<()> {
        use sha2::{Sha256, Digest};
        use tokio::io::AsyncReadExt;

        let mut file = fs::File::open(&self.path).await?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await?;

        let mut hasher = Sha256::new();
        hasher.update(&buffer);
        let result = hasher.finalize();

        self.metadata.checksum = Some(format!("{:x}", result));
        Ok(())
    }
}

async fn detect_content_type(path: &PathBuf, extension: &Option<String>) -> ContentType {
    if let Some(ext) = extension {
        match ext.to_lowercase().as_str() {
            // Documents
            "doc" | "docx" | "pdf" | "txt" | "rtf" | "odt" | "md" | "pages" => ContentType::Document,

            // Images
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tiff" | "webp" | "svg" => ContentType::Image,

            // Video
            "mp4" | "avi" | "mov" | "wmv" | "flv" | "mkv" | "webm" => ContentType::Video,

            // Audio
            "mp3" | "wav" | "ogg" | "m4a" | "flac" | "aac" => ContentType::Audio,

            // Archives
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" => ContentType::Archive,

            // Code
            "rs" | "py" | "js" | "ts" | "cpp" | "c" | "h" | "java" | "go" | "rb" => ContentType::Code,

            // Database
            "db" | "sqlite" | "sqlite3" | "mdb" | "sql" => ContentType::Database,

            // Email
            "eml" | "msg" => ContentType::Email,

            // Configuration
            "json" | "yaml" | "yml" | "toml" | "ini" | "conf" | "cfg" => ContentType::Configuration,

            // Logs
            "log" => ContentType::Log,

            // Other
            _ => {
                // Try to detect by content if needed
                if let Ok(content_type) = detect_by_content(path).await {
                    content_type
                } else {
                    ContentType::Other(ext.clone())
                }
            }
        }
    } else {
        // No extension, try to detect by content
        detect_by_content(path).await.unwrap_or_else(|_| ContentType::Other("unknown".to_string()))
    }
}

async fn detect_by_content(path: &PathBuf) -> skylock_core::Result<ContentType> {
    use tokio::io::AsyncReadExt;

    let mut file = fs::File::open(path).await?;
    let mut buffer = vec![0; 512]; // Read first 512 bytes for magic number detection
    file.read_exact(&mut buffer).await?;

    // Simple magic number detection
    match &buffer[0..4] {
        // PNG
        [0x89, 0x50, 0x4E, 0x47] => Ok(ContentType::Image),
        // JPEG
        [0xFF, 0xD8, 0xFF, _] => Ok(ContentType::Image),
        // GIF
        [0x47, 0x49, 0x46, 0x38] => Ok(ContentType::Image),
        // PDF
        [0x25, 0x50, 0x44, 0x46] => Ok(ContentType::Document),
        // ZIP
        [0x50, 0x4B, 0x03, 0x04] => Ok(ContentType::Archive),
        // Other formats...
        _ => {
            // Try to detect text files
            if buffer.iter().all(|&b| b.is_ascii()) {
                Ok(ContentType::Document)
            } else {
                Ok(ContentType::Other("unknown".to_string()))
            }
        }
    }
}
