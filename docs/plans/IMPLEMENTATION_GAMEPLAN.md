# Skylock Implementation Gameplan

**Created**: 2025-11-27  
**Target**: v1.0.0 Release  
**Timeline**: 22-32 weeks total

---

## Phase 1: Core Competitive Features (4-6 weeks)

### 1.1 Multi-Provider Storage Support (Week 1-2)

**Goal**: Complete AWS S3 and Backblaze B2 providers; improve local storage

**Tasks**:
```
□ 1.1.1 Complete AWS S3 provider implementation
  - File: skylock-core/src/storage/providers/aws.rs (exists, needs completion)
  - Add multipart upload for large files
  - Implement lifecycle policy integration
  - Add server-side encryption (SSE-S3, SSE-KMS)
  - Tests: upload/download/list/delete operations
  
□ 1.1.2 Complete Backblaze B2 provider
  - File: skylock-core/src/storage/providers/backblaze.rs (stub exists)
  - Implement B2 native API (not S3 compat for better performance)
  - Add large file upload (chunked)
  - Implement file versioning support
  
□ 1.1.3 Enhance local storage provider
  - File: skylock-core/src/storage/providers/local.rs
  - Add hardlink deduplication
  - Implement atomic writes with temp files
  - Add disk space monitoring
  
□ 1.1.4 Create unified storage abstraction
  - File: skylock-core/src/storage/unified.rs (new)
  - Provider-agnostic interface
  - Automatic retry with exponential backoff
  - Connection health monitoring
  
□ 1.1.5 Add storage provider CLI commands
  - `skylock provider list` - list configured providers
  - `skylock provider test <name>` - test connectivity
  - `skylock provider add <type>` - interactive setup
```

**Dependencies**: reqwest, aws-sdk-s3, backblaze-b2 crate
**Deliverable**: Backup to S3/B2/local with single config change

---

### 1.2 Real-Time File Sync (Week 2-3)

**Goal**: Watch directories and upload changes within seconds

**Tasks**:
```
□ 1.2.1 Implement file watcher daemon
  - File: skylock-backup/src/watcher.rs (new)
  - Use notify crate with debouncing (500ms default)
  - Handle inotify limits (increase via sysctl suggestion)
  - Recursive directory watching
  - Ignore patterns (.git, node_modules, etc.)
  
□ 1.2.2 Create change queue processor
  - File: skylock-backup/src/sync_queue.rs (new)
  - Priority queue (recent changes first)
  - Batch similar changes
  - Conflict detection for rapid changes
  
□ 1.2.3 Add continuous backup mode
  - File: skylock-backup/src/continuous.rs (new)
  - `skylock watch <paths>` command
  - Background daemon with PID file
  - Graceful shutdown handling
  
□ 1.2.4 Implement sync state tracking
  - File: skylock-backup/src/sync_state.rs (new)
  - SQLite database for file states
  - Last modified time + size + hash
  - Detect moves via inode tracking
  
□ 1.2.5 Add systemd service for daemon
  - File: systemd/skylock-watch.service (new)
  - Socket activation optional
  - Watchdog integration
```

**Dependencies**: notify 6.x, rusqlite
**Deliverable**: `skylock watch ~/Documents` syncs in real-time

---

### 1.3 Secure File Sharing (Week 3-4)

**Goal**: Generate password-protected download links

**Tasks**:
```
□ 1.3.1 Design sharing data model
  - File: skylock-backup/src/sharing/mod.rs (new)
  - ShareLink struct: id, file_path, password_hash, expiry, download_limit, created_by
  - ShareAccess log: link_id, timestamp, ip_hash, success
  
□ 1.3.2 Implement link generation
  - File: skylock-backup/src/sharing/links.rs (new)
  - Cryptographically random link IDs (128-bit)
  - Argon2id password hashing
  - Optional expiry (1h, 24h, 7d, 30d, never)
  - Download count limit
  
□ 1.3.3 Create share server
  - File: src/share_server.rs (new)
  - Axum-based HTTP server
  - Rate limiting per IP
  - Password verification endpoint
  - Streaming file download
  
□ 1.3.4 Add CLI commands
  - `skylock share <backup_id> <path>` - create link
  - `skylock share --password --expires 7d` - with options
  - `skylock shares list` - list active shares
  - `skylock shares revoke <link_id>` - revoke link
  
□ 1.3.5 Implement upload requests
  - Allow recipients to upload files to your backup
  - Dedicated upload folder per request
  - Size and type restrictions
```

