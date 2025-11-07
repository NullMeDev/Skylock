# Changelog

All notable changes to Skylock will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2025-11-07

### Added
- **Backup Retention Policies**: Configurable retention rules with multiple strategies
  - Keep last N backups (default: 30)
  - Keep backups by age (configurable days)
  - GFS (Grandfather-Father-Son) rotation support
  - Minimum keep safety threshold (always keep at least 3 backups)
- **Cleanup Command**: `skylock cleanup` for automated backup deletion
  - Dry-run mode (`--dry-run`) to preview deletions
  - Interactive confirmation for safety
  - Force mode (`--force`) for automated cleanup
  - Detailed deletion statistics and summaries
- **Retention Module**: Complete retention management system
  - Multiple retention strategies can be combined
  - Safety checks prevent accidental data loss
  - Comprehensive unit tests

### Changed
- Updated version to 0.4.0
- Enhanced README with retention policy documentation
- Updated SCHEDULING_GUIDE with cleanup instructions

### Fixed
- Added Clone derive to BackupManifest for retention calculations
- Fixed chrono trait imports (Datelike, Timelike, IsoWeek)

## [0.3.0] - 2025-11-07

### Added
- **Systemd Timer Integration**: Automated backup scheduling for Linux
  - User-level systemd service and timer files
  - Default schedule: Daily at 2:00 AM with randomization
  - Persistent timers (catch up missed backups)
  - Resource limits (50% CPU, 2GB RAM, 20 tasks)
  - Security hardening with systemd sandboxing
  - Installation script: `scripts/install-timer.sh`
- **Desktop Notifications**: D-Bus notifications on Linux
  - Backup started/completed/failed notifications
  - Restore started/completed/failed notifications
  - Visual urgency levels (normal/critical)
  - Configurable timeouts
  - System icons and sound support
- **Scheduling Guide**: Comprehensive documentation (SCHEDULING_GUIDE.md)
  - Systemd timer configuration
  - Custom schedules (hourly, daily, weekly, monthly)
  - Monitoring and logging
  - Troubleshooting section

### Changed
- Updated README with scheduling and notification features
- Added notifications module (`src/notifications.rs`)

## [0.2.0] - 2025-11-07

### Added
- **Full Backup Restore**: Complete restore functionality with progress tracking
  - Real-time progress bars (overall + per-file)
  - Download, decrypt, and decompress stages
  - SHA-256 integrity verification for every file
  - Automatic directory structure preservation
- **Backup Preview**: Preview backup contents before restoring
  - Shows all files organized by directory
  - Displays sizes, timestamps, compression status
  - Encryption status indicators
- **Conflict Detection**: Check for existing files before restore
  - Lists all potential conflicts
  - Shows what will be overwritten
  - Suggests solutions
- **Individual File Restore**: Restore single files from backups
  - Download only the requested file
  - Full integrity verification
  - Efficient for quick recovery
- **Restore Guide**: Complete documentation (RESTORE_GUIDE.md)
  - Quick start guide
  - Detailed usage examples
  - Troubleshooting section
  - Security notes

### Changed
- Enhanced DirectUploadBackup with restore methods
- Added preview and restore commands to CLI
- Updated README with restore features

### Fixed
- Integrity verification using SHA-256 hashes
- Progress bar display for restore operations

## [0.1.1] - 2025-11-06

### Added
- **Structured Logging**: JSON logging with automatic rotation
  - 10MB max file size, 5 file rotation
  - Secure log sanitization (removes sensitive data)
  - Color-coded console output
- **Progress Bars**: Real-time upload progress
  - Overall progress (files completed)
  - Individual file progress (upload speed, ETA)
  - Adaptive display based on TTY detection
- **Enhanced Error Messages**: User-friendly error handling
  - Color-coded error/warning/success messages
  - Contextual troubleshooting suggestions
  - Actionable help for common issues
- **Security Incident Handling**: Pre-commit hooks
  - Automatic secret scanning
  - Prevents accidental credential leaks
  - Git history protection

### Changed
- Improved user experience with better CLI output
- Added progress module with ProgressReporter and ErrorHandler
- Enhanced backup command with status indicators

## [0.1.0] - 2025-10-25 (Initial Release)

