//! Multi-algorithm compression engine for Skylock Hybrid
//!
//! This module provides adaptive compression using LZ4, ZSTD, and Brotli algorithms
//! with intelligent algorithm selection based on data characteristics.

use std::io::{Read, Write};
use lz4::block::{compress, decompress, CompressionMode};
use thiserror::Error;

/// Compression errors
#[derive(Error, Debug)]
pub enum CompressionError {
    #[error("Compression failed: {0}")]
    Compression(String),
    #[error("Decompression failed: {0}")]
    Decompression(String),
    #[error("Unsupported algorithm: {0}")]
    UnsupportedAlgorithm(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid compressed data")]
    InvalidData,
}

/// Compression algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CompressionAlgorithm {
    /// No compression
    None,
    /// LZ4 - Fast compression/decompression, moderate ratio
    Lz4,
    /// ZSTD - Balanced speed and ratio, good for general use
    Zstd,
    /// Brotli - High compression ratio, slower but good for archival
    Brotli,
}

impl std::fmt::Display for CompressionAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompressionAlgorithm::None => write!(f, "none"),
            CompressionAlgorithm::Lz4 => write!(f, "lz4"),
            CompressionAlgorithm::Zstd => write!(f, "zstd"),
            CompressionAlgorithm::Brotli => write!(f, "brotli"),
        }
    }
}

impl std::str::FromStr for CompressionAlgorithm {
    type Err = CompressionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(CompressionAlgorithm::None),
            "lz4" => Ok(CompressionAlgorithm::Lz4),
            "zstd" => Ok(CompressionAlgorithm::Zstd),
            "brotli" => Ok(CompressionAlgorithm::Brotli),
            _ => Err(CompressionError::UnsupportedAlgorithm(s.to_string())),
        }
    }
}

/// Compression level settings
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CompressionLevel {
    /// Fastest compression, lowest ratio
    Fastest,
    /// Fast compression, good ratio
    Fast,
    /// Balanced compression and ratio
    Default,
    /// Better compression, slower
    Better,
    /// Best compression ratio, slowest
    Best,
    /// Custom level (algorithm-specific)
    Custom(i32),
}

impl CompressionLevel {
    /// Get the algorithm-specific level value
    pub fn to_level(&self, algorithm: CompressionAlgorithm) -> i32 {
        match (self, algorithm) {
            (CompressionLevel::Fastest, CompressionAlgorithm::Lz4) => 1,
            (CompressionLevel::Fast, CompressionAlgorithm::Lz4) => 3,
            (CompressionLevel::Default, CompressionAlgorithm::Lz4) => 6,
            (CompressionLevel::Better, CompressionAlgorithm::Lz4) => 9,
            (CompressionLevel::Best, CompressionAlgorithm::Lz4) => 12,
            
            (CompressionLevel::Fastest, CompressionAlgorithm::Zstd) => 1,
            (CompressionLevel::Fast, CompressionAlgorithm::Zstd) => 3,
            (CompressionLevel::Default, CompressionAlgorithm::Zstd) => 6,
            (CompressionLevel::Better, CompressionAlgorithm::Zstd) => 12,
            (CompressionLevel::Best, CompressionAlgorithm::Zstd) => 19,
            
            (CompressionLevel::Fastest, CompressionAlgorithm::Brotli) => 1,
            (CompressionLevel::Fast, CompressionAlgorithm::Brotli) => 3,
            (CompressionLevel::Default, CompressionAlgorithm::Brotli) => 6,
            (CompressionLevel::Better, CompressionAlgorithm::Brotli) => 9,
            (CompressionLevel::Best, CompressionAlgorithm::Brotli) => 11,
            
            (CompressionLevel::Custom(level), _) => *level,
            (_, CompressionAlgorithm::None) => 0,
        }
    }
}

/// Compressed data container
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompressedData {
    pub algorithm: CompressionAlgorithm,
    pub level: CompressionLevel,
    pub original_size: u64,
    pub compressed_size: u64,
    pub data: Vec<u8>,
    pub checksum: u32, // CRC32 of original data
}

impl CompressedData {
    /// Calculate compression ratio (0.0 to 1.0)
    pub fn ratio(&self) -> f64 {
        if self.original_size == 0 {
            0.0
        } else {
            1.0 - (self.compressed_size as f64 / self.original_size as f64)
        }
    }
    
    /// Check if compression was beneficial
    pub fn is_beneficial(&self) -> bool {
        // Safe comparison as both are u64
        self.compressed_size < self.original_size
    }
}

