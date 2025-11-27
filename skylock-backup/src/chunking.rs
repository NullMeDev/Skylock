//! Adaptive Chunking Strategy
//!
//! Dynamically selects optimal chunk sizes from 256KB-16MB based on:
//! - File size (larger files get larger chunks)
//! - Network throughput (adjust based on measured performance)
//! - Memory pressure (smaller chunks when memory is tight)
//! - File type heuristics (compressible vs incompressible)
//!
//! Optimizes for both throughput and memory efficiency.

use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Minimum chunk size (256KB)
pub const MIN_CHUNK_SIZE: usize = 256 * 1024;

/// Maximum chunk size (16MB)
pub const MAX_CHUNK_SIZE: usize = 16 * 1024 * 1024;

/// Default chunk size (1MB)
pub const DEFAULT_CHUNK_SIZE: usize = 1024 * 1024;

/// Threshold for "small" files (use single chunk)
const SMALL_FILE_THRESHOLD: u64 = 256 * 1024; // 256KB

/// Threshold for "medium" files
const MEDIUM_FILE_THRESHOLD: u64 = 10 * 1024 * 1024; // 10MB

/// Threshold for "large" files
const LARGE_FILE_THRESHOLD: u64 = 100 * 1024 * 1024; // 100MB

/// Threshold for "huge" files
const HUGE_FILE_THRESHOLD: u64 = 1024 * 1024 * 1024; // 1GB

/// Target number of chunks for optimal parallelism
const TARGET_CHUNK_COUNT: usize = 16;

/// File type categories for chunking decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTypeCategory {
    /// Text files, source code - highly compressible
    Text,
    /// Binary executables, archives - mixed compressibility
    Binary,
    /// Already compressed files (zip, gz, mp4) - incompressible
    Compressed,
    /// Unknown or other file types
    Unknown,
}

impl FileTypeCategory {
    /// Determine file category from extension
    pub fn from_extension(ext: &str) -> Self {
        let ext_lower = ext.to_lowercase();
        match ext_lower.as_str() {
            // Text/source files
            "txt" | "md" | "rs" | "py" | "js" | "ts" | "java" | "go" | "c" | "cpp" | "h"
            | "hpp" | "json" | "yaml" | "yml" | "toml" | "xml" | "html" | "css" | "sql"
            | "sh" | "bash" | "zsh" | "log" | "csv" => Self::Text,

            // Binary files
            "exe" | "dll" | "so" | "dylib" | "bin" | "dat" | "db" | "sqlite" => Self::Binary,

            // Already compressed
            "zip" | "gz" | "tar" | "xz" | "bz2" | "7z" | "rar" | "zst" | "lz4" | "mp3"
            | "mp4" | "mkv" | "avi" | "mov" | "webm" | "jpg" | "jpeg" | "png" | "gif"
            | "webp" | "pdf" | "docx" | "xlsx" | "pptx" => Self::Compressed,

            _ => Self::Unknown,
        }
    }

    /// Determine file category from path
    pub fn from_path(path: &Path) -> Self {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(Self::from_extension)
            .unwrap_or(Self::Unknown)
    }

    /// Get compression ratio estimate for this file type
    pub fn estimated_compression_ratio(&self) -> f64 {
        match self {
            Self::Text => 0.3,       // Text compresses to ~30% of original
            Self::Binary => 0.6,    // Binary compresses to ~60%
            Self::Compressed => 1.0, // Already compressed, no gain
            Self::Unknown => 0.7,    // Assume moderate compression
        }
    }
}

/// Chunk size selection strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkStrategy {
    /// Fixed chunk size
    Fixed(usize),
    /// Adaptive based on file size
    AdaptiveByFileSize,
    /// Adaptive based on network throughput
    AdaptiveByThroughput,
    /// Fully adaptive (file size + throughput + memory)
    FullyAdaptive,
}

impl Default for ChunkStrategy {
    fn default() -> Self {
        Self::FullyAdaptive
    }
}

/// Configuration for the chunking controller
#[derive(Debug, Clone)]
pub struct ChunkingConfig {
    /// Minimum chunk size
    pub min_chunk_size: usize,
    /// Maximum chunk size
    pub max_chunk_size: usize,
    /// Default/initial chunk size
    pub default_chunk_size: usize,
    /// Chunking strategy
    pub strategy: ChunkStrategy,
    /// Memory pressure threshold (fraction of available memory to use)
    pub max_memory_fraction: f64,
    /// Target throughput for adaptive adjustment (bytes/sec)
    pub target_throughput: Option<u64>,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            min_chunk_size: MIN_CHUNK_SIZE,
            max_chunk_size: MAX_CHUNK_SIZE,
            default_chunk_size: DEFAULT_CHUNK_SIZE,
            strategy: ChunkStrategy::FullyAdaptive,
            max_memory_fraction: 0.25, // Use up to 25% of available memory for chunks
            target_throughput: None,
        }
    }
}

