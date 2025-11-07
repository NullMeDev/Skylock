//! Content-addressable storage with SHA-256 hashing for block-level deduplication
//!
//! This module provides efficient deduplication by splitting files into blocks,
//! computing content hashes, and storing only unique blocks.

use std::io::{Read, Write};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Deduplication errors
#[derive(Error, Debug)]
pub enum DeduplicationError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Block not found: {0}")]
    BlockNotFound(String),
    #[error("Invalid block size: {0}")]
    InvalidBlockSize(usize),
    #[error("Checksum mismatch")]
    ChecksumMismatch,
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Content hash type (SHA-256)
pub type ContentHash = [u8; 32];

/// Block reference with hash and size
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlockRef {
    pub hash: ContentHash,
    pub size: usize,
    pub offset: u64,
}

/// File metadata for deduplicated storage
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileMetadata {
    pub path: PathBuf,
    pub size: u64,
    pub blocks: Vec<BlockRef>,
    pub block_size: usize,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub modified_at: chrono::DateTime<chrono::Utc>,
}

/// Block storage statistics
#[derive(Debug, Clone)]
pub struct DeduplicationStats {
    pub total_blocks: u64,
    pub unique_blocks: u64,
    pub total_size: u64,
    pub unique_size: u64,
    pub deduplication_ratio: f64,
    pub space_saved: u64,
}

impl DeduplicationStats {
    /// Calculate deduplication ratio
    pub fn calculate_ratio(total_size: u64, unique_size: u64) -> f64 {
        if total_size == 0 {
            0.0
        } else {
            1.0 - (unique_size as f64 / total_size as f64)
        }
    }
}

/// Content-addressable storage for deduplicated blocks
pub struct ContentAddressableStorage {
    storage_path: PathBuf,
    block_size: usize,
    blocks: HashMap<ContentHash, usize>, // hash -> size
    block_refs: HashMap<ContentHash, u32>, // hash -> reference count
}

impl ContentAddressableStorage {
    /// Create new content-addressable storage
    pub fn new<P: AsRef<Path>>(storage_path: P, block_size: usize) -> Result<Self, DeduplicationError> {
        let storage_dir = storage_path.as_ref().to_path_buf();
        std::fs::create_dir_all(&storage_dir)?;
        
        // Validate block size
        if block_size < 1024 || block_size > 1024 * 1024 * 64 {
            return Err(DeduplicationError::InvalidBlockSize(block_size));
        }
        
        let mut cas = ContentAddressableStorage {
            storage_path: storage_dir,
            block_size,
            blocks: HashMap::new(),
            block_refs: HashMap::new(),
        };
        
        cas.load_index()?;
        Ok(cas)
    }
    
    /// Store a block and return its content hash
    pub fn store_block(&mut self, data: &[u8]) -> Result<ContentHash, DeduplicationError> {
        let hash = Self::compute_hash(data);
        
        // Check if block already exists
        if self.blocks.contains_key(&hash) {
            // Increment reference count
            *self.block_refs.entry(hash).or_insert(0) += 1;
            return Ok(hash);
        }
        
        // Store new block
        let block_path = self.get_block_path(&hash);
        if let Some(parent) = block_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        std::fs::write(&block_path, data)?;
        
        // Update index
        self.blocks.insert(hash, data.len());
        self.block_refs.insert(hash, 1);
        
        Ok(hash)
    }
    
    /// Retrieve a block by its content hash
    pub fn get_block(&self, hash: &ContentHash) -> Result<Vec<u8>, DeduplicationError> {
        if !self.blocks.contains_key(hash) {
            return Err(DeduplicationError::BlockNotFound(hex::encode(hash)));
        }
        
        let block_path = self.get_block_path(hash);
        let data = std::fs::read(&block_path)?;
        
        // Verify integrity
        let computed_hash = Self::compute_hash(&data);
        if computed_hash != *hash {
            return Err(DeduplicationError::ChecksumMismatch);
        }
        
        Ok(data)
    }
    