**Dependencies**: axum, tower (rate limiting)
**Deliverable**: `skylock share backup_123 /docs/report.pdf --password`

---

### 1.4 Ransomware Detection & Protection (Week 4-5)

**Goal**: Detect encryption attacks and protect backups

**Tasks**:
```
□ 1.4.1 Implement entropy analyzer
  - File: skylock-backup/src/ransomware/entropy.rs (new)
  - Shannon entropy calculation
  - Flag files >7.5 entropy (likely encrypted)
  - Track entropy changes over time
  
□ 1.4.2 Create behavior detector
  - File: skylock-backup/src/ransomware/behavior.rs (new)
  - Detect mass file modifications (>100 files/minute)
  - Detect extension changes (.docx -> .encrypted)
  - Detect ransom note patterns (README.txt with keywords)
  
□ 1.4.3 Add immutable backup mode
  - File: skylock-backup/src/ransomware/immutable.rs (new)
  - Object lock for S3 (compliance mode)
  - Append-only local backups
  - Configurable retention lock (7-365 days)
  
□ 1.4.4 Implement alert system
  - File: skylock-backup/src/ransomware/alerts.rs (new)
  - Desktop notification on detection
  - Optional webhook/email alerts
  - Automatic backup pause on high-risk
  
□ 1.4.5 Add snapshot isolation
  - Keep last N clean snapshots
  - Automatic rollback suggestion
  - Quarantine suspicious changes
```

**Dependencies**: None (pure Rust)
**Deliverable**: Automatic alerts when ransomware detected

---

### 1.5 Phase 1 Integration & Testing (Week 5-6)

**Tasks**:
```
□ 1.5.1 Integration tests for all providers
□ 1.5.2 Real-time sync stress tests (10k files)
□ 1.5.3 Sharing security audit
□ 1.5.4 Ransomware detection accuracy tests
□ 1.5.5 Documentation updates
□ 1.5.6 Performance benchmarks
□ 1.5.7 Release v0.7.0
```

---

## Phase 2: User Experience (6-8 weeks)

### 2.1 Desktop GUI Foundation (Week 7-9)

**Goal**: Native cross-platform GUI with egui

**Tasks**:
```
□ 2.1.1 Set up GUI crate
  - File: skylock-gui/Cargo.toml (new crate)
  - Dependencies: eframe, egui, egui-notify
  - Cross-platform build setup
  
□ 2.1.2 Implement main window
  - File: skylock-gui/src/app.rs
  - Tab navigation (Dashboard, Backups, Restore, Settings)
  - Dark/light theme support
  - Responsive layout
  
□ 2.1.3 Create dashboard view
  - File: skylock-gui/src/views/dashboard.rs
  - Backup status cards
  - Storage usage chart
  - Recent activity list
  - Quick action buttons
  
□ 2.1.4 Implement backup view
  - File: skylock-gui/src/views/backups.rs
  - Backup list with search/filter
  - Create backup wizard
  - Progress indicators
  - Schedule management
  
□ 2.1.5 Create restore view
  - File: skylock-gui/src/views/restore.rs
  - File browser tree
  - Preview pane
  - Batch selection
  - Restore options dialog
```

**Dependencies**: eframe 0.28+, egui-extras
**Deliverable**: Basic functional GUI

---

### 2.2 System Tray Integration (Week 9-10)

**Goal**: Background operation with tray icon

**Tasks**:
```
□ 2.2.1 Implement tray icon
  - File: skylock-gui/src/tray.rs
  - Use tray-icon crate
  - Status icons (idle, syncing, error)
  - Animated icon during backup
  
□ 2.2.2 Create tray menu
  - Open main window
  - Quick backup now
  - Pause/resume sync
  - Recent backups submenu
  - Exit
  
□ 2.2.3 Add native notifications
  - File: skylock-gui/src/notifications.rs
  - Backup complete/failed
  - Storage warning
  - Ransomware alert
  - Update available
  
□ 2.2.4 Implement minimize to tray
  - Window close -> minimize
  - Start minimized option
  - Single instance enforcement
```

**Dependencies**: tray-icon, notify-rust
**Deliverable**: System tray with notifications

---

### 2.3 Web Dashboard Completion (Week 10-12)

**Goal**: Finish React dashboard from design spec

