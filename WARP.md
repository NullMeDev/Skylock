# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Project Overview

**Skylock** is a secure, encrypted backup system written in Rust that provides client-side AES-256-GCM encryption with Argon2id key derivation. All data is encrypted before upload, ensuring zero-knowledge cloud storage. Currently supports Hetzner Storage Box via WebDAV (HTTPS) and SFTP (SSH with Ed25519 keys).

**Version**: 0.4.0  
**Rust MSRV**: 1.70+  
**Primary Platform**: Linux (with Windows/macOS support)  
**Contact**: null@nullme.lol

## Common Development Commands

### Build
```bash
# Development build (all workspace crates)
cargo build --workspace

# Release build with optimizations
cargo build --release --workspace

# Build specific crate
cargo build -p skylock-core
cargo build -p skylock-backup
```

### Testing
```bash
# Run all tests
cargo test --workspace

# Run tests with output visible
cargo test --workspace -- --nocapture

# Run specific test by name
cargo test -p skylock-hybrid test_end_to_end_backup_workflow -- --nocapture

# Run platform-specific tests
cargo test --package skylock-hybrid --test unix_platform_tests
cargo test --package skylock-hybrid --test windows_platform_tests

# Run extended tests (including ignored tests)
cargo test --all-features --workspace -- --include-ignored

# Run thread safety tests sequentially
cargo test --all-features --workspace -- --test-threads=1 thread_safety_tests
```

### Linting and Formatting
```bash
# Format all code
cargo fmt --all

# Check formatting without modifying
cargo fmt --all -- --check

# Run clippy linter (CI uses strict mode)
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Run clippy on specific crate
cargo clippy -p skylock-core --all-targets -- -D warnings
```

### Benchmarks
```bash
# Run all benchmarks
cargo bench --workspace

# Run criterion benchmarks with verbose output
cargo criterion --output-format verbose

# Run specific benchmark
cargo bench -p skylock-hybrid -- backup_performance
```

### Documentation
```bash
# Build documentation for all crates
cargo doc --no-deps --all-features

# Open documentation in browser
cargo doc --no-deps --all-features --open
```

### Running the Binary
```bash
# Run from workspace root
cargo run --bin skylock -- --help

# Run with release optimizations
cargo run --release --bin skylock -- backup --direct ~/Documents

# Run test binaries
cargo run --bin test_hetzner
cargo run --bin simple_webdav_test
```

## Architecture Overview

Skylock is organized as a **Rust workspace** with 6 modular crates:

### Crate Structure and Responsibilities

1. **skylock-core** (`./skylock-core/`)
   - Core abstractions and primitives
   - Modules: `encryption`, `compression`, `storage`, `security`, `scheduler`, `sync`, `error_types`
   - Key types: `Config`, `EncryptionEngine`, `SecureKey`, `KeyManager`, `CompressionEngine`
   - Feature flags: `hetzner-storage`, `aws-storage`, `azure-storage`, `gcp-storage`, `backblaze-storage`
   - Platform features: `windows-vss`, `unix-lvm`, `unix-zfs`, `hetzner-api`

2. **skylock-backup** (`./skylock-backup/`)
   - Backup implementation with direct upload and archive modes
   - Modules: `direct_upload`, `encryption`, `retention`, `vss` (Windows VSS snapshots)
   - Key types: `BackupManager`, `DirectUploadBackup`, `BackupManifest`, `RetentionPolicy`, `GfsPolicy`
   - Direct upload: Per-file streaming with parallel uploads (4 threads default)
   - Archive mode: tar.zst.enc compressed archives (legacy)

3. **skylock-hetzner** (`./skylock-hetzner/`)
   - Hetzner Storage Box integration
   - Protocols: WebDAV (HTTPS) and SFTP (SSH with Ed25519)
   - Key type: `HetznerClient`
   - Dependencies: `reqwest` (WebDAV), `ssh2`/`async-ssh2` (SFTP)

4. **skylock-ui** (`./skylock-ui/`)
   - User interface components and progress display
   - Desktop notifications via D-Bus (Linux)