impl ChunkingConfig {
    /// Create config optimized for small files
    pub fn for_small_files() -> Self {
        Self {
            min_chunk_size: 64 * 1024,  // 64KB
            max_chunk_size: 1024 * 1024, // 1MB
            default_chunk_size: 256 * 1024, // 256KB
            strategy: ChunkStrategy::Fixed(256 * 1024),
            ..Default::default()
        }
    }

    /// Create config optimized for large files
    pub fn for_large_files() -> Self {
        Self {
            min_chunk_size: 1024 * 1024,      // 1MB
            max_chunk_size: 32 * 1024 * 1024, // 32MB
            default_chunk_size: 8 * 1024 * 1024, // 8MB
            strategy: ChunkStrategy::FullyAdaptive,
            ..Default::default()
        }
    }

    /// Create config optimized for streaming
    pub fn for_streaming() -> Self {
        Self {
            min_chunk_size: 512 * 1024,      // 512KB
            max_chunk_size: 4 * 1024 * 1024, // 4MB
            default_chunk_size: 2 * 1024 * 1024, // 2MB
            strategy: ChunkStrategy::AdaptiveByThroughput,
            ..Default::default()
        }
    }

    /// Set target throughput for adaptive adjustment
    pub fn with_target_throughput(mut self, bytes_per_sec: u64) -> Self {
        self.target_throughput = Some(bytes_per_sec);
        self
    }
}

/// Performance metrics for chunk size optimization
#[derive(Debug, Default)]
struct ChunkMetrics {
    /// Total bytes processed
    bytes_processed: AtomicU64,
    /// Total time spent processing (milliseconds)
    processing_time_ms: AtomicU64,
    /// Number of chunks processed
    chunks_processed: AtomicUsize,
    /// Current chunk size being used
    current_chunk_size: AtomicUsize,
}

impl ChunkMetrics {
    fn new(initial_chunk_size: usize) -> Self {
        Self {
            bytes_processed: AtomicU64::new(0),
            processing_time_ms: AtomicU64::new(0),
            chunks_processed: AtomicUsize::new(0),
            current_chunk_size: AtomicUsize::new(initial_chunk_size),
        }
    }

    fn record_chunk(&self, bytes: u64, time_ms: u64) {
        self.bytes_processed.fetch_add(bytes, Ordering::Relaxed);
        self.processing_time_ms.fetch_add(time_ms, Ordering::Relaxed);
        self.chunks_processed.fetch_add(1, Ordering::Relaxed);
    }

    fn throughput_bytes_per_sec(&self) -> f64 {
        let time_ms = self.processing_time_ms.load(Ordering::Relaxed);
        if time_ms == 0 {
            return 0.0;
        }
        let bytes = self.bytes_processed.load(Ordering::Relaxed);
        (bytes as f64 / time_ms as f64) * 1000.0
    }

    fn average_chunk_time_ms(&self) -> f64 {
        let chunks = self.chunks_processed.load(Ordering::Relaxed);
        if chunks == 0 {
            return 0.0;
        }
        self.processing_time_ms.load(Ordering::Relaxed) as f64 / chunks as f64
    }

    fn reset(&self) {
        self.bytes_processed.store(0, Ordering::Relaxed);
        self.processing_time_ms.store(0, Ordering::Relaxed);
        self.chunks_processed.store(0, Ordering::Relaxed);
    }
}

/// Adaptive chunking controller
///
/// Dynamically determines optimal chunk sizes based on file characteristics
/// and runtime performance metrics.
pub struct ChunkingController {
    /// Configuration
    config: ChunkingConfig,
    /// Performance metrics
    metrics: Arc<ChunkMetrics>,
    /// Last adjustment time
    last_adjustment: RwLock<Instant>,
    /// Available memory (updated periodically)
    available_memory: AtomicU64,
}

impl ChunkingController {
    /// Create a new chunking controller with default config
    pub fn new() -> Self {
        Self::with_config(ChunkingConfig::default())
    }

    /// Create a new chunking controller with custom config
    pub fn with_config(config: ChunkingConfig) -> Self {
        let available_memory = detect_available_memory();

        Self {
            metrics: Arc::new(ChunkMetrics::new(config.default_chunk_size)),
            config,
            last_adjustment: RwLock::new(Instant::now()),
            available_memory: AtomicU64::new(available_memory),
        }
    }

