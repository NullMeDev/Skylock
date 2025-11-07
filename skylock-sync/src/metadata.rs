use std::path::{Path, PathBuf};
use tokio::fs;
use serde::{Serialize, Deserialize};
use skylock_core::Result;
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataStore {
    base_path: PathBuf,
    #[serde(skip)]
    index: Arc<RwLock<HashMap<String, FileMetadata>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMetadata {
    pub last_sync: chrono::DateTime<chrono::Utc>,
    pub file_count: u64,
    pub total_size: u64,
    pub categories: HashMap<FileCategory, CategoryStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryStats {
    pub file_count: u64,
    pub total_size: u64,
    pub last_modified: chrono::DateTime<chrono::Utc>,
}

impl MetadataStore {
    pub async fn new(base_path: &Path) -> Result<Self> {
        let mut store = Self {
            base_path: base_path.to_path_buf(),
            index: Arc::new(RwLock::new(HashMap::new())),
        };

        store.load_index().await?;
        Ok(store)
    }

    pub async fn add_file(&self, metadata: FileMetadata) -> Result<()> {
        let mut index = self.index.write().await;
        index.insert(metadata.hash.clone(), metadata);
        self.save_index().await?;
        Ok(())
    }

    pub async fn get_file(&self, hash: &str) -> Option<FileMetadata> {
        let index = self.index.read().await;
        index.get(hash).cloned()
    }

    pub async fn remove_file(&self, hash: &str) -> Result<()> {
        let mut index = self.index.write().await;
        index.remove(hash);
        self.save_index().await?;
        Ok(())
    }

    pub async fn get_sync_metadata(&self) -> Result<SyncMetadata> {
        let index = self.index.read().await;
        let mut categories = HashMap::new();
        let mut total_size = 0u64;

        for metadata in index.values() {
            total_size += metadata.size;

            let stats = categories
                .entry(metadata.category.clone())
                .or_insert_with(|| CategoryStats {
                    file_count: 0,
                    total_size: 0,
                    last_modified: chrono::Utc::now(),
                });

            stats.file_count += 1;
            stats.total_size += metadata.size;
            if metadata.modified > stats.last_modified {
                stats.last_modified = metadata.modified;
            }
        }

        Ok(SyncMetadata {
            last_sync: chrono::Utc::now(),
            file_count: index.len() as u64,
            total_size,
            categories,
        })
    }

    pub async fn find_duplicates(&self) -> Result<Vec<Vec<FileMetadata>>> {
        let index = self.index.read().await;
        let mut size_groups: HashMap<u64, Vec<FileMetadata>> = HashMap::new();

        // Group files by size first
        for metadata in index.values() {
            size_groups
                .entry(metadata.size)
                .or_default()
                .push(metadata.clone());
        }

        // Then check hashes within size groups
        let mut duplicates = Vec::new();
        for files in size_groups.values() {
            if files.len() > 1 {
                let mut hash_groups: HashMap<String, Vec<FileMetadata>> = HashMap::new();

                for file in files {
                    hash_groups
                        .entry(file.hash.clone())
                        .or_default()
                        .push(file.clone());
                }

                for group in hash_groups.values() {
                    if group.len() > 1 {
                        duplicates.push(group.clone());
                    }
                }
            }
        }

        Ok(duplicates)
    }

    pub async fn cleanup_missing_files(&self) -> Result<Vec<FileMetadata>> {
        let mut index = self.index.write().await;
        let mut removed = Vec::new();

        index.retain(|_, metadata| {
            let exists = metadata.path.exists();
            if !exists {
                removed.push(metadata.clone());
            }
            exists
        });

        if !removed.is_empty() {
            self.save_index().await?;
        }

        Ok(removed)
    }

    async fn load_index(&self) -> Result<()> {
        let index_path = self.base_path.join("metadata_index.json");

        if index_path.exists() {
            let data = fs::read_to_string(&index_path).await?;
            let loaded: HashMap<String, FileMetadata> = serde_json::from_str(&data)?;
            let mut index = self.index.write().await;
            *index = loaded;
        }

        Ok(())
    }

    async fn save_index(&self) -> Result<()> {
        let index_path = self.base_path.join("metadata_index.json");
        let index = self.index.read().await;
        let data = serde_json::to_string_pretty(&*index)?;
        fs::write(&index_path, data).await?;
        Ok(())
    }

    pub async fn generate_report(&self) -> Result<Report> {
        let metadata = self.get_sync_metadata().await?;
        let duplicates = self.find_duplicates().await?;

        Ok(Report {
            timestamp: chrono::Utc::now(),
            sync_metadata: metadata,
            duplicate_groups: duplicates.len(),
            duplicate_size: duplicates.iter()
                .flat_map(|group| group.iter())
                .map(|file| file.size)
                .sum(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub sync_metadata: SyncMetadata,
    pub duplicate_groups: usize,
    pub duplicate_size: u64,
}
