use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};
use tokio::fs;
use skylock_core::Result;
use sha2::{Sha256, Digest};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use tokio::io::AsyncReadExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileBlock {
    pub checksum: String,
    pub size: u64,
    pub references: HashSet<PathBuf>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub blocks: Vec<String>, // Checksums of blocks
    pub total_size: u64,
    pub modified: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

pub struct Deduplicator {
    base_path: PathBuf,
    block_size: usize,
    blocks: HashMap<String, FileBlock>,
    files: HashMap<PathBuf, FileEntry>,
    storage_path: PathBuf,
}

impl Deduplicator {
    pub fn new(base_path: PathBuf, block_size: usize) -> Self {
        let storage_path = base_path.join("dedup_storage");

        Self {
            base_path,
            block_size,
            blocks: HashMap::new(),
            files: HashMap::new(),
            storage_path,
        }
    }

    pub async fn process_file(&mut self, path: &Path) -> Result<()> {
        let metadata = fs::metadata(path).await?;
        let modified = DateTime::from(metadata.modified()?);

        // Check if file has changed
        if let Some(existing) = self.files.get(path) {
            if existing.modified == modified {
                return Ok(());
            }
        }

        let mut file = fs::File::open(path).await?;
        let mut blocks = Vec::new();
        let mut total_size = 0;

        loop {
            let mut buffer = vec![0; self.block_size];
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            buffer.truncate(n);

            let checksum = self.calculate_block_checksum(&buffer);
            blocks.push(checksum.clone());
            total_size += n as u64;

            // Store block if it's new
            if !self.blocks.contains_key(&checksum) {
                self.store_block(&checksum, &buffer).await?;

                self.blocks.insert(checksum.clone(), FileBlock {
                    checksum: checksum.clone(),
                    size: n as u64,
                    references: HashSet::new(),
                    timestamp: Utc::now(),
                });
            }

            // Update block references
            if let Some(block) = self.blocks.get_mut(&checksum) {
                block.references.insert(path.to_path_buf());
            }
        }

        // Update file entry
        let file_entry = FileEntry {
            path: path.to_path_buf(),
            blocks,
            total_size,
            modified,
            metadata: HashMap::new(),
        };

        self.files.insert(path.to_path_buf(), file_entry);

        Ok(())
    }

    pub async fn reconstruct_file(&self, path: &Path, output_path: &Path) -> Result<()> {
        let file_entry = self.files.get(path)
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "File not found in deduplication storage"
            ))?;

        // Create parent directories if needed
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut output_file = fs::File::create(output_path).await?;

        for block_checksum in &file_entry.blocks {
            let block_path = self.get_block_path(block_checksum);
            let block_data = fs::read(&block_path).await?;

            tokio::io::AsyncWriteExt::write_all(&mut output_file, &block_data).await?;
        }

        Ok(())
    }

    pub async fn remove_file(&mut self, path: &Path) -> Result<()> {
        if let Some(file_entry) = self.files.remove(path) {
            for block_checksum in file_entry.blocks {
                if let Some(block) = self.blocks.get_mut(&block_checksum) {
                    block.references.remove(path);

                    // Remove block if it has no references
                    if block.references.is_empty() {
                        self.blocks.remove(&block_checksum);
                        let block_path = self.get_block_path(&block_checksum);
                        if block_path.exists() {
                            fs::remove_file(block_path).await?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn optimize_storage(&mut self) -> Result<()> {
        // Remove unreferenced blocks
        let mut unreferenced = Vec::new();

        for (checksum, block) in &self.blocks {
            if block.references.is_empty() {
                unreferenced.push(checksum.clone());
            }
        }

        for checksum in unreferenced {
            self.blocks.remove(&checksum);
            let block_path = self.get_block_path(&checksum);
            if block_path.exists() {
                fs::remove_file(block_path).await?;
            }
        }

        Ok(())
    }

    pub async fn load_state(&mut self) -> Result<()> {
        let blocks_path = self.storage_path.join("blocks.json");
        let files_path = self.storage_path.join("files.json");

        if blocks_path.exists() {
            let blocks_data = fs::read_to_string(blocks_path).await?;
            self.blocks = serde_json::from_str(&blocks_data)?;
        }

        if files_path.exists() {
            let files_data = fs::read_to_string(files_path).await?;
            self.files = serde_json::from_str(&files_data)?;
        }

        Ok(())
    }

    pub async fn save_state(&self) -> Result<()> {
        fs::create_dir_all(&self.storage_path).await?;

        let blocks_path = self.storage_path.join("blocks.json");
        let files_path = self.storage_path.join("files.json");

        let blocks_data = serde_json::to_string_pretty(&self.blocks)?;
        let files_data = serde_json::to_string_pretty(&self.files)?;

        fs::write(blocks_path, blocks_data).await?;
        fs::write(files_path, files_data).await?;

        Ok(())
    }

    fn calculate_block_checksum(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    fn get_block_path(&self, checksum: &str) -> PathBuf {
        // Use first 2 characters as directory name for better distribution
        let dir_name = &checksum[0..2];
        self.storage_path.join("blocks").join(dir_name).join(checksum)
    }

    async fn store_block(&self, checksum: &str, data: &[u8]) -> Result<()> {
        let block_path = self.get_block_path(checksum);

        if let Some(parent) = block_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::write(block_path, data).await?;
        Ok(())
    }

    pub fn get_storage_stats(&self) -> StorageStats {
        let mut stats = StorageStats {
            total_files: self.files.len(),
            total_blocks: self.blocks.len(),
            total_size: 0,
            deduped_size: 0,
            space_saved: 0,
        };

        // Calculate total original size
        for file in self.files.values() {
            stats.total_size += file.total_size;
        }

        // Calculate deduplicated size
        for block in self.blocks.values() {
            stats.deduped_size += block.size;
        }

        stats.space_saved = stats.total_size - stats.deduped_size;

        stats
    }
}

#[derive(Debug, Clone)]
pub struct StorageStats {
    pub total_files: usize,
    pub total_blocks: usize,
    pub total_size: u64,
    pub deduped_size: u64,
    pub space_saved: u64,
}

impl StorageStats {
    pub fn get_savings_percentage(&self) -> f64 {
        if self.total_size == 0 {
            return 0.0;
        }
        (self.space_saved as f64 / self.total_size as f64) * 100.0
    }
}