    /// Get optimal chunk size for a file
    pub fn chunk_size_for_file(&self, file_size: u64, path: Option<&Path>) -> usize {
        match self.config.strategy {
            ChunkStrategy::Fixed(size) => size,
            ChunkStrategy::AdaptiveByFileSize => self.chunk_size_by_file_size(file_size),
            ChunkStrategy::AdaptiveByThroughput => self.chunk_size_by_throughput(),
            ChunkStrategy::FullyAdaptive => {
                self.fully_adaptive_chunk_size(file_size, path)
            }
        }
    }

    /// Calculate chunk size based on file size alone
    fn chunk_size_by_file_size(&self, file_size: u64) -> usize {
        if file_size <= SMALL_FILE_THRESHOLD {
            // Small files: use entire file as single chunk
            (file_size as usize).max(self.config.min_chunk_size)
        } else if file_size <= MEDIUM_FILE_THRESHOLD {
            // Medium files: 512KB-1MB chunks
            self.config.min_chunk_size.max(512 * 1024)
        } else if file_size <= LARGE_FILE_THRESHOLD {
            // Large files: 2MB-4MB chunks
            2 * 1024 * 1024
        } else if file_size <= HUGE_FILE_THRESHOLD {
            // Huge files: 8MB-16MB chunks
            8 * 1024 * 1024
        } else {
            // Giant files: max chunk size
            self.config.max_chunk_size
        }
        .min(self.config.max_chunk_size)
        .max(self.config.min_chunk_size)
    }

    /// Calculate chunk size based on observed throughput
    fn chunk_size_by_throughput(&self) -> usize {
        let throughput = self.metrics.throughput_bytes_per_sec();
        let avg_chunk_time = self.metrics.average_chunk_time_ms();
        let current = self.metrics.current_chunk_size.load(Ordering::Relaxed);

        // If no metrics yet, use default
        if throughput == 0.0 || avg_chunk_time == 0.0 {
            return self.config.default_chunk_size;
        }

        // Target chunk processing time: 200-500ms
        // This balances between overhead of small chunks and memory use of large chunks
        let target_time_ms = 350.0;

        let ratio = target_time_ms / avg_chunk_time;
        let adjusted = (current as f64 * ratio) as usize;

        adjusted
            .max(self.config.min_chunk_size)
            .min(self.config.max_chunk_size)
    }

    /// Fully adaptive chunk size considering all factors
    fn fully_adaptive_chunk_size(&self, file_size: u64, path: Option<&Path>) -> usize {
        // Start with file size based calculation
        let base_size = self.chunk_size_by_file_size(file_size);

        // Adjust for file type
        let file_type = path
            .map(FileTypeCategory::from_path)
            .unwrap_or(FileTypeCategory::Unknown);

        let type_adjusted = match file_type {
            FileTypeCategory::Text => {
                // Text files compress well, can use larger chunks
                (base_size as f64 * 1.5) as usize
            }
            FileTypeCategory::Compressed => {
                // Already compressed, smaller chunks reduce memory pressure
                (base_size as f64 * 0.75) as usize
            }
            _ => base_size,
        };

        // Adjust for memory pressure
        let available = self.available_memory.load(Ordering::Relaxed);
        let max_chunk_by_memory = (available as f64 * self.config.max_memory_fraction) as usize;
        let memory_adjusted = type_adjusted.min(max_chunk_by_memory);

        // Adjust for target chunk count (aim for reasonable parallelism)
        let target_by_parallelism = if file_size > 0 {
            (file_size as usize / TARGET_CHUNK_COUNT).max(self.config.min_chunk_size)
        } else {
            self.config.default_chunk_size
        };

        // Take minimum of adjustments to be conservative with memory
        let final_size = memory_adjusted
            .min(target_by_parallelism)
            .max(self.config.min_chunk_size)
            .min(self.config.max_chunk_size);

        debug!(
            "Chunk size for {}KB file: base={}KB, type_adj={}KB, mem_adj={}KB, final={}KB",
            file_size / 1024,
            base_size / 1024,
            type_adjusted / 1024,
            memory_adjusted / 1024,
            final_size / 1024
        );

        final_size
    }

    /// Record chunk processing metrics
    pub fn record_chunk(&self, bytes: u64, duration: Duration) {
        self.metrics
            .record_chunk(bytes, duration.as_millis() as u64);
    }

