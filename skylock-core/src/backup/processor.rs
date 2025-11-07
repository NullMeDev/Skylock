use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;
use futures::StreamExt;
use crate::Result;
use super::dedup::{Block, DedupEngine};
use super::storage::BackupStorage;
use super::compression::{CompressionEngine, CompressionConfig, CompressionStats};
use walkdir::WalkDir;
use tokio::fs;

pub struct BlockProcessor {
    dedup_engine: Arc<DedupEngine>,
    storage: Arc<BackupStorage>,
    compression_engine: Arc<CompressionEngine>,
    max_concurrent: usize,
    semaphore: Arc<Semaphore>,
}

#[derive(Debug)]
pub struct ProcessingStats {
    pub files_processed: usize,
    pub blocks_processed: usize,
    pub bytes_processed: u64,
    pub unique_blocks: usize,
    pub compression_stats: CompressionStats,
}

impl BlockProcessor {
    pub fn new(dedup_engine: DedupEngine, storage: BackupStorage, compression_config: CompressionConfig, max_concurrent: usize) -> Self {
        Self {
            dedup_engine: Arc::new(dedup_engine),
            storage: Arc::new(storage),
            compression_engine: Arc::new(CompressionEngine::new(compression_config)),
            max_concurrent,
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    pub async fn process_directory(&self, source: &Path) -> Result<ProcessingStats> {
        let (tx, mut rx) = mpsc::channel(self.max_concurrent);
        let mut join_set = JoinSet::new();
        let mut stats = ProcessingStats {
            files_processed: 0,
            blocks_processed: 0,
            bytes_processed: 0,
            unique_blocks: 0,
            compression_stats: CompressionStats::default(),
        };

        // Spawn file processor tasks
        let entries: Vec<_> = WalkDir::new(source)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .collect();

        for entry in entries {
            let path = entry.path().to_owned();
            let tx = tx.clone();
            let dedup_engine = self.dedup_engine.clone();
            let storage = self.storage.clone();
            let compression_engine = self.compression_engine.clone();
            let semaphore = self.semaphore.clone();

            join_set.spawn(async move {
                let _permit = semaphore.acquire().await?;
                let file_size = fs::metadata(&path).await?.len();
                let mut blocks = dedup_engine.process_file(&path).await?;

                // Compress blocks if needed
                let mut compression_stats = CompressionStats::default();
                if compression_engine.should_compress(&path, file_size) {
                    for block in blocks.iter_mut() {
                        let original_size = block.data.len() as u64;
                        block.data = compression_engine.compress(&block.data)?;
                        compression_stats.total_bytes += original_size;
                        compression_stats.compressed_bytes += block.data.len() as u64;
                        compression_stats.files_compressed += 1;
                    }
                } else {
                    compression_stats.files_skipped += 1;
                }

                // Store blocks concurrently
                let mut block_join_set = JoinSet::new();
                for block in blocks {
                    let storage = storage.clone();
                    block_join_set.spawn(async move {
                        storage.store_block(&block).await
                    });
                }

                // Wait for all block storage operations to complete
                while let Some(result) = block_join_set.join_next().await {
                    result??;
                }

                tx.send((path, file_size, blocks.len(), compression_stats)).await?;
                Ok::<_, crate::error::SkylockError>(())
            });
        }

        // Drop original sender so channel closes when all tasks complete
        drop(tx);

        // Process results as they come in
        while let Some((path, size, block_count, comp_stats)) = rx.recv().await {
            stats.files_processed += 1;
            stats.bytes_processed += size;
            stats.blocks_processed += block_count;
            stats.compression_stats.total_bytes += comp_stats.total_bytes;
            stats.compression_stats.compressed_bytes += comp_stats.compressed_bytes;
            stats.compression_stats.files_compressed += comp_stats.files_compressed;
            stats.compression_stats.files_skipped += comp_stats.files_skipped;
        }

        // Wait for all tasks to complete and check for errors
        while let Some(result) = join_set.join_next().await {
            result??;
        }

        // Get unique block count
        stats.unique_blocks = self.storage.get_unique_block_count().await?;

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs::File;
    use std::io::Write;

    #[tokio::test]
    async fn test_concurrent_processing() -> Result<()> {
        // Create test directory structure
        let source_dir = tempdir()?;
        let backup_dir = tempdir()?;

        // Create some test files
        for i in 0..10 {
            let file_path = source_dir.path().join(format!("test{}.txt", i));
            let mut file = File::create(file_path)?;
            writeln!(file, "Test content {}", i)?;
        }

        // Initialize processor
        let dedup_engine = DedupEngine::new(4096)?;
        let storage = BackupStorage::new(&backup_dir.path().to_path_buf())?;
        let processor = BlockProcessor::new(dedup_engine, storage, 4);

        // Process directory
        let stats = processor.process_directory(source_dir.path()).await?;

        assert_eq!(stats.files_processed, 10);
        assert!(stats.blocks_processed >= 10);
        assert!(stats.bytes_processed > 0);
        assert!(stats.unique_blocks > 0);

        Ok(())
    }
}