    /// Check if a block exists
    pub fn has_block(&self, hash: &ContentHash) -> bool {
        self.blocks.contains_key(hash)
    }
    
    /// Delete a block (decrements reference count)
    pub fn delete_block(&mut self, hash: &ContentHash) -> Result<bool, DeduplicationError> {
        if let Some(ref_count) = self.block_refs.get_mut(hash) {
            *ref_count -= 1;
            
            if *ref_count == 0 {
                // No more references, remove block
                let block_path = self.get_block_path(hash);
                std::fs::remove_file(&block_path)?;
                
                self.blocks.remove(hash);
                self.block_refs.remove(hash);
                
                Ok(true) // Block deleted
            } else {
                Ok(false) // Still has references
            }
        } else {
            Err(DeduplicationError::BlockNotFound(hex::encode(hash)))
        }
    }
    
    /// Get storage statistics
    pub fn get_stats(&self) -> DeduplicationStats {
        let unique_blocks = self.blocks.len() as u64;
        let unique_size: u64 = self.blocks.values().map(|&size| size as u64).sum();
        let total_blocks: u64 = self.block_refs.values().map(|&count| count as u64).sum();
        let total_size: u64 = self.blocks.iter()
            .map(|(hash, &size)| {
                let ref_count = self.block_refs.get(hash).copied().unwrap_or(0) as u64;
                size as u64 * ref_count
            })
            .sum();
        
        let deduplication_ratio = DeduplicationStats::calculate_ratio(total_size, unique_size);
        let space_saved = total_size.saturating_sub(unique_size);
        
        DeduplicationStats {
            total_blocks,
            unique_blocks,
            total_size,
            unique_size,
            deduplication_ratio,
            space_saved,
        }
    }
    
    /// Garbage collect unreferenced blocks
    pub fn garbage_collect(&mut self) -> Result<u64, DeduplicationError> {
        let mut removed_count = 0;
        let mut to_remove = Vec::new();
        
        for (hash, &ref_count) in &self.block_refs {
            if ref_count == 0 {
                to_remove.push(*hash);
            }
        }
        
        for hash in to_remove {
            let block_path = self.get_block_path(&hash);
            if block_path.exists() {
                std::fs::remove_file(&block_path)?;
                removed_count += 1;
            }
            
            self.blocks.remove(&hash);
            self.block_refs.remove(&hash);
        }
        
        self.save_index()?;
        Ok(removed_count)
    }
    
    /// Compute SHA-256 hash of data
    fn compute_hash(data: &[u8]) -> ContentHash {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }
    
    /// Get file path for a block
    fn get_block_path(&self, hash: &ContentHash) -> PathBuf {
        let hex_hash = hex::encode(hash);
        let (dir1, dir2) = hex_hash.split_at(2);
        let (dir2, filename) = dir2.split_at(2);
        
        self.storage_path
            .join("blocks")
            .join(dir1)
            .join(dir2)
            .join(filename)
    }
    
    /// Load block index from disk
    fn load_index(&mut self) -> Result<(), DeduplicationError> {
        let index_path = self.storage_path.join("index.json");
        
        if index_path.exists() {
            let index_data = std::fs::read_to_string(&index_path)?;
            let index: (HashMap<String, usize>, HashMap<String, u32>) = 
                serde_json::from_str(&index_data)
                    .map_err(|e| DeduplicationError::Serialization(e.to_string()))?;
            
            // Convert string keys back to ContentHash
            self.blocks = index.0
                .into_iter()
                .filter_map(|(k, v)| {
                    hex::decode(&k).ok()
                        .and_then(|bytes| bytes.try_into().ok())
                        .map(|hash| (hash, v))
                })
                .collect();
                
            self.block_refs = index.1
                .into_iter()
                .filter_map(|(k, v)| {
                    hex::decode(&k).ok()
                        .and_then(|bytes| bytes.try_into().ok())
                        .map(|hash| (hash, v))
                })
                .collect();
        }
        
        Ok(())
    }
    