5. **skylock-monitor** (`./skylock-monitor/`)
   - System monitoring and health checks

6. **skylock-sync** (`./skylock-sync/`)
   - File synchronization engine

### Main Application (`./src/`)

The main binary is in `src/main.rs` with CLI commands:
- **backup**: Create backups (direct or archive mode)
- **restore**: Restore entire backups or specific files
- **restore-file**: Restore single files from backups
- **preview**: Preview backup contents before restoring
- **list**: List available backups with filtering
- **test**: Test Hetzner connection and components
- **config**: Generate default configuration
- **cleanup**: Clean up old backups based on retention policy

Key modules in `src/`:
- `cli/`: Command-line interface
- `crypto/`: Encryption/decryption
- `compression/`: Zstd compression (level 3 for files >10MB)
- `deduplication/`: File-level deduplication
- `restore/`: Restore operations with integrity verification
- `platform/`: Platform-specific abstractions (Windows VSS, Unix stubs)
- `error_handler/`: Enhanced error display with troubleshooting
- `monitoring/`: System monitoring
- `notifications.rs`: Desktop notifications
- `logging.rs`: Structured JSON logging with automatic rotation (10MB max, 5 files)
- `progress.rs`: Real-time progress bars

### Data Flow

```
User Input (CLI)
    ↓
Configuration Load (~/.config/skylock-hybrid/config.toml)
    ↓
BackupManager / DirectUploadBackup
    ↓
Encryption (AES-256-GCM per-file) + Compression (Zstd for >10MB)
    ↓
HetznerClient (WebDAV/SFTP)
    ↓
Hetzner Storage Box
```

### Key Architectural Patterns

- **Async/await**: All I/O operations use Tokio runtime
- **Trait-based backends**: Storage abstraction allows multiple cloud providers
- **Streaming pipeline**: Files are encrypted/compressed in streaming fashion (memory efficient)
- **Parallel uploads**: Adaptive concurrency control (default 4 threads)
- **Error handling**: Comprehensive error types with recovery suggestions
- **Security by default**: All encryption happens client-side before upload

## Direct Upload vs Archive Mode

### Direct Upload Mode (`--direct` flag) ✅ **Recommended**
- **Per-file streaming**: Each file encrypted individually with unique nonce
- **Parallel uploads**: 4 concurrent uploads by default
- **No temp files**: Direct streaming to cloud, minimal disk usage
- **Individual file restore**: Can restore single files without downloading entire backup
- **Smart compression**: Only files >10MB are compressed (Zstd level 3)
- **Manifest**: JSON manifest tracks all files with metadata and SHA-256 hashes
- **Backup ID format**: `backup_YYYYMMDD_HHMMSS`
- **Use case**: Daily backups, selective restore, large datasets

### Archive Mode (no `--direct` flag) ⚠️ **Legacy**
- **Single tar archive**: Creates tar.zst.enc compressed archive locally first
- **Better compression**: Single-pass compression more efficient
- **Disk space required**: Needs local temp space for entire backup
- **Full restore only**: Must download entire archive to restore anything
- **Slower**: Sequential operation, no parallelization
- **Use case**: One-time large backups where compression ratio is critical

**Default**: Archive mode (for backward compatibility). Always prefer `--direct` for production use.

**Example**:
```bash
# Direct upload (recommended)
skylock backup --direct ~/Documents ~/Pictures

# Archive mode (legacy)
skylock backup ~/Documents
```

## Testing Infrastructure

### Unit Tests
Located in each crate's `src/` directory (inline tests) and workspace `tests/` directory.

```bash
# Run all unit tests
cargo test --workspace

# Run tests for specific crate
cargo test -p skylock-core
cargo test -p skylock-backup
```

### Integration Tests

Located in `tests/` directory:
- `integration_tests.rs`: End-to-end backup workflow tests
- `thread_safety_tests.rs`: Concurrent access tests (run with `--test-threads=1`)
- `error_handling_tests.rs`: Error handling and recovery tests
- `unix_platform_tests.rs`: Unix-specific platform tests
- `windows_platform_tests.rs`: Windows-specific platform tests (VSS, etc.)