**Tasks**:
```
□ 2.3.1 Complete backend API
  - File: src/api/mod.rs (expand existing)
  - RESTful endpoints for all operations
  - WebSocket for real-time updates
  - JWT authentication
  
□ 2.3.2 Finish React frontend
  - File: web-dashboard/frontend/
  - Dashboard page
  - Backup management
  - File browser
  - Settings page
  
□ 2.3.3 Add real-time updates
  - WebSocket connection
  - Live backup progress
  - Activity feed
  
□ 2.3.4 Implement authentication
  - Login/logout
  - Session management
  - API key generation
  
□ 2.3.5 Mobile responsive design
  - Responsive breakpoints
  - Touch-friendly controls
```

**Dependencies**: React, TypeScript, Tailwind CSS
**Deliverable**: Web dashboard at localhost:8080

---

### 2.4 Two-Factor Authentication (Week 12-13)

**Goal**: TOTP and recovery codes

**Tasks**:
```
□ 2.4.1 Implement TOTP generation
  - File: skylock-core/src/security/totp.rs (new)
  - RFC 6238 compliant
  - QR code generation for setup
  - 6-digit codes, 30-second window
  
□ 2.4.2 Add recovery codes
  - 10 single-use codes
  - Secure generation and storage
  - Regeneration option
  
□ 2.4.3 Integrate with CLI
  - `skylock 2fa setup` - enable with QR
  - `skylock 2fa verify` - test code
  - `skylock 2fa disable` - with code verification
  
□ 2.4.4 Integrate with GUI/Web
  - Setup wizard
  - Code entry dialog
  - Recovery flow
```

**Dependencies**: totp-rs, qrcode
**Deliverable**: 2FA protection for sensitive operations

---

### 2.5 Phase 2 Integration & Testing (Week 13-14)

**Tasks**:
```
□ 2.5.1 GUI usability testing
□ 2.5.2 Cross-platform GUI testing (Linux, Windows, macOS)
□ 2.5.3 Web dashboard security audit
□ 2.5.4 2FA implementation review
□ 2.5.5 Documentation (user guide with screenshots)
□ 2.5.6 Release v0.8.0
```

---

## Phase 3: Performance Optimization (4-6 weeks)

### 3.1 Block-Level Deduplication (Week 15-17)

**Goal**: Content-defined chunking with 60-80% storage savings

**Tasks**:
```
□ 3.1.1 Implement CDC algorithm
  - File: skylock-backup/src/dedup/cdc.rs (new)
  - FastCDC or Rabin fingerprinting
  - Variable chunk size (min 64KB, avg 256KB, max 1MB)
  - Content-based boundaries
  
□ 3.1.2 Create chunk store
  - File: skylock-backup/src/dedup/store.rs (new)
  - Content-addressable storage (SHA-256)
  - Reference counting
  - Garbage collection
  
□ 3.1.3 Build chunk index
  - File: skylock-backup/src/dedup/index.rs (new)
  - Bloom filter for fast lookups
  - LRU cache for hot chunks
  - SQLite persistent index
  
□ 3.1.4 Integrate with backup pipeline
  - Chunk -> hash -> check exists -> upload if new
  - Parallel chunk processing
  - Manifest records chunk references
  
□ 3.1.5 Add dedup statistics
  - Dedup ratio per backup
  - Total storage saved
  - Chunk distribution analysis
```

**Dependencies**: fastcdc, sha2
**Deliverable**: 60%+ storage reduction for similar files

---

### 3.2 Delta Encoding (Week 17-18)

**Goal**: Upload only changed bytes for modified files

**Tasks**:
```
□ 3.2.1 Implement binary diff
  - File: skylock-backup/src/delta/diff.rs (new)
  - rsync-style rolling checksums
  - Delta instructions (copy, insert)
  - Compression of delta
  
□ 3.2.2 Create delta application
  - File: skylock-backup/src/delta/patch.rs (new)
  - Apply delta to base file
  - Verification after patch
  
□ 3.2.3 Add delta storage
  - Store deltas against previous version
  - Chain limit (max 10 deltas before full)
  - Automatic consolidation
  
□ 3.2.4 Integrate with restore
  - Reconstruct file from base + deltas
  - Parallel delta fetch
  - Cache reconstructed files
```

**Dependencies**: xdelta3 or pure Rust impl
**Deliverable**: 90%+ bandwidth reduction for large file edits

---

### 3.3 Performance Optimization (Week 18-20)

**Goal**: 200+ MB/s throughput