    /// Get current throughput in bytes per second
    pub fn current_throughput(&self) -> f64 {
        self.metrics.throughput_bytes_per_sec()
    }

    /// Get average chunk processing time
    pub fn average_chunk_time(&self) -> Duration {
        Duration::from_millis(self.metrics.average_chunk_time_ms() as u64)
    }

    /// Update available memory estimate
    pub fn update_available_memory(&self) {
        let available = detect_available_memory();
        self.available_memory.store(available, Ordering::Relaxed);
    }

    /// Reset metrics (e.g., at start of new backup)
    pub fn reset_metrics(&self) {
        self.metrics.reset();
    }

    /// Get the current chunk size setting
    pub fn current_chunk_size(&self) -> usize {
        self.metrics.current_chunk_size.load(Ordering::Relaxed)
    }

    /// Get configuration
    pub fn config(&self) -> &ChunkingConfig {
        &self.config
    }
}

impl Default for ChunkingController {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect available memory
#[cfg(target_os = "linux")]
fn detect_available_memory() -> u64 {
    use std::fs;

    let content = match fs::read_to_string("/proc/meminfo") {
        Ok(c) => c,
        Err(_) => return 4 * 1024 * 1024 * 1024, // Default 4GB
    };

    for line in content.lines() {
        if line.starts_with("MemAvailable:") {
            if let Some(kb_str) = line.split_whitespace().nth(1) {
                if let Ok(kb) = kb_str.parse::<u64>() {
                    return kb * 1024;
                }
            }
        }
    }

    4 * 1024 * 1024 * 1024 // Default 4GB
}

#[cfg(not(target_os = "linux"))]
fn detect_available_memory() -> u64 {
    // On non-Linux platforms, assume 4GB available
    4 * 1024 * 1024 * 1024
}

/// A chunk of file data for processing
#[derive(Debug)]
pub struct FileChunk {
    /// Chunk index (0-based)
    pub index: usize,
    /// Offset in original file
    pub offset: u64,
    /// Chunk data
    pub data: Vec<u8>,
    /// Whether this is the last chunk
    pub is_last: bool,
}

/// Iterator that yields file chunks
pub struct ChunkIterator {
    /// File data reader
    data: Vec<u8>,
    /// Current position
    position: usize,
    /// Chunk size to use
    chunk_size: usize,
    /// Current chunk index
    index: usize,
}

impl ChunkIterator {
    /// Create a new chunk iterator
    pub fn new(data: Vec<u8>, chunk_size: usize) -> Self {
        Self {
            data,
            position: 0,
            chunk_size,
            index: 0,
        }
    }

    /// Get total number of chunks
    pub fn total_chunks(&self) -> usize {
        (self.data.len() + self.chunk_size - 1) / self.chunk_size
    }
}

impl Iterator for ChunkIterator {
    type Item = FileChunk;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.data.len() {
            return None;
        }

        let start = self.position;
        let end = (self.position + self.chunk_size).min(self.data.len());
        let chunk_data = self.data[start..end].to_vec();
        let is_last = end >= self.data.len();

        let chunk = FileChunk {
            index: self.index,
            offset: start as u64,
            data: chunk_data,
            is_last,
        };

        self.position = end;
        self.index += 1;

        Some(chunk)
    }
}

/// Async streaming chunk reader for large files
pub struct StreamingChunkReader {
    chunk_size: usize,
}

impl StreamingChunkReader {
    pub fn new(chunk_size: usize) -> Self {
        Self { chunk_size }
    }

    /// Read file in chunks without loading entire file into memory
    pub async fn read_file_chunks(
        &self,
        path: &Path,
    ) -> std::io::Result<impl Iterator<Item = std::io::Result<Vec<u8>>>> {
        use std::fs::File;
        use std::io::{BufReader, Read};

        let file = File::open(path)?;
        let chunk_size = self.chunk_size;

        Ok(ChunkReadIterator {
            reader: BufReader::new(file),
            chunk_size,
            done: false,
        })
    }
}

struct ChunkReadIterator<R: std::io::Read> {
    reader: std::io::BufReader<R>,
    chunk_size: usize,
    done: bool,
}

impl<R: std::io::Read> Iterator for ChunkReadIterator<R> {
    type Item = std::io::Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        use std::io::Read;