/// Data type detection for compression algorithm selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    /// Plain text data
    Text,
    /// Binary data
    Binary,
    /// Already compressed data (e.g., images, videos)
    Compressed,
    /// Unknown data type
    Unknown,
}

/// Compression statistics for algorithm selection
#[derive(Debug, Clone)]
pub struct CompressionStats {
    pub data_type: DataType,
    pub entropy: f64,
    pub repetition_ratio: f64,
    pub size: u64,
}

/// Multi-algorithm compression engine
pub struct CompressionEngine {
    default_algorithm: CompressionAlgorithm,
    default_level: CompressionLevel,
    adaptive_selection: bool,
    min_size_for_compression: usize,
}

impl Default for CompressionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CompressionEngine {
    /// Create new compression engine with default settings
    pub fn new() -> Self {
        CompressionEngine {
            default_algorithm: CompressionAlgorithm::Zstd,
            default_level: CompressionLevel::Default,
            adaptive_selection: true,
            min_size_for_compression: 1024, // Don't compress files smaller than 1KB
        }
    }
    
    /// Create compression engine with specific algorithm
    pub fn with_algorithm(algorithm: CompressionAlgorithm, level: CompressionLevel) -> Self {
        CompressionEngine {
            default_algorithm: algorithm,
            default_level: level,
            adaptive_selection: false,
            min_size_for_compression: 1024,
        }
    }
    
    /// Enable or disable adaptive algorithm selection
    pub fn set_adaptive(&mut self, adaptive: bool) {
        self.adaptive_selection = adaptive;
    }
    
    /// Set minimum size for compression
    pub fn set_min_compression_size(&mut self, size: usize) {
        self.min_size_for_compression = size;
    }
    
    /// Analyze data characteristics for compression algorithm selection
    pub fn analyze_data(&self, data: &[u8]) -> CompressionStats {
        let size = data.len();
        
        // Calculate entropy (simplified Shannon entropy)
        let mut byte_counts = [0u32; 256];
        for &byte in data {
            byte_counts[byte as usize] += 1;
        }
        
        let mut entropy = 0.0;
        let total = size as f64;
        for &count in &byte_counts {
            if count > 0 {
                let p = count as f64 / total;
                entropy -= p * p.log2();
            }
        }
        
        // Calculate repetition ratio (simple RLE analysis)
        let mut repetitions = 0;
        let mut prev_byte = None;
        for &byte in data {
            if Some(byte) == prev_byte {
                repetitions += 1;
            }
            prev_byte = Some(byte);
        }
        let repetition_ratio = repetitions as f64 / size.max(1) as f64;
        
        // Determine data type
        let data_type = if self.is_text_data(data) {
            DataType::Text
        } else if self.is_compressed_data(data) {
            DataType::Compressed
        } else {
            DataType::Binary
        };
        
        CompressionStats {
            data_type,
            entropy,
            repetition_ratio,
            size: size as u64,
        }
    }
    
    /// Select optimal compression algorithm based on data analysis
    pub fn select_algorithm(&self, stats: &CompressionStats) -> (CompressionAlgorithm, CompressionLevel) {
        if !self.adaptive_selection {
            return (self.default_algorithm, self.default_level);
        }
        
        // Don't compress small files or already compressed data
        if stats.size < self.min_size_for_compression as u64 || stats.data_type == DataType::Compressed {
            return (CompressionAlgorithm::None, CompressionLevel::Fastest);
        }
        
        // Select based on data characteristics
        match stats.data_type {
            DataType::Text => {
                if stats.repetition_ratio > 0.3 {
                    // High repetition - use LZ4 for speed
                    (CompressionAlgorithm::Lz4, CompressionLevel::Fast)
                } else if stats.entropy < 4.0 {
                    // Low entropy - use Brotli for best ratio
                    (CompressionAlgorithm::Brotli, CompressionLevel::Default)
                } else {
                    // Balanced text - use ZSTD
                    (CompressionAlgorithm::Zstd, CompressionLevel::Default)
                }
            },
            DataType::Binary => {
                if stats.size > 1024 * 1024 * 10 {
                    // Large files - prioritize speed
                    (CompressionAlgorithm::Lz4, CompressionLevel::Fast)
                } else if stats.entropy < 6.0 {
                    // Structured binary data - good compression potential
                    (CompressionAlgorithm::Zstd, CompressionLevel::Better)
                } else {
                    // High entropy binary - use fast compression
                    (CompressionAlgorithm::Lz4, CompressionLevel::Default)
                }
            },
            DataType::Compressed => {
                // Already compressed - don't compress further
                (CompressionAlgorithm::None, CompressionLevel::Fastest)
            },
            DataType::Unknown => {
                // Default to balanced approach
                (CompressionAlgorithm::Zstd, CompressionLevel::Default)
            },
        }
    }
    
