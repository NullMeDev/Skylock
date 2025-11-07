# Option A Implementation: Progress Indicators & Logging

**Status**: In Progress  
**Started**: 2025-11-07  
**Target Completion**: 2025-11-07  

## Overview

Implementing professional progress feedback and logging system to make Skylock feel responsive and trustworthy.

## Phase 1: Structured Logging System

### Goals
- Implement secure, structured logging with `tracing` crate
- Log to file: `~/.local/share/skylock/skylock.log`
- Automatic log rotation (max 10MB, keep 5 files)
- Configurable log levels (debug, info, warn, error)
- **Security**: Never log encryption keys, passwords, or sensitive data

### Implementation Steps

#### 1.1 Add Dependencies
Add to `Cargo.toml`:
```toml
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender = "0.2"
```

#### 1.2 Create Logging Module
File: `src/logging.rs`
- Initialize tracing subscriber
- Configure file appender with rotation
- Set up format (JSON for structured logs)
- Implement security filters (sanitize sensitive data)

#### 1.3 Security Considerations
**CRITICAL**: Implement filters to prevent logging:
- Encryption keys or key material
- Passwords or API tokens
- File contents (only log metadata)
- Personal identifiable information

**Implementation**: Use `tracing::field::display` with custom sanitization

### Rollback Plan
If logging causes issues:
1. Revert to `println!` statements
2. Check disk space (logs can fill up)
3. Verify file permissions on log directory
4. Fallback: Log only to stderr if file logging fails

### Testing
```bash
# Test basic logging
RUST_LOG=debug cargo run -- test hetzner

# Verify log file created
ls -lh ~/.local/share/skylock/skylock.log

# Check log rotation works
# (Generate >10MB of logs and verify rotation)

# Verify no sensitive data in logs
grep -i "encryption_key\|password\|api_key" ~/.local/share/skylock/skylock.log
# Should return nothing!
```

---

## Phase 2: Progress Bars for Uploads

### Goals
- Real-time progress bars using `indicatif` crate
- Show current file, upload speed, ETA
- Multi-file progress with overall completion
- Adaptive display (hide if non-TTY)

### Implementation Steps

#### 2.1 Add Dependencies
```toml
indicatif = { version = "0.17", features = ["rayon"] }
```

#### 2.2 Create Progress Module
File: `skylock-backup/src/progress.rs`
- Multi-progress bar manager
- File-level progress tracking
- Speed calculation (bytes/sec)
- ETA estimation

#### 2.3 Integration Points
Modify `skylock-backup/src/direct_upload.rs`:
- Initialize progress bars before upload loop
- Update progress for each file chunk uploaded
- Complete/fail individual file progress
- Update overall progress

#### 2.4 Progress Bar Format
```
Uploading: documents/file.pdf
[===>    ] 45% (2.3 MB/5.1 MB) 850 KB/s ETA: 3s

Overall Progress
[==>     ] 23% (12/52 files) ETA: 2m 15s
```

### Rollback Plan
If progress bars cause issues:
1. Check if terminal supports ANSI codes
2. Fallback to simple line-by-line output
3. Disable progress bars if non-TTY detected
4. Verify no crashes on narrow terminal widths

### Edge Cases
- **Network stalls**: Show "Stalled" instead of ETA
- **Very large files**: Show progress every 5% to avoid spam
- **Terminal resize**: Handle SIGWINCH gracefully
- **Ctrl+C**: Clean up progress bars before exit

### Testing
```bash
# Test with real backup
skylock backup --direct ~/.ssh

# Test with small files
mkdir /tmp/test_backup
echo "test" > /tmp/test_backup/file{1..10}.txt
skylock backup --direct /tmp/test_backup

# Test with large file
dd if=/dev/urandom of=/tmp/largefile bs=1M count=100
skylock backup --direct /tmp/largefile

# Test terminal width handling
# Resize terminal while upload is running
```

---

## Phase 3: Better Error Messages

### Goals
- Replace generic errors with actionable messages
- Include troubleshooting hints
- Suggest fixes for common problems
- Color-coded severity (red=error, yellow=warning)