### Added
- **Core Security**
  - AES-256-GCM client-side encryption
  - Argon2id key derivation
  - Per-file encryption with unique nonces
  - TLS 1.3 transport security
  - Ed25519 SSH key authentication (SFTP)
- **Backup Operations**
  - Direct upload mode (per-file streaming)
  - Archive mode (tar.zst.enc)
  - File-level deduplication
  - Backup manifest system
  - Adaptive concurrency control
- **Storage Integration**
  - Hetzner Storage Box via WebDAV (HTTPS)
  - Hetzner Storage Box via SFTP (SSH)
  - Automatic directory creation
  - Connection testing
- **Compression**
  - Zstd compression (level 3) for files >10MB
  - Smart compression (only when beneficial)
  - Streaming compression
- **CLI Interface**
  - `backup` - Create backups
  - `list` - List all backups
  - `test` - Test connections
  - `config` - Configuration management
- **Cross-Platform Support**
  - Linux (primary)
  - Windows (via platform modules)
  - macOS (via platform modules)

### Documentation
- README.md - Main documentation
- USAGE.md - Detailed usage guide
- SECURITY.md - Security guide and best practices
- SECURITY_AUDIT.md - Security audit details
- CONTRIBUTING.md - Contributing guidelines
- CODE_OF_CONDUCT.md - Code of conduct

## [Unreleased]

### Added
- **Resume Interrupted Uploads**: Automatic resume capability for interrupted backups
  - State tracking saves progress after each successful file upload
  - Automatic detection of interrupted backups on restart
  - Skip already-uploaded files when resuming
  - Progress bars show resumed upload status
  - Atomic state file updates prevent corruption
  - Automatic cleanup of old state files (>7 days)
  - Zero configuration required - works automatically
- **Bandwidth Throttling**: Upload speed limiting to prevent network saturation
  - Token bucket algorithm for smooth rate limiting
  - Configurable via CLI flag (`--max-speed`) or config file
  - Supports human-readable formats: "1.5M", "500K", "1024"
  - Per-upload bandwidth control
  - No throttling on restores (maximum speed for recovery)
  - CLI example: `skylock backup --direct --max-speed 1.5M ~/Documents`
- **Cron Expression Support**: Advanced scheduling with cron expressions
  - 6-field cron format: second minute hour day month weekday
  - `skylock schedule` command for validation and testing
  - Built-in presets: HOURLY, DAILY_2AM, WEEKLY_SUNDAY, MONTHLY_1ST, etc.
  - Human-readable schedule descriptions
  - Shows next 5 scheduled runs with relative time
  - Enhanced `should_run_backup()` with last_run tracking
  - Full validation and error messages
  - CLI examples:
    - `skylock schedule "0 0 2 * * *"` - Validate and show next runs
    - `skylock schedule --presets` - Show common presets
- **Backup Diff/Comparison Tools**: Compare backups and identify changes
  - `skylock diff <backup_id1> <backup_id2>` command
  - Intelligently detects files added, removed, modified, and moved/renamed
  - Move detection via hash comparison (same content, different path)
  - Detailed summary with file counts and size changes
  - Color-coded output (green = added, red = removed, yellow = modified, cyan = moved)
  - Supports filtering by change type (`--filter added,removed,modified,moved`)
  - Detailed mode (`--detailed`) shows individual file paths and size deltas
  - Net size change calculation
  - 7 comprehensive unit tests covering all diff scenarios
  - CLI examples:
    - `skylock diff backup_20251107_120000 backup_20251107_140000` - Summary
    - `skylock diff <old> <new> --detailed` - Show all files
    - `skylock diff <old> <new> --filter added,modified` - Only show specific changes

### Planned
- System tray integration (GUI)
- System snapshot capability
- Real-time file system monitoring
- Incremental backups
- Parallel restore
- AWS S3 support
- Google Cloud Storage support
- Azure Blob Storage support
- Block-level deduplication
- Hardware Security Module (HSM) integration
- Yubikey support

---

## Version Numbering

Skylock follows [Semantic Versioning](https://semver.org/):
- **Major (X.0.0)**: Breaking changes, major feature overhauls
- **Minor (0.X.0)**: New features, significant improvements (backwards compatible)
- **Patch (0.0.X)**: Bug fixes, minor improvements (backwards compatible)

Current version: **0.4.0**