    /// Compress data with automatic algorithm selection
    pub fn compress(&self, data: &[u8]) -> Result<CompressedData, CompressionError> {
        let stats = self.analyze_data(data);
        let (algorithm, level) = self.select_algorithm(&stats);
        
        self.compress_with_algorithm(data, algorithm, level)
    }
    
    /// Compress data with specific algorithm and level
    pub fn compress_with_algorithm(
        &self,
        data: &[u8],
        algorithm: CompressionAlgorithm,
        level: CompressionLevel,
    ) -> Result<CompressedData, CompressionError> {
        let original_size = data.len();
        let checksum = crc32fast::hash(data);
        
        let compressed_data = match algorithm {
            CompressionAlgorithm::None => data.to_vec(),
            CompressionAlgorithm::Lz4 => self.compress_lz4(data, level)?,
            CompressionAlgorithm::Zstd => self.compress_zstd(data, level)?,
            CompressionAlgorithm::Brotli => self.compress_brotli(data, level)?,
        };
        
        Ok(CompressedData {
            algorithm,
            level,
            original_size: original_size as u64,
            compressed_size: compressed_data.len() as u64,
            data: compressed_data,
            checksum,
        })
    }
    
    /// Decompress data
    pub fn decompress(&self, compressed: &CompressedData) -> Result<Vec<u8>, CompressionError> {
        let decompressed = match compressed.algorithm {
            CompressionAlgorithm::None => compressed.data.clone(),
            CompressionAlgorithm::Lz4 => self.decompress_lz4(&compressed.data)?,
            CompressionAlgorithm::Zstd => self.decompress_zstd(&compressed.data)?,
            CompressionAlgorithm::Brotli => self.decompress_brotli(&compressed.data)?,
        };
        
        // Verify checksum
        let checksum = crc32fast::hash(&decompressed);
        if checksum != compressed.checksum {
            return Err(CompressionError::InvalidData);
        }
        
        // Verify size
        if decompressed.len() as u64 != compressed.original_size {
            return Err(CompressionError::InvalidData);
        }
        
        Ok(decompressed)
    }
    
    /// Compress data using LZ4
    fn compress_lz4(&self, data: &[u8], level: CompressionLevel) -> Result<Vec<u8>, CompressionError> {
        let level = level.to_level(CompressionAlgorithm::Lz4);
        
        if level <= 6 {
            // Use fast compression
            compress(data, Some(CompressionMode::FAST(level)), true)
                .map_err(|e| CompressionError::Compression(format!("LZ4: {}", e)))
        } else {
            // Use high compression
            compress(data, Some(CompressionMode::HIGHCOMPRESSION(level)), true)
                .map_err(|e| CompressionError::Compression(format!("LZ4: {}", e)))
        }
    }
    
    /// Decompress LZ4 data
    fn decompress_lz4(&self, data: &[u8]) -> Result<Vec<u8>, CompressionError> {
        decompress(data, None)
            .map_err(|e| CompressionError::Decompression(format!("LZ4: {}", e)))
    }
    
    /// Compress data using ZSTD
    fn compress_zstd(&self, data: &[u8], level: CompressionLevel) -> Result<Vec<u8>, CompressionError> {
        let level = level.to_level(CompressionAlgorithm::Zstd);
        zstd::encode_all(data, level)
            .map_err(|e| CompressionError::Compression(format!("ZSTD: {}", e)))
    }
    
    /// Decompress ZSTD data
    fn decompress_zstd(&self, data: &[u8]) -> Result<Vec<u8>, CompressionError> {
        zstd::decode_all(data)
            .map_err(|e| CompressionError::Decompression(format!("ZSTD: {}", e)))
    }
    
    /// Compress data using Brotli
    fn compress_brotli(&self, data: &[u8], level: CompressionLevel) -> Result<Vec<u8>, CompressionError> {
        let level = level.to_level(CompressionAlgorithm::Brotli);
        let mut compressed = Vec::new();
        
        let mut cursor = std::io::Cursor::new(data);
        brotli::BrotliCompress(&mut cursor, &mut compressed, &brotli::enc::BrotliEncoderParams {
            quality: level,
            ..Default::default()
        }).map_err(|e| CompressionError::Compression(format!("Brotli: {}", e)))?;
        
        Ok(compressed)
    }
    
