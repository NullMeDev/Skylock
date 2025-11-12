//! Compression configuration and benchmarking
//! 
//! Provides configurable compression levels with performance/ratio trade-offs

use serde::{Serialize, Deserialize};
use crate::error::{Result, SkylockError};

/// Compression level preset
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CompressionLevel {
    /// No compression (fastest, largest files)
    None,
    /// Fast compression (level 1)
    Fast,
    /// Balanced compression/speed (level 3) - DEFAULT
    Balanced,
    /// Good compression (level 6)
    Good,
    /// Best compression (level 9, slowest)
    Best,
    /// Custom level (0-22)
    Custom(i32),
}

impl Default for CompressionLevel {
    fn default() -> Self {
        CompressionLevel::Balanced
    }
}

impl CompressionLevel {
    /// Get the numeric compression level for zstd
    pub fn to_zstd_level(&self) -> i32 {
        match self {
            CompressionLevel::None => 0,
            CompressionLevel::Fast => 1,
            CompressionLevel::Balanced => 3,
            CompressionLevel::Good => 6,
            CompressionLevel::Best => 9,
            CompressionLevel::Custom(level) => *level,
        }
    }
    
    /// Parse from string
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "none" | "0" => Ok(CompressionLevel::None),
            "fast" | "1" => Ok(CompressionLevel::Fast),
            "balanced" | "3" => Ok(CompressionLevel::Balanced),
            "good" | "6" => Ok(CompressionLevel::Good),
            "best" | "9" => Ok(CompressionLevel::Best),
            _ => {
                if let Ok(level) = s.parse::<i32>() {
                    if (0..=22).contains(&level) {
                        Ok(CompressionLevel::Custom(level))
                    } else {
                        Err(SkylockError::Backup(format!(
                            "Invalid compression level: {} (must be 0-22)", level
                        )))
                    }
                } else {
                    Err(SkylockError::Backup(format!(
                        "Invalid compression level: {}", s
                    )))
                }
            }
        }
    }
}

/// Compression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Compression level
    pub level: CompressionLevel,
    /// Minimum file size to compress (bytes)
    pub min_file_size: u64,
    /// Whether to show compression ratios
    pub show_ratios: bool,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            level: CompressionLevel::Balanced,
            min_file_size: 10 * 1024 * 1024, // 10 MB
            show_ratios: true,
        }
    }
}

impl CompressionConfig {
    /// Check if file should be compressed
    pub fn should_compress(&self, file_size: u64) -> bool {
        file_size >= self.min_file_size && self.level.to_zstd_level() > 0
    }
}

/// Compression statistics
#[derive(Debug, Clone)]
pub struct CompressionStats {
    pub original_size: u64,
    pub compressed_size: u64,
    pub ratio: f64,
    pub level: i32,
}

impl CompressionStats {
    pub fn new(original_size: u64, compressed_size: u64, level: i32) -> Self {
        let ratio = if compressed_size > 0 {
            original_size as f64 / compressed_size as f64
        } else {
            1.0
        };
        
        Self {
            original_size,
            compressed_size,
            ratio,
            level,
        }
    }
    
    pub fn savings_percent(&self) -> f64 {
        if self.original_size > 0 {
            100.0 * (1.0 - (self.compressed_size as f64 / self.original_size as f64))
        } else {
            0.0
        }
    }
    
    pub fn format_ratio(&self) -> String {
        format!("{:.2}x", self.ratio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compression_level_parsing() {
        assert_eq!(CompressionLevel::from_str("none").unwrap(), CompressionLevel::None);
        assert_eq!(CompressionLevel::from_str("3").unwrap(), CompressionLevel::Balanced);
        assert_eq!(CompressionLevel::from_str("balanced").unwrap(), CompressionLevel::Balanced);
        
        match CompressionLevel::from_str("15").unwrap() {
            CompressionLevel::Custom(15) => {},
            _ => panic!("Should be Custom(15)"),
        }
        
        assert!(CompressionLevel::from_str("invalid").is_err());
        assert!(CompressionLevel::from_str("99").is_err());
    }
    
    #[test]
    fn test_compression_stats() {
        let stats = CompressionStats::new(1000, 500, 3);
        assert_eq!(stats.ratio, 2.0);
        assert_eq!(stats.savings_percent(), 50.0);
        assert_eq!(stats.format_ratio(), "2.00x");
    }
    
    #[test]
    fn test_should_compress() {
        let config = CompressionConfig::default();
        assert!(!config.should_compress(1024)); // 1 KB - too small
        assert!(config.should_compress(20 * 1024 * 1024)); // 20 MB - should compress
        
        let no_compression = CompressionConfig {
            level: CompressionLevel::None,
            ..Default::default()
        };
        assert!(!no_compression.should_compress(100 * 1024 * 1024)); // Never compress
    }
}