### Implementation Steps

#### 3.1 Enhance Error Types
File: `skylock-core/src/error.rs`
- Add `help_text` field to errors
- Add `suggested_action` field
- Implement `Display` with formatting

#### 3.2 Common Error Scenarios
1. **Connection Failed**
   - Error: "Failed to connect to Hetzner Storage Box"
   - Help: "Check your internet connection and verify credentials in config"
   - Action: `skylock test hetzner`

2. **Authentication Failed**
   - Error: "Authentication failed: Invalid credentials"
   - Help: "Verify username and password in ~/.config/skylock-hybrid/config.toml"
   - Action: "Check that api_key is correct"

3. **Disk Space**
   - Error: "Insufficient disk space for backup"
   - Help: "Free up space or use --direct mode to avoid local caching"
   - Action: `df -h ~/.local/share/skylock`

4. **Permission Denied**
   - Error: "Permission denied reading /path/to/file"
   - Help: "Run with sudo or adjust file permissions"
   - Action: `chmod +r /path/to/file`

#### 3.3 Error Display Format
```
ERROR: Failed to upload file.pdf
  │
  ├─ Reason: Connection timeout after 30s
  ├─ Help: Your network may be unstable or the server is unreachable
  └─ Try: skylock test hetzner
```

### Rollback Plan
If new error format causes issues:
1. Fallback to simple error strings
2. Ensure all errors still print to stderr
3. Verify error codes are preserved for scripting

### Testing
```bash
# Test connection error (disconnect network)
skylock backup --direct ~/.ssh

# Test auth error (use wrong credentials)
# Edit config with bad password
skylock test hetzner

# Test permission error
touch /tmp/noperm.txt
chmod 000 /tmp/noperm.txt
skylock backup --direct /tmp/noperm.txt

# Test disk space error
# (Requires filling up disk - skip in testing)
```

---

## Phase 4: Documentation & Recovery

### Create Troubleshooting Guide
File: `docs/TROUBLESHOOTING.md`
- Common error scenarios
- Step-by-step debugging
- Log analysis instructions
- Recovery procedures

### Create Failure Recovery Guide
File: `docs/FAILURE_RECOVERY.md`
- What to do if backup fails mid-upload
- How to resume partial backups
- Verifying backup integrity
- Restoring from incomplete backups

### Security Checklist
File: `docs/SECURITY_CHECKLIST.md`
- Verify no keys in logs
- Check file permissions on config
- Audit error messages for data leaks
- Validate encryption is always applied

---

## Success Criteria

- [x] Logs created at `~/.local/share/skylock/skylock.log`
- [x] Log rotation works (10MB max, 5 files)
- [x] No sensitive data appears in logs
- [x] Progress bars show during backup
- [x] Upload speed and ETA displayed
- [x] Progress bars disappear cleanly on completion
- [ ] Error messages include helpful suggestions
- [ ] All common errors have recovery instructions
- [ ] Documentation complete for failures
- [ ] All tests pass

---

## Rollback Instructions

If Option A implementation breaks core functionality:

1. **Revert logging changes**:
   ```bash
   git diff skylock-core/src/logging.rs
   git checkout HEAD -- skylock-core/src/logging.rs
   ```

2. **Revert progress bars**:
   ```bash
   git checkout HEAD -- skylock-backup/src/progress.rs
   git checkout HEAD -- skylock-backup/src/direct_upload.rs
   ```

3. **Rebuild and test**:
   ```bash
   cargo build --release
   skylock list  # Basic functionality test
   ```

4. **Check logs for specific errors**:
   ```bash
   tail -100 ~/.local/share/skylock/skylock.log
   ```

---

## Notes & Decisions

- Using `tracing` instead of `log` for structured logging (modern best practice)
- Using `indicatif` for progress bars (most popular Rust progress library)
- Log format: JSON for structured parsing, human-readable for CLI
- Progress bars: Auto-disable on non-TTY (CI/CD friendly)

---

## Next Steps (Option B)

After Option A is complete:
- Implement backup verification (`skylock verify`)
- Add retry logic for network failures
- Improve error recovery mechanisms