```bash
# Run integration tests
cargo test --test integration_tests

# Run platform-specific tests
cargo test --test unix_platform_tests  # Linux/macOS
cargo test --test windows_platform_tests  # Windows only

# Run thread safety tests (sequential execution required)
cargo test --test thread_safety_tests -- --test-threads=1
```

### Benchmarks

Located in `benches/performance_benchmarks.rs` using Criterion framework.

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark suite
cargo bench -- backup_performance
cargo bench -- encryption_performance
cargo bench -- compression_performance

# Generate HTML reports
cargo bench
# Reports in: target/criterion/*/report/index.html
```

### Test Configuration

Tests may need:
- **Hetzner credentials**: Set in test config or skip with `cargo test -- --skip hetzner`
- **Temp directories**: Tests use `tempfile` crate for isolation
- **Network access**: Some integration tests require network (marked with `#[ignore]`)

```bash
# Run ignored tests (requires network/credentials)
cargo test -- --ignored

# Skip specific tests
cargo test -- --skip test_hetzner_connection
```

## Security Considerations

### Encryption Details

**Algorithm**: AES-256-GCM (Authenticated Encryption with Associated Data)
- Library: `aes-gcm` crate version 0.10
- Mode: Galois/Counter Mode with 256-bit keys
- Authentication: AEAD provides integrity verification

**Key Derivation**: Argon2id
- Password-based key derivation with configurable parameters
- Designed to resist GPU/ASIC attacks

**Per-File Encryption**:
- Each file gets a unique nonce (never reused)
- Files >10MB compressed with Zstd (level 3) before encryption
- SHA-256 hash computed for integrity verification

### Configuration Security

**Encryption Key Setup**:
```bash
# Generate a secure 256-bit encryption key
openssl rand -base64 32
```

Add to `~/.config/skylock-hybrid/config.toml`:
```toml
[hetzner]
encryption_key = "YOUR_GENERATED_KEY_HERE"
```

**Protect Configuration**:
```bash
chmod 600 ~/.config/skylock-hybrid/config.toml
```

### Transport Security

- **WebDAV**: TLS 1.3 with certificate validation
- **SFTP**: SSH with Ed25519 key authentication (recommended)

**Generate Ed25519 SSH key for SFTP**:
```bash
ssh-keygen -t ed25519 -f ~/.ssh/id_ed25519_hetzner -C "hetzner-storagebox"
# Upload public key to Hetzner Storage Box
```

### Key Management Best Practices

1. **Store keys securely**: Use password manager or secure vault
2. **Backup keys safely**: Without keys, encrypted data cannot be recovered
3. **Never commit keys**: Ensure `.gitignore` excludes config files
4. **Rotate keys periodically**: Consider key rotation for long-term backups

### Threat Model

**Protects against**:
- Storage provider compromise (data encrypted before upload)
- Network interception (TLS/SSH encrypts in transit)
- Unauthorized access (strong encryption and authentication)
- Data tampering (AEAD integrity verification)

**Does NOT protect against**:
- Compromised client machine (keys may be exposed)
- Weak encryption keys (use strong, random keys)
- Lost keys (data is unrecoverable without keys)

### Security Audit

See `SECURITY_AUDIT.md` for details on pre-release security audit.

**Report vulnerabilities**: null@nullme.lol (do not open public issues)

## Configuration

### Configuration Files

**Default location**: `~/.config/skylock-hybrid/config.toml`

**Data directory**: `~/.local/share/skylock/`
**Log directory**: `~/.local/share/skylock/logs/`

### Configuration Structure

```toml
[syncthing]
api_key = "your-syncthing-api-key"
api_url = "http://localhost:8384"
folders = ["/path/to/sync1", "/path/to/sync2"]

[hetzner]
endpoint = "https://your-username.your-storagebox.de"
username = "your-username"
password = "your-password"
encryption_key = "base64-encoded-256-bit-key"
# Optional: Use SFTP instead of WebDAV
# protocol = "sftp"
# port = 23

[backup]
vss_enabled = false  # Windows only
schedule = "0 2 * * *"  # Cron format
retention_days = 30
backup_paths = [
    "/home/user/Documents",
    "/home/user/.ssh",
    "/home/user/.config"
]

[ui]
always_prompt_deletions = true
notification_enabled = true
```