    /// Save block index to disk
    fn save_index(&self) -> Result<(), DeduplicationError> {
        let index_path = self.storage_path.join("index.json");
        
        // Convert ContentHash keys to strings for JSON serialization
        let blocks_str: HashMap<String, usize> = self.blocks
            .iter()
            .map(|(k, &v)| (hex::encode(k), v))
            .collect();
            
        let refs_str: HashMap<String, u32> = self.block_refs
            .iter()
            .map(|(k, &v)| (hex::encode(k), v))
            .collect();
        
        let index = (blocks_str, refs_str);
        let index_data = serde_json::to_string_pretty(&index)
            .map_err(|e| DeduplicationError::Serialization(e.to_string()))?;
        
        std::fs::write(&index_path, index_data)?;
        Ok(())
    }
}

impl Drop for ContentAddressableStorage {
    fn drop(&mut self) {
        let _ = self.save_index();
    }
}

/// Deduplication engine for files
pub struct DeduplicationEngine {
    cas: ContentAddressableStorage,
    metadata_path: PathBuf,
    file_metadata: HashMap<PathBuf, FileMetadata>,
}

impl DeduplicationEngine {
    /// Create new deduplication engine
    pub fn new<P: AsRef<Path>>(
        storage_path: P,
        block_size: usize
    ) -> Result<Self, DeduplicationError> {
        let storage_dir = storage_path.as_ref();
        let cas = ContentAddressableStorage::new(&storage_dir, block_size)?;
        let metadata_path = storage_dir.join("metadata.json");
        
        let mut engine = DeduplicationEngine {
            cas,
            metadata_path,
            file_metadata: HashMap::new(),
        };
        
        engine.load_metadata()?;
        Ok(engine)
    }
    
    /// Store a file with deduplication
    pub fn store_file<P: AsRef<Path>, R: Read>(
        &mut self,
        file_path: P,
        mut reader: R,
    ) -> Result<FileMetadata, DeduplicationError> {
        let file_path = file_path.as_ref().to_path_buf();
        let mut blocks = Vec::new();
        let mut buffer = vec![0u8; self.cas.block_size];
        let mut total_size = 0u64;
        let mut offset = 0u64;
        
        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            
            let block_data = &buffer[..bytes_read];
            let hash = self.cas.store_block(block_data)?;
            
            blocks.push(BlockRef {
                hash,
                size: bytes_read,
                offset,
            });
            
            total_size += bytes_read as u64;
            offset += bytes_read as u64;
        }
        
        let now = chrono::Utc::now();
        let metadata = FileMetadata {
            path: file_path.clone(),
            size: total_size,
            blocks,
            block_size: self.cas.block_size,
            created_at: now,
            modified_at: now,
        };
        
        self.file_metadata.insert(file_path, metadata.clone());
        self.save_metadata()?;
        
