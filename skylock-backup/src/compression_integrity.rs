//! Compression Integrity Verification Module
//!
//! Ensures lossless compression by verifying data integrity:
//! - Pre-compression hash of original data
//! - Post-decompression hash verification
//! - Automatic rollback on integrity failures
//!
//! This guarantees that compressed files can be perfectly restored
//! to their original state without any data loss.

use std::io::{Read, Write};
use sha2::{Sha256, Digest};
use zstd;
use crate::error::{Result, SkylockError};

/// Compression level (matches zstd default for skylock)
pub const DEFAULT_COMPRESSION_LEVEL: i32 = 3;

/// Minimum file size for compression (10MB)
pub const MIN_COMPRESSION_SIZE: u64 = 10 * 1024 * 1024;

/// Compression result with integrity verification data
#[derive(Debug, Clone)]
pub struct VerifiedCompression {
    /// Compressed data
    pub compressed_data: Vec<u8>,
    /// SHA-256 hash of original data
    pub original_hash: String,
    /// Size of original data
    pub original_size: u64,
    /// SHA-256 hash of compressed data
    pub compressed_hash: String,
    /// Compression ratio (original_size / compressed_size)
    pub compression_ratio: f64,
    /// Whether compression was applied (may be skipped for small gains)
    pub was_compressed: bool,
}

/// Decompression result with integrity verification
#[derive(Debug, Clone)]
pub struct VerifiedDecompression {
    /// Decompressed data
    pub data: Vec<u8>,
    /// SHA-256 hash of decompressed data
    pub hash: String,
    /// Whether hash matches expected value
    pub integrity_verified: bool,
}

/// Compression integrity verifier
pub struct CompressionVerifier {
    /// Compression level
    compression_level: i32,
    /// Minimum ratio to accept compression (e.g., 0.95 = 5% savings minimum)
    min_compression_ratio: f64,
}

impl Default for CompressionVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl CompressionVerifier {
    /// Create a new compression verifier with default settings
    pub fn new() -> Self {
        Self {
            compression_level: DEFAULT_COMPRESSION_LEVEL,
            min_compression_ratio: 0.95, // At least 5% savings
        }
    }
    
    /// Create with custom compression level
    pub fn with_level(level: i32) -> Self {
        Self {
            compression_level: level.clamp(1, 22),
            min_compression_ratio: 0.95,
        }
    }
    
    /// Set minimum compression ratio threshold
    pub fn with_min_ratio(mut self, ratio: f64) -> Self {
        self.min_compression_ratio = ratio.clamp(0.5, 1.0);
        self
    }
    
    /// Compress data with integrity verification
    /// 
    /// Returns verified compression result with hashes for later verification
    pub fn compress_verified(&self, data: &[u8]) -> Result<VerifiedCompression> {
        // Calculate original hash BEFORE compression
        let original_hash = calculate_hash(data);
        let original_size = data.len() as u64;
        
        // Compress the data
        let compressed_data = zstd::encode_all(data, self.compression_level)
            .map_err(|e| SkylockError::Compression(
                format!("Compression failed: {}", e)
            ))?;
        
        // Check compression ratio
        let compressed_size = compressed_data.len() as u64;
        let compression_ratio = compressed_size as f64 / original_size as f64;
        
        // Skip compression if gains are minimal
        if compression_ratio >= self.min_compression_ratio {
            return Ok(VerifiedCompression {
                compressed_data: data.to_vec(),
                original_hash: original_hash.clone(),
                original_size,
                compressed_hash: original_hash,
                compression_ratio: 1.0,
                was_compressed: false,
            });
        }
        
        // Calculate compressed data hash
        let compressed_hash = calculate_hash(&compressed_data);
        
        // VERIFY: Decompress and check integrity before returning
        let verify_result = self.decompress_internal(&compressed_data)?;
        let verify_hash = calculate_hash(&verify_result);
        
        if verify_hash != original_hash {
            return Err(SkylockError::Compression(
                format!(
                    "Compression integrity check failed: original hash {} != decompressed hash {}",
                    original_hash, verify_hash
                )
            ));
        }
        
        Ok(VerifiedCompression {
            compressed_data,
            original_hash,
            original_size,
            compressed_hash,
            compression_ratio,
            was_compressed: true,
        })
    }
    
