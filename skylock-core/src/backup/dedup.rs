use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::collections::HashMap;
use sha2::{Sha256, Digest};
use tokio::sync::RwLock;
use crate::Result;
use crate::error::{Error, ErrorCategory, ErrorSeverity};
use crate::storage::StorageProvider;
use zstd::stream::encode_all;
use serde::{Serialize, Deserialize};
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub hash: String,
    pub size: usize,
    pub data: Vec<u8>,
    pub compressed: bool,
}

#[derive(Debug)]
pub struct DedupEngine {
    block_size: usize,
    compression_level: i32,
    storage: Arc<dyn StorageProvider + Send + Sync>,
    block_cache: Arc<RwLock<HashMap<String, BlockMetadata>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockMetadata {
    pub hash: String,
    pub size: usize,
    pub compressed_size: usize,
    pub reference_count: usize,
    pub last_verified: chrono::DateTime<chrono::Utc>,
}

impl DedupEngine {
    pub fn new(
        block_size: usize,
        compression_level: i32,
        storage: Arc<dyn StorageProvider + Send + Sync>,
    ) -> Result<Self> {
        if block_size == 0 || block_size > 1024 * 1024 * 64 { // Max 64MB blocks
            return Err(Error::new(
                ErrorCategory::Configuration,
                ErrorSeverity::High,
                format!("Invalid block size: {}", block_size),
                "dedup_engine".to_string(),
            ));
        }

        if compression_level < -7 || compression_level > 22 {
            return Err(Error::new(
                ErrorCategory::Configuration,
                ErrorSeverity::High,
                format!("Invalid compression level: {}", compression_level),
                "dedup_engine".to_string(),
            ));
        }

        Ok(Self {
            block_size,
            compression_level,
            storage,
            block_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    #[tracing::instrument(skip(self, path))]
    pub async fn process_file(&self, path: &PathBuf) -> Result<Vec<Block>> {
        let mut file = File::open(path).await?;
        let mut blocks = Vec::new();
        let mut buffer = vec![0; self.block_size];

        let mut total_bytes = 0;
        let mut unique_blocks = 0;
        let mut duplicate_blocks = 0;

        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            total_bytes += n;

            let block_data = if n == self.block_size {
                buffer.clone()
            } else {
                buffer[..n].to_vec()
            };

            let hash = self.calculate_hash(&block_data);
            let block_path = PathBuf::from("blocks").join(&hash[..2]).join(&hash[2..4]).join(&hash);

            // Check if block exists in cache
            let needs_upload = {
                let cache = self.block_cache.read().await;
                !cache.contains_key(&hash)
            };

            if needs_upload {
                // Compress the block
                let compressed_data = encode_all(&block_data[..], self.compression_level)?;

                // Store the compressed block
                let block = Block {
                    hash: hash.clone(),
                    size: block_data.len(),
                    data: compressed_data,
                    compressed: true,
                };

                // Upload to storage
                self.storage.create_directory(&block_path.parent().unwrap()).await?;
                self.storage.upload(
                    &block_path,
                    Box::new(std::io::Cursor::new(block.data.clone())),
                    Default::default(),
                ).await?;

                // Update cache
                let mut cache = self.block_cache.write().await;
                cache.insert(hash.clone(), BlockMetadata {
                    hash: hash.clone(),
                    size: block.size,
                    compressed_size: block.data.len(),
                    reference_count: 1,
                    last_verified: chrono::Utc::now(),
                });

                unique_blocks += 1;
                blocks.push(block);
            } else {
                // Block exists, just update reference count
                let mut cache = self.block_cache.write().await;
                if let Some(metadata) = cache.get_mut(&hash) {
                    metadata.reference_count += 1;
                }
                duplicate_blocks += 1;

                blocks.push(Block {
                    hash: hash.clone(),
                    size: block_data.len(),
                    data: Vec::new(), // Don't store duplicate data
                    compressed: false,
                });
            }

        }

        info!(
            "Processed file: {} bytes, {} unique blocks, {} duplicate blocks",
            total_bytes, unique_blocks, duplicate_blocks
        );

        Ok(blocks)
    }

    #[tracing::instrument(skip(self, blocks))]
    pub async fn restore_blocks(&self, blocks: &[Block], output: &mut File) -> Result<()> {
        for block in blocks {
            if block.data.is_empty() {
                // Block is a reference, need to fetch from storage
                let block_path = PathBuf::from("blocks")
                    .join(&block.hash[..2])
                    .join(&block.hash[2..4])
                    .join(&block.hash);

                let data = self.storage.download(&block_path, Default::default()).await?;
                let mut reader = tokio::io::BufReader::new(data);
                let mut compressed_data = Vec::new();
                reader.read_to_end(&mut compressed_data).await?;

                // Decompress and write
                let decompressed = zstd::decode_all(&compressed_data[..])?;
                output.write_all(&decompressed).await?;
            } else {
                // Block data is included (unique block)
                if block.compressed {
                    let decompressed = zstd::decode_all(&block.data[..])?;
                    output.write_all(&decompressed).await?;
                } else {
                    output.write_all(&block.data).await?;
                }
            }
        }

        output.flush().await?;
        Ok(())
    }

    #[tracing::instrument(skip(self, data))]
    fn calculate_hash(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        hex::encode(result)
    }

    #[tracing::instrument(skip(self))]
    pub async fn verify_blocks(&self, blocks: &[String]) -> Result<Vec<String>> {
        let mut corrupted = Vec::new();

        for hash in blocks {
            let block_path = PathBuf::from("blocks")
                .join(&hash[..2])
                .join(&hash[2..4])
                .join(hash);

            match self.storage.download(&block_path, Default::default()).await {
                Ok(data) => {
                    let mut reader = tokio::io::BufReader::new(data);
                    let mut compressed_data = Vec::new();
                    if let Err(e) = reader.read_to_end(&mut compressed_data).await {
                        warn!("Failed to read block {}: {}", hash, e);
                        corrupted.push(hash.clone());
                        continue;
                    }

                    // Try to decompress to verify integrity
                    if let Err(e) = zstd::decode_all(&compressed_data[..]) {
                        warn!("Failed to decompress block {}: {}", hash, e);
                        corrupted.push(hash.clone());
                    }
                }
                Err(e) => {
                    warn!("Failed to access block {}: {}", hash, e);
                    corrupted.push(hash.clone());
                }
            }
        }

        Ok(corrupted)
    }
}