        Ok(metadata)
    }
    
    /// Retrieve a file from deduplicated storage
    pub fn retrieve_file<P: AsRef<Path>, W: Write>(
        &self,
        file_path: P,
        mut writer: W,
    ) -> Result<u64, DeduplicationError> {
        let file_path = file_path.as_ref();
        let metadata = self.file_metadata.get(file_path)
            .ok_or_else(|| DeduplicationError::BlockNotFound(file_path.display().to_string()))?;
        
        let mut bytes_written = 0u64;
        
        for block_ref in &metadata.blocks {
            let block_data = self.cas.get_block(&block_ref.hash)?;
            
            if block_data.len() != block_ref.size {
                return Err(DeduplicationError::ChecksumMismatch);
            }
            
            writer.write_all(&block_data)?;
            bytes_written += block_data.len() as u64;
        }
        
        Ok(bytes_written)
    }
    
    /// Delete a file from deduplicated storage
    pub fn delete_file<P: AsRef<Path>>(&mut self, file_path: P) -> Result<(), DeduplicationError> {
        let file_path = file_path.as_ref();
        let metadata = self.file_metadata.remove(file_path)
            .ok_or_else(|| DeduplicationError::BlockNotFound(file_path.display().to_string()))?;
        
        // Decrement reference counts for all blocks
        for block_ref in &metadata.blocks {
            self.cas.delete_block(&block_ref.hash)?;
        }
        
        self.save_metadata()?;
        Ok(())
    }
    
    /// List all stored files
    pub fn list_files(&self) -> Vec<&FileMetadata> {
        self.file_metadata.values().collect()
    }
    
    /// Get file metadata
    pub fn get_file_metadata<P: AsRef<Path>>(&self, file_path: P) -> Option<&FileMetadata> {
        self.file_metadata.get(file_path.as_ref())
    }
    
    /// Get deduplication statistics
    pub fn get_stats(&self) -> DeduplicationStats {
        self.cas.get_stats()
    }
    
    /// Perform garbage collection
    pub fn garbage_collect(&mut self) -> Result<u64, DeduplicationError> {
        self.cas.garbage_collect()
    }
    
    /// Verify integrity of stored files
    pub fn verify_integrity(&self) -> Result<Vec<PathBuf>, DeduplicationError> {
        let mut corrupted_files = Vec::new();
        
        for metadata in self.file_metadata.values() {
            for block_ref in &metadata.blocks {
                match self.cas.get_block(&block_ref.hash) {
                    Ok(data) => {
                        if data.len() != block_ref.size {
                            corrupted_files.push(metadata.path.clone());
                            break;
                        }
                    }
                    Err(_) => {
                        corrupted_files.push(metadata.path.clone());
                        break;
                    }
                }
            }
        }
        
        Ok(corrupted_files)
    }
    
    /// Load file metadata from disk
    fn load_metadata(&mut self) -> Result<(), DeduplicationError> {
        if self.metadata_path.exists() {
            let metadata_data = std::fs::read_to_string(&self.metadata_path)?;
            let metadata: HashMap<String, FileMetadata> = serde_json::from_str(&metadata_data)
                .map_err(|e| DeduplicationError::Serialization(e.to_string()))?;
            
            self.file_metadata = metadata
                .into_iter()
                .map(|(k, v)| (PathBuf::from(k), v))
                .collect();
        }
        
        Ok(())
    }
    
    /// Save file metadata to disk
    fn save_metadata(&self) -> Result<(), DeduplicationError> {
        let metadata: HashMap<String, &FileMetadata> = self.file_metadata
            .iter()
            .map(|(k, v)| (k.display().to_string(), v))
            .collect();
        
        let metadata_data = serde_json::to_string_pretty(&metadata)
            .map_err(|e| DeduplicationError::Serialization(e.to_string()))?;
        
        std::fs::write(&self.metadata_path, metadata_data)?;
        Ok(())
    }
}

/// Deduplication analyzer for detecting duplicate files
pub struct DuplicationAnalyzer {
    block_size: usize,
}

impl DuplicationAnalyzer {
    /// Create new duplication analyzer
    pub fn new(block_size: usize) -> Self {
        DuplicationAnalyzer { block_size }
    }
    