    /// Decompress data with integrity verification
    /// 
    /// Verifies the decompressed data matches the expected hash
    pub fn decompress_verified(
        &self,
        compressed_data: &[u8],
        expected_hash: &str,
        was_compressed: bool,
    ) -> Result<VerifiedDecompression> {
        let data = if was_compressed {
            self.decompress_internal(compressed_data)?
        } else {
            // Data wasn't actually compressed, return as-is
            compressed_data.to_vec()
        };
        
        // Calculate hash of decompressed data
        let hash = calculate_hash(&data);
        
        // Verify integrity
        let integrity_verified = hash == expected_hash;
        
        if !integrity_verified {
            return Err(SkylockError::Compression(
                format!(
                    "Decompression integrity check failed: expected {} got {}",
                    expected_hash, hash
                )
            ));
        }
        
        Ok(VerifiedDecompression {
            data,
            hash,
            integrity_verified,
        })
    }
    
    /// Internal decompression without verification
    fn decompress_internal(&self, data: &[u8]) -> Result<Vec<u8>> {
        zstd::decode_all(data)
            .map_err(|e| SkylockError::Compression(
                format!("Decompression failed: {}", e)
            ))
    }
    
    /// Compress streaming data with verification
    pub fn compress_stream_verified<R: Read, W: Write>(
        &self,
        mut reader: R,
        mut writer: W,
    ) -> Result<StreamCompressionResult> {
        // Read all data (for hashing)
        let mut data = Vec::new();
        reader.read_to_end(&mut data)
            .map_err(|e| SkylockError::Io(e))?;
        
        // Compress with verification
        let result = self.compress_verified(&data)?;
        
        // Write compressed data
        writer.write_all(&result.compressed_data)
            .map_err(|e| SkylockError::Io(e))?;
        
        Ok(StreamCompressionResult {
            original_hash: result.original_hash,
            original_size: result.original_size,
            compressed_size: result.compressed_data.len() as u64,
            compression_ratio: result.compression_ratio,
            was_compressed: result.was_compressed,
        })
    }
    
    /// Decompress streaming data with verification
    pub fn decompress_stream_verified<R: Read, W: Write>(
        &self,
        mut reader: R,
        mut writer: W,
        expected_hash: &str,
        was_compressed: bool,
    ) -> Result<StreamDecompressionResult> {
        // Read compressed data
        let mut compressed_data = Vec::new();
        reader.read_to_end(&mut compressed_data)
            .map_err(|e| SkylockError::Io(e))?;
        
        // Decompress with verification
        let result = self.decompress_verified(&compressed_data, expected_hash, was_compressed)?;
        
        // Write decompressed data
        writer.write_all(&result.data)
            .map_err(|e| SkylockError::Io(e))?;
        
        Ok(StreamDecompressionResult {
            decompressed_size: result.data.len() as u64,
            hash: result.hash,
            integrity_verified: result.integrity_verified,
        })
    }
}

/// Result of streaming compression
#[derive(Debug, Clone)]
pub struct StreamCompressionResult {
    pub original_hash: String,
    pub original_size: u64,
    pub compressed_size: u64,
    pub compression_ratio: f64,
    pub was_compressed: bool,
}

/// Result of streaming decompression
#[derive(Debug, Clone)]
pub struct StreamDecompressionResult {
    pub decompressed_size: u64,
    pub hash: String,
    pub integrity_verified: bool,
}

/// Calculate SHA-256 hash of data
pub fn calculate_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Quick integrity check - verifies hash without decompression
pub fn verify_compressed_hash(data: &[u8], expected_hash: &str) -> bool {
    let hash = calculate_hash(data);
    // Use constant-time comparison for security
    use subtle::ConstantTimeEq;
    hash.as_bytes().ct_eq(expected_hash.as_bytes()).into()
}