        let mut buffer = vec![0u8; self.chunk_size];
        match self.reader.read(&mut buffer) {
            Ok(0) => {
                self.done = true;
                None
            }
            Ok(n) => {
                buffer.truncate(n);
                if n < self.chunk_size {
                    self.done = true;
                }
                Some(Ok(buffer))
            }
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_type_detection() {
        assert_eq!(
            FileTypeCategory::from_extension("rs"),
            FileTypeCategory::Text
        );
        assert_eq!(
            FileTypeCategory::from_extension("mp4"),
            FileTypeCategory::Compressed
        );
        assert_eq!(
            FileTypeCategory::from_extension("exe"),
            FileTypeCategory::Binary
        );
        assert_eq!(
            FileTypeCategory::from_extension("xyz"),
            FileTypeCategory::Unknown
        );
    }

    #[test]
    fn test_chunk_size_by_file_size() {
        let controller = ChunkingController::new();

        // Small file
        let small = controller.chunk_size_by_file_size(100 * 1024); // 100KB
        assert!(small >= MIN_CHUNK_SIZE);
        assert!(small <= MAX_CHUNK_SIZE);

        // Medium file
        let medium = controller.chunk_size_by_file_size(5 * 1024 * 1024); // 5MB
        assert!(medium >= MIN_CHUNK_SIZE);
        assert!(medium >= 512 * 1024); // At least 512KB

        // Large file
        let large = controller.chunk_size_by_file_size(50 * 1024 * 1024); // 50MB
        assert!(large >= 2 * 1024 * 1024); // At least 2MB

        // Huge file
        let huge = controller.chunk_size_by_file_size(500 * 1024 * 1024); // 500MB
        assert!(huge >= 8 * 1024 * 1024); // At least 8MB
    }

    #[test]
    fn test_fully_adaptive_chunk_size() {
        let controller = ChunkingController::new();

        // Test with text file (should be larger)
        let text_path = Path::new("/test/file.txt");
        let text_size = controller.fully_adaptive_chunk_size(10 * 1024 * 1024, Some(text_path));

        // Test with compressed file (should be smaller)
        let zip_path = Path::new("/test/file.zip");
        let zip_size = controller.fully_adaptive_chunk_size(10 * 1024 * 1024, Some(zip_path));

        // Text should get equal or larger chunks than compressed
        assert!(text_size >= zip_size);
    }

    #[test]
    fn test_chunk_iterator() {
        let data = vec![0u8; 10000]; // 10KB
        let iterator = ChunkIterator::new(data, 3000); // 3KB chunks

        assert_eq!(iterator.total_chunks(), 4); // ceil(10000/3000)

        let chunks: Vec<_> = iterator.collect();
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].index, 0);
        assert_eq!(chunks[0].offset, 0);
        assert_eq!(chunks[0].data.len(), 3000);
        assert!(!chunks[0].is_last);

        assert_eq!(chunks[3].index, 3);
        assert_eq!(chunks[3].offset, 9000);
        assert_eq!(chunks[3].data.len(), 1000);
        assert!(chunks[3].is_last);
    }

    #[test]
    fn test_metrics_recording() {
        let controller = ChunkingController::new();

        // Record some chunks
        controller.record_chunk(1024 * 1024, Duration::from_millis(100));
        controller.record_chunk(1024 * 1024, Duration::from_millis(100));
        controller.record_chunk(1024 * 1024, Duration::from_millis(100));

        // Check throughput (3MB in 300ms = 10MB/s)
        let throughput = controller.current_throughput();
        assert!(throughput > 9.0 * 1024.0 * 1024.0);
        assert!(throughput < 11.0 * 1024.0 * 1024.0);

        // Check average time
        let avg_time = controller.average_chunk_time();
        assert_eq!(avg_time, Duration::from_millis(100));

        // Reset and verify
        controller.reset_metrics();
        assert_eq!(controller.current_throughput(), 0.0);
    }

    #[test]
    fn test_config_presets() {
        let small_config = ChunkingConfig::for_small_files();
        assert!(small_config.max_chunk_size <= 1024 * 1024);

        let large_config = ChunkingConfig::for_large_files();
        assert!(large_config.min_chunk_size >= 1024 * 1024);

        let streaming_config = ChunkingConfig::for_streaming();
        assert!(streaming_config.default_chunk_size == 2 * 1024 * 1024);
    }

    #[test]
    fn test_fixed_strategy() {
        let config = ChunkingConfig {
            strategy: ChunkStrategy::Fixed(512 * 1024),
            ..Default::default()
        };
        let controller = ChunkingController::with_config(config);

        // Should always return fixed size regardless of file size
        assert_eq!(
            controller.chunk_size_for_file(100, None),
            512 * 1024
        );
        assert_eq!(
            controller.chunk_size_for_file(1024 * 1024 * 1024, None),
            512 * 1024
        );
    }
}