**Tasks**:
```
□ 3.3.1 Optimize parallel uploads
  - Dynamic thread pool (already started)
  - Better work stealing
  - Connection reuse improvements
  
□ 3.3.2 Improve memory efficiency
  - Streaming encryption (no full file in memory)
  - Memory-mapped files for large files
  - Buffer pool reuse
  
□ 3.3.3 Add compression optimization
  - Detect incompressible files (skip compression)
  - Parallel compression
  - Dictionary compression for similar files
  
□ 3.3.4 Benchmark and profile
  - Criterion benchmarks
  - Flamegraph profiling
  - Memory profiling with heaptrack
```

**Deliverable**: 200 MB/s on gigabit, <5% CPU overhead

---

### 3.4 Phase 3 Integration & Testing (Week 20-21)

**Tasks**:
```
□ 3.4.1 Dedup correctness tests
□ 3.4.2 Delta encoding edge cases
□ 3.4.3 Performance regression tests
□ 3.4.4 Large-scale tests (1TB+ data)
□ 3.4.5 Documentation updates
□ 3.4.6 Release v0.9.0
```

---

## Phase 4: Advanced Features (8-12 weeks)

### 4.1 Mobile App Foundation (Week 22-25)

**Tasks**:
```
□ 4.1.1 Set up Flutter project
□ 4.1.2 Implement Rust FFI bridge
□ 4.1.3 Create browse/restore UI
□ 4.1.4 Add photos backup
□ 4.1.5 iOS and Android builds
```

### 4.2 Virtual Drive / FUSE (Week 25-27)

**Tasks**:
```
□ 4.2.1 Implement FUSE filesystem
□ 4.2.2 Read-only backup mount
□ 4.2.3 Caching layer
□ 4.2.4 Windows support (WinFsp)
```

### 4.3 Plugin System (Week 27-29)

**Tasks**:
```
□ 4.3.1 Define plugin API
□ 4.3.2 Storage provider plugins
□ 4.3.3 Notification plugins
□ 4.3.4 Processing pipeline hooks
```

### 4.4 Final Polish (Week 29-32)

**Tasks**:
```
□ 4.4.1 Security audit
□ 4.4.2 Performance audit
□ 4.4.3 Documentation complete
□ 4.4.4 Release v1.0.0
```

---

## File Structure Overview

```
skylock-hybrid/
├── skylock-core/
│   └── src/
│       ├── storage/providers/
│       │   ├── aws.rs          # Complete S3
│       │   ├── backblaze.rs    # Complete B2
│       │   └── unified.rs      # NEW: Unified interface
│       └── security/
│           └── totp.rs         # NEW: 2FA
│
├── skylock-backup/
│   └── src/
│       ├── watcher.rs          # NEW: File watcher
│       ├── sync_queue.rs       # NEW: Change queue
│       ├── continuous.rs       # NEW: Daemon mode
│       ├── sync_state.rs       # NEW: SQLite state
│       ├── sharing/            # NEW: File sharing
│       │   ├── mod.rs
│       │   └── links.rs
│       ├── ransomware/         # NEW: Protection
│       │   ├── mod.rs
│       │   ├── entropy.rs
│       │   ├── behavior.rs
│       │   └── alerts.rs
│       ├── dedup/              # NEW: Deduplication
│       │   ├── mod.rs
│       │   ├── cdc.rs
│       │   ├── store.rs
│       │   └── index.rs
│       └── delta/              # NEW: Delta encoding
│           ├── mod.rs
│           ├── diff.rs
│           └── patch.rs
│
├── skylock-gui/                # NEW: Desktop GUI crate
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── app.rs
│       ├── tray.rs
│       ├── notifications.rs
│       └── views/
│           ├── dashboard.rs
│           ├── backups.rs
│           ├── restore.rs
│           └── settings.rs
│
├── src/
│   ├── share_server.rs         # NEW: Sharing HTTP server
│   └── api/                    # Expand for web dashboard
│
├── web-dashboard/              # Complete existing
│   ├── backend/
│   └── frontend/
│
└── systemd/
    └── skylock-watch.service   # NEW: Watch daemon
```

---

## Release Schedule

| Version | Date | Features |
|---------|------|----------|
| v0.7.0 | Week 6 | Multi-provider, Real-time sync, File sharing, Ransomware detection |
| v0.8.0 | Week 14 | Desktop GUI, System tray, Web dashboard, 2FA |
| v0.9.0 | Week 21 | Block dedup, Delta encoding, Performance boost |
| v1.0.0 | Week 32 | Mobile app, Virtual drive, Plugin system, Production ready |

---

## Getting Started

To begin Phase 1, start with task **1.1.1** (AWS S3 provider completion).

Ready to implement when you approve.
