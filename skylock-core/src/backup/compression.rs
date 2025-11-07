use std::io::Write;
use flate2::write::{GzEncoder, ZlibEncoder};
use flate2::Compression;
use lz4_flex::frame::FrameEncoder;
use zstd::stream::write::Encoder as ZstdEncoder;
use crate::Result;

#[derive(Debug, Clone, Copy)]
pub enum CompressionAlgorithm {
    None,
    Gzip,
    Zlib,
    Lz4,
    Zstd,
}

#[derive(Debug, Clone)]
pub struct CompressionConfig {
    pub algorithm: CompressionAlgorithm,
    pub level: u32,
    pub min_size: u64,            // Minimum file size to attempt compression
    pub skip_extensions: Vec<String>, // File extensions to skip compression
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            algorithm: CompressionAlgorithm::Zstd,
            level: 3,
            min_size: 4096, // Don't compress files smaller than 4KB
            skip_extensions: vec![
                "jpg".into(), "jpeg".into(), "png".into(), "gif".into(),
                "mp3".into(), "mp4".into(), "zip".into(), "gz".into(),
                "7z".into(), "rar".into()
            ],
        }
    }
}

pub struct CompressionEngine {
    config: CompressionConfig,
}

impl CompressionEngine {
    pub fn new(config: CompressionConfig) -> Self {
        Self { config }
    }

    pub fn should_compress(&self, path: &std::path::Path, size: u64) -> bool {
        // Skip if file is too small
        if size < self.config.min_size {
            return false;
        }

        // Skip if extension is in skip list
        if let Some(ext) = path.extension() {
            if let Some(ext_str) = ext.to_str() {
                if self.config.skip_extensions.iter().any(|s| s.eq_ignore_ascii_case(ext_str)) {
                    return false;
                }
            }
        }

        true
    }

    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self.config.algorithm {
            CompressionAlgorithm::None => Ok(data.to_vec()),
            CompressionAlgorithm::Gzip => {
                let mut encoder = GzEncoder::new(Vec::new(), Compression::new(self.config.level));
                encoder.write_all(data)?;
                Ok(encoder.finish()?)
            }
            CompressionAlgorithm::Zlib => {
                let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(self.config.level));
                encoder.write_all(data)?;
                Ok(encoder.finish()?)
            }
            CompressionAlgorithm::Lz4 => {
                let mut encoder = FrameEncoder::new(Vec::new());
                encoder.write_all(data)?;
                Ok(encoder.finish()?)
            }
            CompressionAlgorithm::Zstd => {
                let mut encoder = ZstdEncoder::new(Vec::new(), self.config.level as i32)?;
                encoder.write_all(data)?;
                Ok(encoder.finish()?)
            }
        }
    }

    pub fn decompress(&self, data: &[u8], algorithm: CompressionAlgorithm) -> Result<Vec<u8>> {
        match algorithm {
            CompressionAlgorithm::None => Ok(data.to_vec()),
            CompressionAlgorithm::Gzip => {
                let mut decoder = flate2::read::GzDecoder::new(data);
                let mut buf = Vec::new();
                std::io::copy(&mut decoder, &mut buf)?;
                Ok(buf)
            }
            CompressionAlgorithm::Zlib => {
                let mut decoder = flate2::read::ZlibDecoder::new(data);
                let mut buf = Vec::new();
                std::io::copy(&mut decoder, &mut buf)?;
                Ok(buf)
            }
            CompressionAlgorithm::Lz4 => {
                let mut decoder = lz4_flex::frame::FrameDecoder::new(data);
                let mut buf = Vec::new();
                std::io::copy(&mut decoder, &mut buf)?;
                Ok(buf)
            }
            CompressionAlgorithm::Zstd => {
                let mut decoder = zstd::stream::read::Decoder::new(data)?;
                let mut buf = Vec::new();
                std::io::copy(&mut decoder, &mut buf)?;
                Ok(buf)
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct CompressionStats {
    pub total_bytes: u64,
    pub compressed_bytes: u64,
    pub files_compressed: usize,
    pub files_skipped: usize,
}

impl CompressionStats {
    pub fn compression_ratio(&self) -> f64 {
        if self.total_bytes == 0 {
            1.0
        } else {
            self.compressed_bytes as f64 / self.total_bytes as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_compression_config() {
        let config = CompressionConfig::default();
        assert_eq!(config.level, 3);
        assert!(config.skip_extensions.contains(&"jpg".to_string()));
    }

    #[test]
    fn test_should_compress() {
        let config = CompressionConfig::default();
        let engine = CompressionEngine::new(config);

        // Test file size check
        assert!(!engine.should_compress(&PathBuf::from("test.txt"), 1024));
        assert!(engine.should_compress(&PathBuf::from("test.txt"), 10000));

        // Test extension check
        assert!(!engine.should_compress(&PathBuf::from("test.jpg"), 10000));
        assert!(engine.should_compress(&PathBuf::from("test.txt"), 10000));
    }

    #[test]
    fn test_compression_roundtrip() -> Result<()> {
        let config = CompressionConfig::default();
        let engine = CompressionEngine::new(config);

        let test_data = b"Hello, World!".repeat(1000);
        let compressed = engine.compress(&test_data)?;
        let decompressed = engine.decompress(&compressed, config.algorithm)?;

        assert_eq!(test_data.to_vec(), decompressed);
        assert!(compressed.len() < test_data.len());

        Ok(())
    }
}