/// Compression metadata stored with files
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompressionMetadata {
    /// Whether the file was compressed
    pub compressed: bool,
    /// Hash of original (uncompressed) data
    pub original_hash: String,
    /// Size of original data
    pub original_size: u64,
    /// Hash of compressed data (if compressed)
    pub compressed_hash: Option<String>,
    /// Compression level used
    pub compression_level: Option<i32>,
    /// Compression ratio achieved
    pub compression_ratio: Option<f64>,
}

impl CompressionMetadata {
    /// Create metadata for uncompressed file
    pub fn uncompressed(data: &[u8]) -> Self {
        Self {
            compressed: false,
            original_hash: calculate_hash(data),
            original_size: data.len() as u64,
            compressed_hash: None,
            compression_level: None,
            compression_ratio: None,
        }
    }
    
    /// Create metadata from verified compression
    pub fn from_verified(result: &VerifiedCompression, level: i32) -> Self {
        Self {
            compressed: result.was_compressed,
            original_hash: result.original_hash.clone(),
            original_size: result.original_size,
            compressed_hash: if result.was_compressed {
                Some(result.compressed_hash.clone())
            } else {
                None
            },
            compression_level: if result.was_compressed {
                Some(level)
            } else {
                None
            },
            compression_ratio: if result.was_compressed {
                Some(result.compression_ratio)
            } else {
                None
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compression_roundtrip() {
        let verifier = CompressionVerifier::new();
        
        // Create compressible test data (repeated pattern compresses well)
        let original_data = "Hello, World! ".repeat(10000).into_bytes();
        
        // Compress with verification
        let compressed = verifier.compress_verified(&original_data).unwrap();
        
        assert!(compressed.was_compressed);
        assert!(compressed.compression_ratio < 0.5); // Should compress well
        assert_eq!(compressed.original_size, original_data.len() as u64);
        
        // Decompress with verification
        let decompressed = verifier.decompress_verified(
            &compressed.compressed_data,
            &compressed.original_hash,
            compressed.was_compressed,
        ).unwrap();
        
        assert!(decompressed.integrity_verified);
        assert_eq!(decompressed.data, original_data);
        assert_eq!(decompressed.hash, compressed.original_hash);
    }
    
    #[test]
    fn test_incompressible_data_skipped() {
        use rand::{RngCore, rngs::OsRng};
        let verifier = CompressionVerifier::new();
        
        // Truly random data doesn't compress well
        let mut random_data = vec![0u8; 4096];
        OsRng.fill_bytes(&mut random_data);
        
        let result = verifier.compress_verified(&random_data).unwrap();
        
        // Random data either won't compress or will have ratio >= 0.95 threshold
        // So it should be marked as not compressed
        assert!(!result.was_compressed);
        assert_eq!(result.compression_ratio, 1.0);
        assert_eq!(result.compressed_data, random_data);
    }
    
    #[test]
    fn test_wrong_hash_fails() {
        let verifier = CompressionVerifier::new();
        let data = b"Test data for compression verification";
        
        let compressed = verifier.compress_verified(data).unwrap();
        
        // Try to decompress with wrong expected hash
        let result = verifier.decompress_verified(
            &compressed.compressed_data,
            "wrong_hash_value",
            compressed.was_compressed,
        );
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("integrity check failed"));
    }
    
    #[test]
    fn test_corrupted_data_fails() {
        let verifier = CompressionVerifier::new();
        let data = "Compress this data ".repeat(1000).into_bytes();
        
        let compressed = verifier.compress_verified(&data).unwrap();
        assert!(compressed.was_compressed);
        
        // Corrupt the compressed data
        let mut corrupted = compressed.compressed_data.clone();
        if corrupted.len() > 10 {
            corrupted[5] ^= 0xFF;
            corrupted[10] ^= 0xFF;
        }
        
        // Decompression should fail (either bad zstd or hash mismatch)
        let result = verifier.decompress_verified(
            &corrupted,
            &compressed.original_hash,
            true,
        );
        
        assert!(result.is_err());
    }
    
    #[test]
    fn test_hash_calculation() {
        let data = b"Test data for hashing";
        let hash1 = calculate_hash(data);
        let hash2 = calculate_hash(data);
        
        // Same data should produce same hash
        assert_eq!(hash1, hash2);
        
        // Different data should produce different hash
        let hash3 = calculate_hash(b"Different data");
        assert_ne!(hash1, hash3);
        
        // Hash should be 64 hex characters (256 bits)
        assert_eq!(hash1.len(), 64);
        assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));
    }
    
