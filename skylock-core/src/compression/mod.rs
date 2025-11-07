use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;
use tokio::fs;

#[derive(Debug, Error)]
pub enum CompressionError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Compression error: {0}")]
    Compression(String),
    #[error("Decompression error: {0}")]
    Decompression(String),
    #[error("Invalid compression type")]
    InvalidType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CompressionType {
    Zstd,
    Lz4,
    Brotli,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    pub compression_type: CompressionType,
    pub level: i32,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            compression_type: CompressionType::Zstd,
            level: 3,
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

    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>, CompressionError> {
        match self.config.compression_type {
            CompressionType::Zstd => {
                let mut encoder = zstd::Encoder::new(Vec::new(), self.config.level)?;
                encoder.write_all(data)?;
                Ok(encoder.finish()?)
            }
            CompressionType::Lz4 => {
                let mut encoder = lz4::EncoderBuilder::new()
                    .level(self.config.level as u32)
                    .build(Vec::new())?;
                encoder.write_all(data)?;
                let (compressed, result) = encoder.finish();
                result?;
                Ok(compressed)
            }
            CompressionType::Brotli => {
                let mut compressed = Vec::new();
                let mut encoder = brotli::CompressorReader::new(
                    data,
                    4096, // buffer size
                    self.config.level as u32,
                    22,   // window size
                );
                std::io::copy(&mut encoder, &mut compressed)?;
                Ok(compressed)
            }
        }
    }

    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, CompressionError> {
        match self.config.compression_type {
            CompressionType::Zstd => {
                let mut decoder = zstd::Decoder::new(data)?;
                let mut decompressed = Vec::new();
                decoder.read_to_end(&mut decompressed)?;
                Ok(decompressed)
            }
            CompressionType::Lz4 => {
                let mut decoder = lz4::Decoder::new(data)?;
                let mut decompressed = Vec::new();
                decoder.read_to_end(&mut decompressed)?;
                Ok(decompressed)
            }
            CompressionType::Brotli => {
                let mut decompressed = Vec::new();
                let mut decoder = brotli::Decompressor::new(
                    data,
                    4096, // buffer size
                );
                std::io::copy(&mut decoder, &mut decompressed)?;
                Ok(decompressed)
            }
        }
    }

    pub async fn compress_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<(), CompressionError> {
        let data = fs::read(source).await?;
        let compressed = self.compress(&data)?;
        fs::write(destination, compressed).await?;
        Ok(())
    }

    pub async fn decompress_file(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<(), CompressionError> {
        let data = fs::read(source).await?;
        let decompressed = self.decompress(&data)?;
        fs::write(destination, decompressed).await?;
        Ok(())
    }
}

// Required for write_all and read_to_end
use std::io::{Read, Write};