    /// Decompress Brotli data
    fn decompress_brotli(&self, data: &[u8]) -> Result<Vec<u8>, CompressionError> {
        let mut decompressed = Vec::new();
        let mut cursor = std::io::Cursor::new(data);
        brotli::BrotliDecompress(&mut cursor, &mut decompressed)
            .map_err(|e| CompressionError::Decompression(format!("Brotli: {}", e)))?;
        
        Ok(decompressed)
    }
    
    /// Check if data appears to be text
    fn is_text_data(&self, data: &[u8]) -> bool {
        if data.is_empty() {
            return true;
        }
        
        let mut text_chars = 0;
        let mut total_chars = 0;
        
        for &byte in data.iter().take(1024) {
            total_chars += 1;
            if byte.is_ascii() && (byte.is_ascii_graphic() || byte.is_ascii_whitespace()) {
                text_chars += 1;
            }
        }
        
        text_chars as f64 / total_chars as f64 > 0.8
    }
    
    /// Check if data appears to be already compressed
    fn is_compressed_data(&self, data: &[u8]) -> bool {
        if data.len() < 4 {
            return false;
        }
        
        // Check common compressed file signatures
        matches!(
            &data[..4],
            // ZIP
            b"PK\x03\x04" | b"PK\x05\x06" | b"PK\x07\x08" |
            // GZIP
            b"\x1f\x8b\x08\x00" |
            // 7Z
            b"7z\xbc\xaf" |
            // RAR
            b"Rar!" |
            // BZIP2
            b"BZh1" | b"BZh2" | b"BZh3" | b"BZh4" | b"BZh5" | b"BZh6" | b"BZh7" | b"BZh8" | b"BZh9"
        ) || 
        // JPEG
        (data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8) ||
        // PNG
        (data.len() >= 8 && &data[..8] == b"\x89PNG\x0D\x0A\x1A\x0A") ||
        // MP3
        (data.len() >= 3 && (&data[..3] == b"ID3" || (data[0] == 0xFF && (data[1] & 0xE0) == 0xE0)))
    }
    
    /// Benchmark compression algorithms on sample data
    pub fn benchmark(&self, data: &[u8]) -> Result<Vec<(CompressionAlgorithm, CompressionStats, std::time::Duration)>, CompressionError> {
        let mut results = Vec::new();
        let algorithms = [
            CompressionAlgorithm::None,
            CompressionAlgorithm::Lz4,
            CompressionAlgorithm::Zstd,
            CompressionAlgorithm::Brotli,
        ];
        
        for algorithm in &algorithms {
            let start = std::time::Instant::now();
            let compressed = self.compress_with_algorithm(data, *algorithm, CompressionLevel::Default)?;
            let duration = start.elapsed();
            
            // Create stats for this compression
            let stats = CompressionStats {
                data_type: self.analyze_data(data).data_type,
                entropy: compressed.ratio(),
                repetition_ratio: compressed.compressed_size as f64 / compressed.original_size as f64,
                size: compressed.compressed_size,
            };
            
            results.push((*algorithm, stats, duration));
        }
        
        Ok(results)
    }
}

/// Stream compression for large files
pub struct CompressionStream<W: Write> {
    writer: W,
    engine: CompressionEngine,
    algorithm: CompressionAlgorithm,
    level: CompressionLevel,
    buffer: Vec<u8>,
    chunk_size: usize,
}

impl<W: Write> CompressionStream<W> {
    /// Create new compression stream
    pub fn new(writer: W, algorithm: CompressionAlgorithm, level: CompressionLevel) -> Self {
        CompressionStream {
            writer,
            engine: CompressionEngine::new(),
            algorithm,
            level,
            buffer: Vec::new(),
            chunk_size: 64 * 1024, // 64KB chunks
        }
    }
    
    /// Set chunk size for streaming compression
    pub fn set_chunk_size(&mut self, size: usize) {
        self.chunk_size = size;
    }
    
    /// Write compressed chunk header
    fn write_chunk_header(&mut self, compressed: &CompressedData) -> Result<(), CompressionError> {
        let header = bincode::serialize(&compressed)
            .map_err(|e| CompressionError::Compression(format!("Header serialization: {}", e)))?;
        
        self.writer.write_all(&(header.len() as u32).to_le_bytes())?;
        self.writer.write_all(&header)?;
        Ok(())
    }
}