### Generate Configuration

```bash
# Generate sample config
skylock config --output ~/.config/skylock-hybrid/config.toml

# Copy sample config
mkdir -p ~/.config/skylock-hybrid
cp config.sample.toml ~/.config/skylock-hybrid/config.toml
```

### Configuration Precedence

1. **CLI flags**: `--config /path/to/config.toml`
2. **Environment variables**: `SKYLOCK_CONFIG_PATH`
3. **Default location**: `~/.config/skylock-hybrid/config.toml`
4. **Fallback**: `./config.toml` (current directory)

## Automated Scheduling (Linux)

### Systemd Timer Setup

Skylock includes systemd user units for automated backups.

**Installation**:
```bash
# Install timer using provided script
./scripts/install-timer.sh

# Or manually
mkdir -p ~/.config/systemd/user
cp systemd/skylock-backup.service ~/.config/systemd/user/
cp systemd/skylock-backup.timer ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now skylock-backup.timer
```

### Systemd Service Configuration

**Service** (`systemd/skylock-backup.service`):
- Runs: `skylock backup --direct` (uses config paths from config.toml)
- Resource limits: CPU 50%, Memory 2GB max
- Security: Private /tmp, no new privileges, protected system directories
- Restart: On failure only

**Timer** (`systemd/skylock-backup.timer`):
- Schedule: Daily at 2:00 AM
- Randomization: ±10 minutes (avoid thundering herd)
- Catch-up: Runs missed backups within 15 minutes after boot
- Persistent: Yes

### Managing the Timer

```bash
# Check timer status
systemctl --user status skylock-backup.timer

# See next scheduled run
systemctl --user list-timers

# Start backup manually
systemctl --user start skylock-backup.service

# Check backup logs
journalctl --user -u skylock-backup.service -e

# Stop timer
systemctl --user stop skylock-backup.timer

# Disable timer
systemctl --user disable skylock-backup.timer
```

### Custom Schedule

Edit `~/.config/systemd/user/skylock-backup.timer`:

```ini
[Timer]
# Run at 3:30 AM daily
OnCalendar=*-*-* 03:30:00

# Run twice daily (6 AM and 6 PM)
OnCalendar=*-*-* 06:00:00
OnCalendar=*-*-* 18:00:00

# Run hourly
OnCalendar=hourly

# Run on weekdays at 8 PM
OnCalendar=Mon-Fri *-*-* 20:00:00
```

After editing:
```bash
systemctl --user daemon-reload
systemctl --user restart skylock-backup.timer
```

See `SCHEDULING_GUIDE.md` for complete documentation.

## Desktop Notifications

Skylock sends desktop notifications (Linux via D-Bus) for:

- **Backup Started**: Shows paths being backed up
- **Backup Completed**: Shows file count, size, duration
- **Backup Failed**: Critical urgency with error message
- **Restore Started**: Shows backup ID
- **Restore Completed**: Shows file count and duration
- **Restore Failed**: Critical urgency with error message

Notifications use system default icons and sounds.

## Retention Policies

Configure automatic cleanup of old backups:

```toml
[backup.retention]
# Keep last N backups
keep_last = 7

# Keep backups newer than N days
keep_days = 30

# GFS (Grandfather-Father-Son) rotation
gfs_enabled = true
gfs_daily = 7     # Keep 7 daily backups
gfs_weekly = 4    # Keep 4 weekly backups
gfs_monthly = 12  # Keep 12 monthly backups
```

**Manual cleanup**:
```bash
# Dry run (show what would be deleted)
skylock cleanup --dry-run

# Interactive cleanup
skylock cleanup

# Force cleanup without confirmation
skylock cleanup --force
```

See `RESTORE_GUIDE.md` for restore operations and integrity verification details.

## CI/CD

### GitHub Actions Workflow

Located in `.github/workflows/ci.yml`:

**Test Matrix**:
- Platforms: Ubuntu, Windows, macOS
- Rust versions: stable, beta, 1.70.0 (MSRV)