    #[test]
    fn test_verify_compressed_hash() {
        let data = b"Data to hash";
        let hash = calculate_hash(data);
        
        assert!(verify_compressed_hash(data, &hash));
        assert!(!verify_compressed_hash(data, "wrong_hash"));
    }
    
    #[test]
    fn test_compression_metadata() {
        let verifier = CompressionVerifier::new();
        let data = "Compressible data ".repeat(1000).into_bytes();
        
        let result = verifier.compress_verified(&data).unwrap();
        let metadata = CompressionMetadata::from_verified(&result, DEFAULT_COMPRESSION_LEVEL);
        
        assert!(metadata.compressed);
        assert_eq!(metadata.original_size, data.len() as u64);
        assert!(metadata.compressed_hash.is_some());
        assert!(metadata.compression_ratio.unwrap() < 1.0);
    }
    
    #[test]
    fn test_uncompressed_metadata() {
        let data = b"Small data";
        let metadata = CompressionMetadata::uncompressed(data);
        
        assert!(!metadata.compressed);
        assert_eq!(metadata.original_size, data.len() as u64);
        assert!(metadata.compressed_hash.is_none());
        assert!(metadata.compression_level.is_none());
    }
    
    #[test]
    fn test_streaming_compression() {
        let verifier = CompressionVerifier::new();
        let data = "Stream this data ".repeat(1000).into_bytes();
        
        let mut compressed = Vec::new();
        let result = verifier.compress_stream_verified(
            data.as_slice(),
            &mut compressed,
        ).unwrap();
        
        assert!(result.was_compressed);
        assert_eq!(result.original_size, data.len() as u64);
        assert!(result.compressed_size < result.original_size);
        
        // Verify decompression
        let mut decompressed = Vec::new();
        let decomp_result = verifier.decompress_stream_verified(
            compressed.as_slice(),
            &mut decompressed,
            &result.original_hash,
            true,
        ).unwrap();
        
        assert!(decomp_result.integrity_verified);
        assert_eq!(decompressed, data);
    }
    
    #[test]
    fn test_compression_levels() {
        let data = "Test data for levels ".repeat(500).into_bytes();
        
        // Test different compression levels
        for level in [1, 3, 10, 19] {
            let verifier = CompressionVerifier::with_level(level);
            let result = verifier.compress_verified(&data).unwrap();
            
            // All levels should preserve data integrity
            let decompressed = verifier.decompress_verified(
                &result.compressed_data,
                &result.original_hash,
                result.was_compressed,
            ).unwrap();
            
            assert_eq!(decompressed.data, data);
        }
    }
    
    #[test]
    fn test_empty_data() {
        let verifier = CompressionVerifier::new();
        let empty_data: Vec<u8> = vec![];
        
        let result = verifier.compress_verified(&empty_data).unwrap();
        
        // Empty data shouldn't be compressed (no benefit)
        assert!(!result.was_compressed);
        assert_eq!(result.original_size, 0);
        
        // Verify roundtrip
        let decompressed = verifier.decompress_verified(
            &result.compressed_data,
            &result.original_hash,
            result.was_compressed,
        ).unwrap();
        
        assert!(decompressed.data.is_empty());
    }
    
    #[test]
    fn test_large_data_compression() {
        let verifier = CompressionVerifier::new();
        
        // Create large test data (1MB of compressible content)
        let data = "Large data block for testing compression ".repeat(25000).into_bytes();
        assert!(data.len() > 1_000_000);
        
        let result = verifier.compress_verified(&data).unwrap();
        
        assert!(result.was_compressed);
        assert!(result.compression_ratio < 0.1); // Should compress very well
        
        // Verify integrity
        let decompressed = verifier.decompress_verified(
            &result.compressed_data,
            &result.original_hash,
            true,
        ).unwrap();
        
        assert_eq!(decompressed.data, data);
    }
}