    /// Analyze files for potential deduplication savings
    pub fn analyze_directory<P: AsRef<Path>>(
        &self,
        directory: P,
    ) -> Result<DeduplicationStats, DeduplicationError> {
        let mut all_blocks = HashMap::<ContentHash, (usize, u32)>::new(); // hash -> (size, count)
        let mut total_size = 0u64;
        
        for entry in walkdir::WalkDir::new(directory) {
            let entry = entry.map_err(|e| DeduplicationError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
            
            if entry.file_type().is_file() {
                let file_size = entry.metadata().map_err(|e| DeduplicationError::Io(e.into()))?.len();
                total_size += file_size;
                
                let blocks = self.analyze_file_blocks(entry.path())?;
                for (hash, size) in blocks {
                    let (stored_size, count) = all_blocks.entry(hash).or_insert((size, 0));
                    *count += 1;
                    // Ensure size consistency
                    assert_eq!(*stored_size, size);
                }
            }
        }
        
        let unique_size: u64 = all_blocks.values().map(|(size, _)| *size as u64).sum();
        let unique_blocks = all_blocks.len() as u64;
        let total_blocks: u64 = all_blocks.values().map(|(_, count)| *count as u64).sum();
        
        let deduplication_ratio = DeduplicationStats::calculate_ratio(total_size, unique_size);
        let space_saved = total_size.saturating_sub(unique_size);
        
        Ok(DeduplicationStats {
            total_blocks,
            unique_blocks,
            total_size,
            unique_size,
            deduplication_ratio,
            space_saved,
        })
    }
    
    /// Analyze blocks in a single file
    fn analyze_file_blocks<P: AsRef<Path>>(
        &self,
        file_path: P,
    ) -> Result<Vec<(ContentHash, usize)>, DeduplicationError> {
        let mut file = std::fs::File::open(file_path)?;
        let mut blocks = Vec::new();
        let mut buffer = vec![0u8; self.block_size];
        
        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            
            let block_data = &buffer[..bytes_read];
            let hash = ContentAddressableStorage::compute_hash(block_data);
            blocks.push((hash, bytes_read));
        }
        
        Ok(blocks)
    }
    