**CI Steps**:
1. Format check: `cargo fmt --all -- --check`
2. Lint: `cargo clippy --all-targets --all-features -- -D warnings`
3. Unit tests: `cargo test --verbose --all -- --nocapture`
4. Extended tests: `cargo test --all-features --workspace -- --include-ignored`
5. Thread safety: `cargo test --all-features --workspace -- --test-threads=1 thread_safety_tests`
6. Platform tests: Unix/Windows specific tests
7. Benchmarks: Criterion performance tests (Ubuntu stable only)
8. Security audit: `cargo audit`, `cargo deny`, `cargo outdated`
9. Documentation: `cargo doc --no-deps --all-features`

**Coverage** (Ubuntu stable only):
- Tool: `cargo-tarpaulin`
- Minimum: 80% coverage required
- Output: XML and HTML reports

## Contributing Guidelines

From `CONTRIBUTING.md`:

1. **Fork and branch**: Create feature branch from `main`
2. **Format**: Run `cargo fmt --all`
3. **Lint**: Run `cargo clippy --workspace --all-targets` (must pass with no warnings)
4. **Test**: Run `cargo test --workspace` (all tests must pass)
5. **Commit messages**: Clear, descriptive messages
6. **Pull request**: Include description of changes and testing performed

**Code Standards**:
- Follow Rust idioms and best practices
- Write self-documenting code
- Add tests for new functionality
- Update documentation as needed
- Keep commits focused and atomic

## Development Tips

### Large Backup Testing

For testing large backups without system impact:

```bash
# Set custom temp directory
export SKYLOCK_TEMP_DIR=/mnt/fast-ssd/skylock-temp

# Run backup with verbose logging
RUST_LOG=info cargo run --release --bin skylock -- backup --direct ~/large-dataset
```

### Debugging

```bash
# Enable debug logging
RUST_LOG=debug cargo run --bin skylock -- backup --direct ~/test

# Trace-level logging (very verbose)
RUST_LOG=trace cargo run --bin skylock -- backup --direct ~/test

# Check logs
tail -f ~/.local/share/skylock/logs/skylock-*.log
```

### Performance Profiling

```bash
# Build with profiling symbols
cargo build --release --bin skylock

# Run with perf (Linux)
perf record --call-graph=dwarf target/release/skylock backup --direct ~/test
perf report

# Flamegraph
cargo install flamegraph
cargo flamegraph --bin skylock -- backup --direct ~/test
```

### Memory Profiling

```bash
# Valgrind (Linux)
valgrind --leak-check=full target/release/skylock backup --direct ~/test

# Heaptrack (Linux)
heaptrack target/release/skylock backup --direct ~/test
```

## Quick Reference

```bash
# Common operations
skylock backup --direct ~/Documents ~/Pictures     # Create backup
skylock list                                        # List all backups
skylock restore backup_20240101_020000 --target ~/ # Restore backup
skylock restore-file <backup_id> <path> -o output  # Restore single file
skylock cleanup --dry-run                           # Preview old backups
skylock test hetzner                                # Test connection

# Development
cargo build --workspace                             # Build all crates
cargo test --workspace                              # Run all tests
cargo fmt --all                                     # Format code
cargo clippy --workspace --all-targets              # Lint code
cargo bench                                         # Run benchmarks

# Systemd automation
systemctl --user status skylock-backup.timer        # Check schedule
systemctl --user start skylock-backup.service       # Manual backup
journalctl --user -u skylock-backup.service -e      # View logs
```

## Additional Documentation

- `README.md`: Project overview and features
- `SECURITY.md`: Security architecture and setup
- `SECURITY_AUDIT.md`: Pre-release security audit details
- `USAGE.md`: User guide with examples
- `RESTORE_GUIDE.md`: Complete restore and recovery guide
- `SCHEDULING_GUIDE.md`: Automated scheduling and notifications
- `CONTRIBUTING.md`: Contributing guidelines
- `CODE_OF_CONDUCT.md`: Community code of conduct
- `CHANGELOG.md`: Version history and changes
