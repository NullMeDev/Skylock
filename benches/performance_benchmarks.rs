use criterion::{criterion_group, criterion_main, Criterion};
use skylock_hybrid::backup;
use std::path::PathBuf;

fn backup_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("backup_operations");
    
    group.bench_function("small_file_backup", |b| {
        b.iter(|| {
            let config = backup::BackupConfig {
                vss_enabled: false,
                schedule: "0 2 * * *".to_string(),
                retention_days: 30,
                backup_paths: vec![PathBuf::from("./test_data/small.txt")],
            };
            
            let backup_service = backup::BackupService::new();
            backup_service.create_backup(&config)
        })
    });

    group.bench_function("deduplication", |b| {
        b.iter(|| {
            // Benchmark deduplication logic
            let backup_service = backup::BackupService::new();
            backup_service.deduplicate_data(&[0u8; 1024])
        })
    });

    group.finish();
}

fn encryption_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("encryption_operations");
    
    group.bench_function("encrypt_1mb", |b| {
        let data = vec![0u8; 1024 * 1024];
        b.iter(|| {
            // Benchmark encryption
            skylock_hybrid::crypto::encrypt_data(&data)
        })
    });

    group.bench_function("decrypt_1mb", |b| {
        let data = vec![0u8; 1024 * 1024];
        let encrypted = skylock_hybrid::crypto::encrypt_data(&data).unwrap();
        b.iter(|| {
            // Benchmark decryption
            skylock_hybrid::crypto::decrypt_data(&encrypted)
        })
    });

    group.finish();
}

fn compression_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_operations");
    
    group.bench_function("compress_1mb", |b| {
        let data = vec![0u8; 1024 * 1024];
        b.iter(|| {
            // Benchmark compression
            skylock_hybrid::compression::compress_data(&data)
        })
    });

    group.bench_function("decompress_1mb", |b| {
        let data = vec![0u8; 1024 * 1024];
        let compressed = skylock_hybrid::compression::compress_data(&data).unwrap();
        b.iter(|| {
            // Benchmark decompression
            skylock_hybrid::compression::decompress_data(&compressed)
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    backup_performance,
    encryption_performance,
    compression_performance
);
criterion_main!(benches);