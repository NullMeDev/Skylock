# Skylock Development Gameplan

## Current State Assessment

### What's Working Well
- Core backup/restore functionality with direct upload mode
- AES-256-GCM encryption with per-file encryption
- Hetzner Storage Box integration (WebDAV)
- CLI interface with all basic commands
- Backup listing and metadata tracking
- 6 existing backups on storage (tested and working)

### Pain Points to Address
1. **No progress feedback** - Backups appear frozen during upload
2. **No scheduling** - Must manually run backups
3. **No verification** - Can't validate backup integrity
4. **No retention policy** - Old backups accumulate indefinitely
5. **Missing restore verification** - Can't confirm restore success
6. **No logging** - Hard to debug issues

## Priority Roadmap

### Phase 1: Essential User Experience (Next 1-2 Weeks)

**Priority 1.1: Progress Indicators & Feedback**
- Implement real-time progress bars for uploads
- Show file-by-file progress with current file name
- Display upload speed, ETA, and transferred/total size
- Add spinners for connection/initialization phases
- **Why**: Makes backups feel responsive and gives confidence
- **Effort**: Low-Medium (1-2 days)

**Priority 1.2: Backup Verification**
- Add `skylock verify <backup_id>` command
- Download and decrypt random samples of files
- Verify manifest checksums match actual files
- Report any corruption or missing files
- **Why**: Critical for trust in backup system
- **Effort**: Medium (2-3 days)

**Priority 1.3: Logging System**
- Implement structured logging with `tracing`
- Log to file: `~/.local/share/skylock/skylock.log`
- Configurable log levels (debug, info, warn, error)
- Automatic log rotation (max 10MB, keep 5 files)
- **Why**: Essential for debugging and monitoring
- **Effort**: Low (1 day)

**Priority 1.4: Better Error Messages**
- Replace generic errors with actionable messages
- Include suggestions for common problems
- Add troubleshooting hints to error output
- **Why**: Reduces frustration, improves UX
- **Effort**: Low (1 day)

### Phase 2: Reliability & Automation (Weeks 3-4)

**Priority 2.1: Backup Scheduling**
- Implement cron-style scheduler
- Add `skylock schedule add <cron> <paths>` command
- Run as background daemon or systemd service
- Email/notification on completion/failure
- **Why**: Backups only work if they run automatically
- **Effort**: Medium (3-4 days)

**Priority 2.2: Retention Policies**
- Add retention rules to config (keep last N, keep daily/weekly/monthly)
- Implement `skylock cleanup` command
- Dry-run mode to preview deletions
- Confirm before deleting backups
- **Why**: Prevents storage from filling up
- **Effort**: Medium (2-3 days)

**Priority 2.3: Resume Interrupted Uploads**
- Track partial uploads in state file
- Resume from last uploaded file on restart
- Handle network interruptions gracefully
- **Why**: Large backups shouldn't restart from zero
- **Effort**: Medium-High (3-4 days)

**Priority 2.4: Bandwidth Throttling**
- Add `--max-speed` flag to limit upload rate
- Configurable in config.toml
- Prevents saturating network during backups
- **Why**: Allows backups during work hours
- **Effort**: Low (1-2 days)

### Phase 3: Advanced Features (Weeks 5-8)

**Priority 3.1: Incremental Backups**
- Track file modification times
- Only upload changed files
- Maintain file history across backups
- **Why**: Dramatically reduces backup time/size
- **Effort**: High (5-7 days)

**Priority 3.2: Local Backup Support**
- Add local filesystem as backup destination
- Useful for NAS, external drives, or testing
- Same encryption and features as cloud
- **Why**: Common use case, increases flexibility
- **Effort**: Medium (3-4 days)

**Priority 3.3: Backup Deduplication**
- Identify identical files across backups
- Store each unique file only once
- Reference counting for safe deletion
- **Why**: Major storage savings for similar backups
- **Effort**: High (7-10 days)

**Priority 3.4: Web Dashboard**
- Simple web UI for backup management
- View backup history and statistics
- Trigger backups and restores
- Monitor backup jobs in real-time
- **Why**: More accessible than CLI for many users
- **Effort**: High (10-14 days)

### Phase 4: Additional Cloud Providers (Weeks 9-12)

**Priority 4.1: AWS S3 Support**
- Implement S3 backend
- Support S3-compatible providers (Backblaze, Wasabi, MinIO)
- **Effort**: Medium (4-5 days)

**Priority 4.2: Google Cloud Storage**
- Implement GCS backend
- **Effort**: Medium (4-5 days)

**Priority 4.3: Azure Blob Storage**
- Implement Azure backend
- **Effort**: Medium (4-5 days)

## Immediate Next Steps (This Session)

### Option A: Quick Wins (Recommended for First Session)
**Focus: Progress Indicators + Logging (Most User-Visible)**

1. Implement progress bars for uploads (2-3 hours)
2. Add structured logging system (1-2 hours)
3. Improve error messages (1 hour)
4. Test with real backup

**Outcome**: Backups feel professional and responsive

### Option B: Reliability Focus
**Focus: Verification + Error Handling**

1. Implement backup verification command (3-4 hours)
2. Add retry logic for network failures (1-2 hours)
3. Improve error recovery (1 hour)

**Outcome**: More trustworthy and robust system

### Option C: Automation Focus
**Focus: Scheduling + Retention**

1. Implement basic cron scheduler (3-4 hours)
2. Add retention policy enforcement (2-3 hours)
3. Create systemd service file (1 hour)

**Outcome**: Backups run automatically without intervention

## Recommendation

**Start with Option A** - Progress indicators and logging are the most visible improvements and will make testing everything else easier. Once users can see what's happening, they'll trust the system more.

Then move to Option B to ensure reliability, and finally Option C for automation.

## Long-Term Vision

- **6 months**: Production-ready with scheduling, verification, multiple cloud providers
- **1 year**: GUI, mobile apps, advanced deduplication, enterprise features
- **2 years**: Kubernetes operator, distributed backups, ML-based optimization

## Decision Points

Let's decide now:
1. Which option (A, B, or C) should we tackle first?
2. What's your biggest pain point with current Skylock?
3. Do you want to focus on features you'll use personally, or features that make it ready for others?

Once you decide, we'll dive in and start implementing!
