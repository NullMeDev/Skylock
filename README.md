# Skylock

[![Version](https://img.shields.io/badge/version-0.4.0-blue.svg)](https://github.com/NullMeDev/Skylock/releases)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](.github/workflows/ci.yml)

**Contact:** null@nullme.lol

A secure, encrypted backup system with client-side AES-256-GCM encryption, built in Rust for reliability and performance.

## Features

### Implemented

**Core Security**
- AES-256-GCM client-side encryption with authenticated encryption (AEAD)
- Argon2id key derivation for password-based keys
- Per-file encryption with unique nonces and integrity verification
- TLS 1.3 transport security for all network communications
- Ed25519 SSH key authentication support for SFTP

**Backup Operations**
- Direct upload mode: per-file streaming with parallel uploads
- Archive mode: tar.zst.enc compressed archives (legacy)
- Resume interrupted uploads: automatic state tracking and recovery
- Bandwidth throttling: configurable upload speed limiting
- File-level deduplication and metadata tracking
- Backup manifest system with JSON metadata
- Professional backup ID structure (backup_YYYYMMDD_HHMMSS)
- Adaptive concurrency control to prevent system overload
- Real-time progress bars with upload speed and ETA
- Individual file and overall backup progress tracking

**Storage Integration**
- Hetzner Storage Box support via WebDAV (HTTPS)
- Hetzner Storage Box support via SFTP (SSH)
- Automatic directory creation and path management
- Connection testing and validation
- Configurable storage paths and endpoints

**Compression**
- Zstd compression (level 3) for files >10MB
- Smart compression: only compresses when beneficial
- Streaming compression for memory efficiency

**CLI Interface**
- `backup` - Create backups with direct or archive mode
- `list` - List all backups with metadata
- `restore` - Restore entire backups or individual files
- `restore-file` - Restore single files from direct upload backups
- `schedule` - Validate and test cron expressions, show presets
- `test` - Test cloud storage connections
- `config` - Configuration management commands

**User Experience**
- Structured JSON logging with automatic rotation (10MB max, 5 files)
- Secure log sanitization (removes all sensitive data automatically)
- Enhanced error messages with color-coded formatting
- Contextual help and diagnostic commands for common errors
- Actionable troubleshooting suggestions
- Pre-commit hooks to prevent secret leaks

**Cross-Platform**
- Linux support (primary platform)
- Windows support (via platform-specific modules)
- macOS support (via platform-specific modules)

### Recently Completed (v0.1.1)

- ✅ Structured logging system with rotation and sanitization
- ✅ Real-time progress bars for file uploads
- ✅ Enhanced error messages with troubleshooting steps
- ✅ Security incident handling and prevention
- ✅ Pre-commit secret scanning

### Recently Completed

**Restore Functionality (v0.2.0)**
- ✅ Full backup restore with real-time progress tracking
- ✅ Individual file restore from any backup
- ✅ SHA-256 integrity verification for every restored file
- ✅ Backup preview with detailed file listings
- ✅ Conflict detection before restore
- ✅ Automatic decryption and decompression
- ✅ Progress bars for download, decrypt, and verify stages

**Automated Scheduling & Notifications (v0.3.0)**
- ✅ Systemd timer integration for automated backups
- ✅ Flexible scheduling (daily, weekly, hourly, custom)
- ✅ Desktop notifications for backup/restore events (Linux D-Bus)
- ✅ Resource limits to prevent system slowdown
- ✅ Security hardening with systemd sandboxing
- ✅ Persistent timers (catch up missed backups)
- ✅ Easy installation script included

**Backup Retention Policies (v0.4.0)**
- ✅ Configurable retention rules (keep last N, keep by age)
- ✅ GFS (Grandfather-Father-Son) rotation support
- ✅ Automated cleanup of old backups
- ✅ Safety checks (minimum keep threshold)
- ✅ Dry-run mode to preview deletions
- ✅ Interactive confirmation for deletions
- ✅ Detailed deletion summary and statistics

### In Progress (Next Releases)

**High Priority - Automation & Reliability**
- ✅ Resume interrupted uploads (automatic state tracking)
- ✅ Bandwidth throttling and rate limiting
- ✅ Cron expression support for flexible scheduling
- System snapshot capability for full system recovery

**Medium Priority - Enhanced Functionality**
- Real-time file system monitoring and change detection
- Incremental backups (only changed files)
- Backup diff/comparison tools
- Parallel restore for faster recovery
- System tray integration (GUI status indicator)

### Planned

**Storage Backends**
- AWS S3 support
- Google Cloud Storage support
- Azure Blob Storage support
- Local filesystem as backup destination
- Custom storage backend plugin system

**Advanced Features**
- Block-level deduplication across backups
- Backup encryption key rotation
- Multi-destination backup (backup to multiple clouds)
- Backup snapshots and point-in-time recovery
- Differential and incremental backup modes
- Backup retention policies and auto-cleanup

**Security Enhancements**
- Hardware Security Module (HSM) integration
- Yubikey/hardware token support
- Multi-factor authentication
- Backup signing and verification
- Key escrow and recovery mechanisms

**User Interface**
- Desktop GUI (native-windows-gui for Windows, GTK for Linux)
- System tray integration
- Desktop notifications
- Backup history visualization
- Storage usage analytics

**Monitoring & Operations**
- Prometheus metrics export
- Health check endpoints
- Email/webhook notifications
- Backup success/failure alerts
- Detailed logging with rotation

**Performance**
- Memory-mapped file handling for large files
- Zero-copy optimizations
- Parallel compression pipelines
- Resume interrupted uploads
- Smart retry with exponential backoff

## Quick Start

### Installation

```bash
git clone https://github.com/NullMeDev/Skylock.git
cd Skylock
cargo build --release --workspace
```

The binary will be at `target/release/skylock`.

### Configuration

Copy the sample configuration and edit with your credentials:

```bash
mkdir -p ~/.config/skylock-hybrid
cp config.sample.toml ~/.config/skylock-hybrid/config.toml
```

Edit `~/.config/skylock-hybrid/config.toml` with your Hetzner Storage Box credentials and backup paths.

### Basic Usage

```bash
# Create a backup
skylock backup --direct /path/to/backup

# Create a backup with bandwidth limit (1.5 MB/s)
skylock backup --direct --max-speed 1.5M /path/to/backup

# List backups
skylock list

# Restore a backup
skylock restore <backup_id> --target /path/to/restore

# Test cron schedule expressions
skylock schedule "0 0 2 * * *"     # Validate and show next runs
skylock schedule --presets         # Show common presets

# Test connection
skylock test hetzner
```

## Architecture

Skylock is organized as a Rust workspace with modular crates:

- **skylock-core**: Core functionality, encryption, compression, storage abstractions
- **skylock-backup**: Backup engine with direct upload and archive modes
- **skylock-hetzner**: Hetzner Storage Box integration (WebDAV/SFTP)
- **skylock-monitor**: System monitoring and health checks
- **skylock-sync**: File synchronization engine
- **skylock-ui**: User interface components

## Security

- **Encryption**: AES-256-GCM with authenticated encryption
- **Key Derivation**: Argon2id for password-based key derivation
- **Transport Security**: TLS 1.3 for all network communications
- **Zero-Knowledge**: All encryption happens client-side before upload

See [SECURITY.md](SECURITY.md) for detailed security information and setup guide.

## Development

### Requirements

- Rust 1.70 or higher
- Cargo

### Building

```bash
# Development build
cargo build --workspace

# Release build with optimizations
cargo build --release --workspace

# Run tests
cargo test --workspace

# Format code
cargo fmt --all

# Run linter
cargo clippy --workspace --all-targets
```

### Project Structure

```
skylock/
├── skylock-core/        # Core functionality
├── skylock-backup/      # Backup implementation
├── skylock-hetzner/     # Cloud storage integration
├── skylock-monitor/     # Monitoring
├── skylock-sync/        # File synchronization
├── skylock-ui/          # UI components
└── src/                 # Main application
```

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

To contribute:

1. Fork the repository
2. Create a feature branch
3. Make your changes with clear commit messages
4. Ensure tests pass and code is formatted
5. Submit a pull request

### Bug Reports

Found a bug? Please [open an issue](https://github.com/NullMeDev/Skylock/issues/new?template=bug_report.md) with details.

### Feature Requests

Have an idea? Please [open an issue](https://github.com/NullMeDev/Skylock/issues/new?template=feature_request.md) describing your proposal.

## Documentation

- [CONTRIBUTING.md](CONTRIBUTING.md) - Contributing guidelines
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) - Code of conduct
- [SECURITY.md](SECURITY.md) - Security guide and best practices
- [SECURITY_AUDIT.md](SECURITY_AUDIT.md) - Security audit details
- [USAGE.md](USAGE.md) - Detailed usage guide
- [RESTORE_GUIDE.md](RESTORE_GUIDE.md) - Complete restore and recovery guide
- [SCHEDULING_GUIDE.md](SCHEDULING_GUIDE.md) - Automated scheduling and notifications guide

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.

## Contact

**Maintainer:** null@nullme.lol

For questions, bugs, or feature requests, please use [GitHub Issues](https://github.com/NullMeDev/Skylock/issues).

---

Built with Rust
