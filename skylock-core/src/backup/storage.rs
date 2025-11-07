use std::path::PathBuf;
use tokio::fs::{File, create_dir_all};
use tokio::io::AsyncWriteExt;
use std::collections::HashMap;
use crate::Result;
use super::dedup::Block;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::io::Write;

#[derive(Debug, Clone)]
pub struct BackupStorage {
    root_path: PathBuf,
    block_path: PathBuf,
    metadata_path: PathBuf,
}

impl BackupStorage {
    pub fn new(root_path: &PathBuf) -> Result<Self> {
        let block_path = root_path.join("blocks");
        let metadata_path = root_path.join("metadata");

        // Ensure directories exist
        std::fs::create_dir_all(&block_path)?;
        std::fs::create_dir_all(&metadata_path)?;

        Ok(Self {
            root_path: root_path.clone(),
            block_path,
            metadata_path,
        })
    }

    pub async fn store_block(&self, block: &Block) -> Result<()> {
        let block_file_path = self.get_block_path(&block.hash);

        // Skip if block already exists
        if block_file_path.exists() {
            return Ok(());
        }

        // Create parent directories if needed
        if let Some(parent) = block_file_path.parent() {
            create_dir_all(parent).await?;
        }

        // Compress block data
        let compressed_data = self.compress_block_data(&block.data)?;

        // Write compressed block to file
        let mut file = File::create(&block_file_path).await?;
        file.write_all(&compressed_data).await?;

        Ok(())
    }

    pub async fn get_block(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        let block_path = self.get_block_path(hash);

        if !block_path.exists() {
            return Ok(None);
        }

        let compressed_data = tokio::fs::read(&block_path).await?;
        let data = self.decompress_block_data(&compressed_data)?;

        Ok(Some(data))
    }

    pub async fn delete_block(&self, hash: &str) -> Result<()> {
        let block_path = self.get_block_path(hash);

        if block_path.exists() {
            tokio::fs::remove_file(block_path).await?;
        }

        Ok(())
    }

    pub async fn store_metadata(&self, backup_id: &str, metadata: &HashMap<String, String>) -> Result<()> {
        let metadata_path = self.metadata_path.join(format!("{}.json", backup_id));

        let json = serde_json::to_string_pretty(metadata)?;
        let mut file = File::create(&metadata_path).await?;
        file.write_all(json.as_bytes()).await?;

        Ok(())
    }

    pub async fn get_metadata(&self, backup_id: &str) -> Result<Option<HashMap<String, String>>> {
        let metadata_path = self.metadata_path.join(format!("{}.json", backup_id));

        if !metadata_path.exists() {
            return Ok(None);
        }

        let data = tokio::fs::read_to_string(&metadata_path).await?;
        let metadata = serde_json::from_str(&data)?;

        Ok(Some(metadata))
    }

    fn get_block_path(&self, hash: &str) -> PathBuf {
        // Use first 4 characters of hash for directory structure to avoid too many files in one directory
        self.block_path.join(&hash[0..2]).join(&hash[2..4]).join(hash)
    }

    fn compress_block_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data)?;
        Ok(encoder.finish()?)
    }

    fn decompress_block_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = flate2::read::GzDecoder::new(data);
        let mut decompressed = Vec::new();
        std::io::copy(&mut decoder, &mut decompressed)?;
        Ok(decompressed)
    }
}
