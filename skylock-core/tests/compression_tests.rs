use crate::compression::{CompressionConfig, CompressionEngine, CompressionAlgorithm};
use std::path::PathBuf;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_compression_algorithms() -> Result<(), Box<dyn std::error::Error>> {
    let test_data = b"This is test data that should be compressible because it contains repeated patterns. ".repeat(100);
    let config = CompressionConfig::default();
    let engine = CompressionEngine::new(config);

    // Test Zstd compression
    let compressed = engine.compress(&test_data)?;
    assert!(compressed.len() < test_data.len());
    let decompressed = engine.decompress(&compressed, CompressionAlgorithm::Zstd)?;
    assert_eq!(test_data.to_vec(), decompressed);

    // Test with different algorithms
    for algorithm in &[
        CompressionAlgorithm::Gzip,
        CompressionAlgorithm::Zlib,
        CompressionAlgorithm::Lz4,
        CompressionAlgorithm::None,
    ] {
        let config = CompressionConfig {
            algorithm: *algorithm,
            ..Default::default()
        };
        let engine = CompressionEngine::new(config);

        let compressed = engine.compress(&test_data)?;
        if *algorithm != CompressionAlgorithm::None {
            assert!(compressed.len() < test_data.len(), "Data should be compressed with {:?}", algorithm);
        }
        let decompressed = engine.decompress(&compressed, *algorithm)?;
        assert_eq!(test_data.to_vec(), decompressed, "Compression roundtrip failed for {:?}", algorithm);
    }

    Ok(())
}

#[test]
fn test_compression_levels() -> Result<(), Box<dyn std::error::Error>> {
    let test_data = b"Compressible test data with patterns ".repeat(1000);
    let mut sizes = Vec::new();

    // Test different compression levels
    for level in 1..=9 {
        let config = CompressionConfig {
            algorithm: CompressionAlgorithm::Zstd,
            level,
            ..Default::default()
        };
        let engine = CompressionEngine::new(config);

        let compressed = engine.compress(&test_data)?;
        sizes.push(compressed.len());

        // Verify decompression works
        let decompressed = engine.decompress(&compressed, CompressionAlgorithm::Zstd)?;
        assert_eq!(test_data.to_vec(), decompressed);
    }

    // Higher compression levels should generally result in smaller sizes
    for i in 1..sizes.len() {
        assert!(sizes[i] <= sizes[0], "Higher compression levels should not produce larger output");
    }

    Ok(())
}

#[test]
fn test_file_type_handling() -> Result<(), Box<dyn std::error::Error>> {
    let config = CompressionConfig {
        algorithm: CompressionAlgorithm::Zstd,
        level: 3,
        min_size: 1000,
        skip_extensions: vec!["jpg".into(), "mp3".into(), "zip".into()],
    };
    let engine = CompressionEngine::new(config);

    // Test various file types and sizes
    let test_cases = vec![
        ("test.txt", 2000, true),
        ("image.jpg", 2000, false),
        ("music.mp3", 2000, false),
        ("archive.zip", 2000, false),
        ("small.txt", 500, false),
        ("large.doc", 5000, true),
    ];

    for (name, size, should_compress) in test_cases {
        assert_eq!(
            engine.should_compress(&PathBuf::from(name), size),
            should_compress,
            "Unexpected compression decision for {}",
            name
        );
    }

    Ok(())
}

#[test]
fn test_compression_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
    let config = CompressionConfig::default();
    let engine = CompressionEngine::new(config);

    // Test empty input
    let empty_data = b"";
    let compressed = engine.compress(empty_data)?;
    let decompressed = engine.decompress(&compressed, CompressionAlgorithm::Zstd)?;
    assert_eq!(empty_data.to_vec(), decompressed);

    // Test small input
    let small_data = b"x";
    let compressed = engine.compress(small_data)?;
    let decompressed = engine.decompress(&compressed, CompressionAlgorithm::Zstd)?;
    assert_eq!(small_data.to_vec(), decompressed);

    // Test random/incompressible data
    let mut random_data = vec![0u8; 1000];
    for i in 0..random_data.len() {
        random_data[i] = rand::random();
    }
    let compressed = engine.compress(&random_data)?;
    let decompressed = engine.decompress(&compressed, CompressionAlgorithm::Zstd)?;
    assert_eq!(random_data, decompressed);

    Ok(())
}

#[test]
fn test_compression_with_real_files() -> Result<(), Box<dyn std::error::Error>> {
    let config = CompressionConfig::default();
    let engine = CompressionEngine::new(config);

    // Create a text file
    let mut text_file = NamedTempFile::new()?;
    writeln!(text_file, "This is a test file with repetitive content.\n")?;
    writeln!(text_file, "This line is repeated many times.\n".repeat(100))?;
    text_file.flush()?;

    // Create a binary file
    let mut bin_file = NamedTempFile::new()?;
    for i in 0..1000 {
        bin_file.write_all(&[i as u8])?;
    }
    bin_file.flush()?;

    // Test text file compression
    assert!(engine.should_compress(text_file.path(), text_file.as_file().metadata()?.len()));
    let text_data = std::fs::read(text_file.path())?;
    let compressed = engine.compress(&text_data)?;
    assert!(compressed.len() < text_data.len());
    let decompressed = engine.decompress(&compressed, CompressionAlgorithm::Zstd)?;
    assert_eq!(text_data, decompressed);

    // Test binary file compression
    assert!(engine.should_compress(bin_file.path(), bin_file.as_file().metadata()?.len()));
    let bin_data = std::fs::read(bin_file.path())?;
    let compressed = engine.compress(&bin_data)?;
    let decompressed = engine.decompress(&compressed, CompressionAlgorithm::Zstd)?;
    assert_eq!(bin_data, decompressed);

    Ok(())
}