    /// Find duplicate files in a directory
    pub fn find_duplicates<P: AsRef<Path>>(
        &self,
        directory: P,
    ) -> Result<HashMap<ContentHash, Vec<PathBuf>>, DeduplicationError> {
        let mut file_hashes = HashMap::<ContentHash, Vec<PathBuf>>::new();
        
        for entry in walkdir::WalkDir::new(directory) {
            let entry = entry.map_err(|e| DeduplicationError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
            
            if entry.file_type().is_file() {
                let file_hash = self.compute_file_hash(entry.path())?;
                file_hashes.entry(file_hash).or_insert_with(Vec::new).push(entry.path().to_path_buf());
            }
        }
        
        // Keep only files that have duplicates
        file_hashes.retain(|_, paths| paths.len() > 1);
        
        Ok(file_hashes)
    }
    
    /// Compute hash of entire file
    fn compute_file_hash<P: AsRef<Path>>(&self, file_path: P) -> Result<ContentHash, DeduplicationError> {
        let mut file = std::fs::File::open(file_path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; self.block_size];
        
        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            
            hasher.update(&buffer[..bytes_read]);
        }
        
        Ok(hasher.finalize().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::TempDir;

    #[test]
    fn test_content_addressable_storage() {
        let temp_dir = TempDir::new().unwrap();
        let mut cas = ContentAddressableStorage::new(temp_dir.path(), 4096).unwrap();
        
        let data1 = b"Hello, world!";
        let data2 = b"Hello, world!"; // Duplicate
        let data3 = b"Different data";
        
        // Store blocks
        let hash1 = cas.store_block(data1).unwrap();
        let hash2 = cas.store_block(data2).unwrap();
        let hash3 = cas.store_block(data3).unwrap();
        
        // Same data should have same hash
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        
        // Retrieve blocks
        let retrieved1 = cas.get_block(&hash1).unwrap();
        let retrieved3 = cas.get_block(&hash3).unwrap();
        
        assert_eq!(data1, &retrieved1[..]);
        assert_eq!(data3, &retrieved3[..]);
        
        // Check stats
        let stats = cas.get_stats();
        assert_eq!(stats.unique_blocks, 2);
        assert_eq!(stats.total_blocks, 3); // 2 refs to hash1 + 1 ref to hash3
    }
    
    #[test]
    fn test_deduplication_engine() {
        let temp_dir = TempDir::new().unwrap();
        let mut engine = DeduplicationEngine::new(temp_dir.path(), 1024).unwrap();
        
        let file_content = b"This is test file content that will be deduplicated.".repeat(100);
        let file_path = PathBuf::from("test.txt");
        
        // Store file
        let cursor = Cursor::new(&file_content);
        let metadata = engine.store_file(&file_path, cursor).unwrap();
        
        assert_eq!(metadata.size, file_content.len() as u64);
        assert!(!metadata.blocks.is_empty());
        
        // Retrieve file
        let mut output = Vec::new();
        let bytes_written = engine.retrieve_file(&file_path, &mut output).unwrap();
        
        assert_eq!(bytes_written, file_content.len() as u64);
        assert_eq!(output, file_content);
        
        // Verify file exists in metadata
        assert!(engine.get_file_metadata(&file_path).is_some());
    }
    
    #[test]
    fn test_duplicate_detection() {
        let temp_dir = TempDir::new().unwrap();
        let mut engine = DeduplicationEngine::new(temp_dir.path(), 1024).unwrap();
        
        let content1 = b"Identical content for testing deduplication";
        let content2 = b"Identical content for testing deduplication"; // Same
        let content3 = b"Different content for testing";
        
        // Store files
        let path1 = PathBuf::from("file1.txt");
        let path2 = PathBuf::from("file2.txt");
        let path3 = PathBuf::from("file3.txt");
        
        engine.store_file(&path1, Cursor::new(content1)).unwrap();
        engine.store_file(&path2, Cursor::new(content2)).unwrap();
        engine.store_file(&path3, Cursor::new(content3)).unwrap();
        
        let stats = engine.get_stats();
        
        // Should have deduplication between file1 and file2
        assert!(stats.deduplication_ratio > 0.0);
        assert!(stats.unique_blocks < stats.total_blocks);
        
        println!("Deduplication ratio: {:.2}%", stats.deduplication_ratio * 100.0);
        println!("Space saved: {} bytes", stats.space_saved);
    }
    
    #[test]
    fn test_file_deletion() {
        let temp_dir = TempDir::new().unwrap();
        let mut engine = DeduplicationEngine::new(temp_dir.path(), 1024).unwrap();
        
        let content = b"Content to be deleted";
        let file_path = PathBuf::from("delete_me.txt");
        
        // Store and then delete file
        engine.store_file(&file_path, Cursor::new(content)).unwrap();
        assert!(engine.get_file_metadata(&file_path).is_some());
        
        engine.delete_file(&file_path).unwrap();
        assert!(engine.get_file_metadata(&file_path).is_none());
        
        // Should not be able to retrieve deleted file
        let mut output = Vec::new();
        let result = engine.retrieve_file(&file_path, &mut output);
        assert!(result.is_err());
    }

    #[test]
    fn test_garbage_collection() {
        let temp_dir = TempDir::new().unwrap();
        let mut engine = DeduplicationEngine::new(temp_dir.path(), 1024).unwrap();
        
        let content = b"Content for garbage collection test";
        let file_path = PathBuf::from("gc_test.txt");
        
        // Store file
        engine.store_file(&file_path, Cursor::new(content)).unwrap();
        let stats_before = engine.get_stats();
        
        // Delete file - this immediately cleans up unreferenced blocks
        engine.delete_file(&file_path).unwrap();
        let stats_after = engine.get_stats();
        
        // Garbage collect (should find no additional blocks to remove)
        let removed = engine.garbage_collect().unwrap();
        
        // Verify that blocks were cleaned up during delete
        assert!(stats_after.unique_blocks < stats_before.unique_blocks);
        // Garbage collection should find nothing to remove since cleanup already happened
        assert_eq!(removed, 0);
    }
}