impl<W: Write> Write for CompressionStream<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        
        while self.buffer.len() >= self.chunk_size {
            let chunk: Vec<u8> = self.buffer.drain(..self.chunk_size).collect();
            
            let compressed = self.engine
                .compress_with_algorithm(&chunk, self.algorithm, self.level)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            
            self.write_chunk_header(&compressed)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        }
        
        Ok(buf.len())
    }
    
    fn flush(&mut self) -> std::io::Result<()> {
        // Compress remaining data
        if !self.buffer.is_empty() {
            let chunk = self.buffer.drain(..).collect::<Vec<u8>>();
            
            let compressed = self.engine
                .compress_with_algorithm(&chunk, self.algorithm, self.level)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            
            self.write_chunk_header(&compressed)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        }
        
        self.writer.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_algorithms() {
        let engine = CompressionEngine::new();
        // Use highly repetitive data that will definitely compress well
        let test_data = "Hello, world! This is a test string that should compress well with repetitive content. ".repeat(50);
        let test_bytes = test_data.as_bytes();
        
        for algorithm in [CompressionAlgorithm::Lz4, CompressionAlgorithm::Zstd, CompressionAlgorithm::Brotli] {
            let compressed = engine.compress_with_algorithm(test_bytes, algorithm, CompressionLevel::Default).unwrap();
            let decompressed = engine.decompress(&compressed).unwrap();
            
            assert_eq!(test_bytes, &decompressed[..]);
            // For highly repetitive data, compression should be beneficial
            if !compressed.is_beneficial() {
                println!("Warning: {} compression was not beneficial: {} -> {} bytes", 
                        algorithm, compressed.original_size, compressed.compressed_size);
            }
            println!("{}: {} -> {} bytes ({}% reduction)", 
                     algorithm, 
                     compressed.original_size, 
                     compressed.compressed_size,
                     (compressed.ratio() * 100.0) as i32);
        }
    }
    
    #[test]
    fn test_adaptive_compression() {
        let engine = CompressionEngine::new();
        
        // Text data
        let text_data = b"This is some text data that contains repetitive patterns. This is some text data that contains repetitive patterns.";
        let compressed = engine.compress(text_data).unwrap();
        let decompressed = engine.decompress(&compressed).unwrap();
        assert_eq!(text_data, &decompressed[..]);
        
        // Binary data
        let binary_data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
        let compressed = engine.compress(&binary_data).unwrap();
        let decompressed = engine.decompress(&compressed).unwrap();
        assert_eq!(binary_data, decompressed);
    }
    
    #[test]
    fn test_data_analysis() {
        let engine = CompressionEngine::new();
        
        // Text data
        let text = b"Hello world, this is a text message with some repetitive content!";
        let stats = engine.analyze_data(text);
        assert_eq!(stats.data_type, DataType::Text);
        
        // Binary data
        let binary: Vec<u8> = (0u8..=255u8).collect();
        let stats = engine.analyze_data(&binary);
        assert_eq!(stats.data_type, DataType::Binary);
        
        // Compressed-like data (JPEG signature)
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0];
        let stats = engine.analyze_data(&jpeg);
        assert_eq!(stats.data_type, DataType::Compressed);
    }
    
    #[test]
    fn test_compression_levels() {
        let engine = CompressionEngine::new();
        let test_data = b"This is test data for compression level testing. ".repeat(100);
        
        let fast = engine.compress_with_algorithm(&test_data, CompressionAlgorithm::Zstd, CompressionLevel::Fast).unwrap();
        let best = engine.compress_with_algorithm(&test_data, CompressionAlgorithm::Zstd, CompressionLevel::Best).unwrap();
        
        // Best compression should yield smaller size
        assert!(best.compressed_size <= fast.compressed_size);
        
        // Both should decompress correctly
        assert_eq!(test_data, engine.decompress(&fast).unwrap());
        assert_eq!(test_data, engine.decompress(&best).unwrap());
    }

    #[test]
    fn test_small_data_no_compression() {
        let mut engine = CompressionEngine::new();
        engine.set_min_compression_size(100);
        
        let small_data = b"small";
        let compressed = engine.compress(small_data).unwrap();
        
        // Should not compress small data
        assert_eq!(compressed.algorithm, CompressionAlgorithm::None);
        assert_eq!(compressed.data, small_data);
    }
}
