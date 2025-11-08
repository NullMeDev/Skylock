# Incremental Backup Guide

Complete guide to using Skylock's incremental backup feature for efficient, fast backups.

---

## Table of Contents
- [Overview](#overview)
- [How It Works](#how-it-works)
- [Basic Usage](#basic-usage)
- [Backup Chains](#backup-chains)
- [Restoring from Incremental Backups](#restoring-from-incremental-backups)
- [File Change Tracking](#file-change-tracking)
- [Best Practices](#best-practices)
- [Troubleshooting](#troubleshooting)
- [Advanced Topics](#advanced-topics)

---

## Overview

### What are Incremental Backups?

Incremental backups only upload files that have changed since the last backup. Instead of re-uploading your entire dataset every time, Skylock intelligently detects which files are new, modified, or unchanged.

### Benefits

| Benefit | Description | Impact |
|---------|-------------|--------|
| **Faster Backups** | Only process changed files | 10-100x faster for large datasets |
| **Reduced Bandwidth** | Upload only what changed | Save bandwidth costs |
| **Less System Load** | Fewer files to encrypt/compress | Lower CPU/memory usage |
| **Quick Scheduling** | Fast enough for frequent backups | Enable hourly backups |

### When to Use

✅ **Use Incremental Backups For:**
- Daily or frequent backups
- Large datasets (>100GB)
- Limited bandwidth connections
- Automated scheduled backups
- Workstations with constantly changing data

❌ **Use Full Backups For:**
- First-time backups (no previous index)
- After major directory restructuring
- When you want to force re-upload everything
- Verification after suspected corruption

---

## How It Works

### File Index

After each backup, Skylock creates a **file index** containing:

```json
{
  "files": {
    "/path/to/file.txt": {
      "path": "/path/to/file.txt",
      "size": 1024,
      "modified": "2025-11-08T12:00:00Z",
      "hash": "abc123..."
    }
  },
  "created_at": "2025-11-08T12:00:00Z",
  "tracked_dirs": ["/home/user/Documents"]
}
```

**Stored at:** `~/.local/share/skylock/indexes/`

### Change Detection

On incremental backup, Skylock:

1. **Loads previous index** from `~/.local/share/skylock/indexes/latest.index.json`
2. **Scans current files** in specified directories
3. **Compares metadata** (size, modification time)
4. **Computes SHA-256 hashes** for potentially changed files
5. **Detects changes**:
   - **Added**: New files not in previous index
   - **Modified**: Files with different hashes
   - **Removed**: Files missing from current scan
   - **Unchanged**: Same size, mtime, and hash

### Upload Strategy

Only files marked as **Added** or **Modified** are uploaded. This creates a complete backup where:
- Changed files are uploaded to the new backup
- Unchanged files reference the previous backup (via manifest)
- Each backup is independently complete and restorable

**Important:** Unlike some backup systems, Skylock's incremental backups are still **complete, standalone backups**. Each backup contains all files needed for restore, not just the delta.

---

## Basic Usage

### First Backup (Full)

Always start with a full backup:

```bash
skylock backup --direct ~/Documents ~/Pictures
```

**Output:**
```
🔐 Encrypting and uploading files...
✅ Backed up 5,230 files (12.3 GB) in 8m 45s
💾 Backup ID: backup_20251108_120000
```

### Subsequent Backups (Incremental)

Add the `--incremental` flag:

```bash
skylock backup --direct --incremental ~/Documents ~/Pictures
```

**Output:**
```
📊 Detecting changes since last backup...
✅ Found 42 changed files out of 5,230 total
➡️ Incremental: Backing up 42 changed files, skipping 5,188 unchanged

🔐 Encrypting and uploading files...
✅ Backed up 42 files (156 MB) in 23s
💾 Backup ID: backup_20251108_140000
```

### Check What Changed

Before backing up, see what would be included:

```bash
skylock changes
```

**Output:**
```
📊 File Changes Since Last Backup

✅ Added (5 files):
  /home/user/Documents/new_project/README.md
  /home/user/Documents/new_project/code.py
  ...

📝 Modified (37 files):
  /home/user/Documents/work/report.pdf
  /home/user/Pictures/photo_2025.jpg
  ...

❌ Removed (2 files):
  /home/user/Documents/old_draft.txt
  ...

📈 Summary:
  Total files: 5,230
  Added: 5
  Modified: 37
  Removed: 2
  Unchanged: 5,188
```

---

## Backup Chains

### What is a Backup Chain?

A backup chain is a series of incremental backups building on each other. Each backup in the chain records its base backup ID.

```
backup_20251108_080000 (full)
    ↓ (base)
backup_20251108_120000 (incremental)
    ↓ (base)
backup_20251108_160000 (incremental)
    ↓ (base)
backup_20251108_200000 (incremental)
```

### Viewing Chain Information

Check backup details:

```bash
skylock list --detailed
```

**Output:**
```
Backup: backup_20251108_120000
  Date: 2025-11-08 12:00:00
  Files: 5,230
  Size: 12.3 GB
  Base Backup: backup_20251108_080000  ← incremental
  Status: Complete
```

### Breaking a Chain

Start a new chain by omitting `--incremental`:

```bash
# This creates a new full backup (no base)
skylock backup --direct ~/Documents ~/Pictures
```

**When to break chains:**
- After major file reorganization
- Monthly "full backup" for peace of mind
- After suspected index corruption
- When chain becomes too long (>30 incremental backups)

---

## Restoring from Incremental Backups

### Full Restore

Incremental backups restore exactly like full backups:

```bash
skylock restore backup_20251108_140000 --target ~/restored_files
```

**How it works:**
1. Skylock reads the manifest for `backup_20251108_140000`
2. Downloads all files listed in the manifest
3. Each file is complete and independent (no dependency on previous backups)

**Note:** Even though this was an incremental backup, the restore is straightforward. Skylock doesn't need to walk the chain or apply deltas.

### Single File Restore

```bash
skylock restore-file backup_20251108_140000 \
    "/home/user/Documents/report.pdf" \
    --output ~/report_restored.pdf
```

Works identically for incremental and full backups.

### Restore from Specific Point in Time

List backups and choose the one you need:

```bash
# List all backups
skylock list

# Restore from specific backup
skylock restore backup_20251107_200000 --target ~/restored_old_version
```

---

## File Change Tracking

### Change Detection Commands

```bash
# Show all changes
skylock changes

# Show summary only
skylock changes --summary

# Check specific paths
skylock changes ~/Documents ~/Pictures

# Save changes to file
skylock changes > changes_report.txt
```

### Understanding Change Types

| Type | Description | Included in Incremental Backup? |
|------|-------------|----------------------------------|
| **Added** | New file not in previous backup | ✅ Yes |
| **Modified** | File content changed (detected via SHA-256) | ✅ Yes |
| **Removed** | File deleted since last backup | ❌ No (absence recorded in index) |
| **MetadataChanged** | Only mtime/permissions changed, content same | ❌ No (optimization) |

### Change Detection Process

1. **Load Previous Index**
   - Located at `~/.local/share/skylock/indexes/latest.index.json`
   - Falls back to most recent backup's index

2. **Scan Current Files**
   - Walk specified directories
   - Collect metadata (path, size, mtime)

3. **Compare**
   - Files in current but not previous → **Added**
   - Files in previous but not current → **Removed**
   - Files in both with different size/mtime → check hash
     - Different hash → **Modified**
     - Same hash → **MetadataChanged**

4. **Report**
   - Display summary and details
   - Save updated index for next comparison

---

## Best Practices

### Scheduling Strategy

**Recommended Schedule:**

```bash
# Daily incremental backups
0 2 * * * skylock backup --direct --incremental ~/Documents ~/Pictures

# Weekly full backup (Sunday 3 AM)
0 3 * * 0 skylock backup --direct ~/Documents ~/Pictures

# Monthly verification
0 4 1 * * skylock verify $(skylock list | head -1 | awk '{print $1}') --full
```

### Performance Optimization

**For Large Datasets (>1TB):**

```bash
# Exclude unnecessary files
skylock backup --direct --incremental ~/Documents \
    --exclude "*.tmp" \
    --exclude "node_modules/" \
    --exclude ".cache/"
```

**For Many Small Files:**

Incremental backups are especially beneficial:
- 100,000 files with 1% daily change
- Full backup: ~2 hours
- Incremental backup: ~2 minutes

### Index Management

**Index Files:**
- `latest.index.json`: Most recent backup
- `<backup_id>.index.json`: Per-backup index

**Cleanup:**

```bash
# Indexes are automatically pruned with backups
skylock cleanup --keep-days 30

# Manual index cleanup (if needed)
rm ~/.local/share/skylock/indexes/backup_20250101_*.json
```

### Chain Length

**Recommendation:** Start fresh chain every 30 incremental backups

```bash
# Check chain length
skylock list --detailed | grep "Base Backup" | wc -l

# If >30, create new full backup
skylock backup --direct ~/Documents ~/Pictures
```

---

## Troubleshooting

### No Previous Backup Found

**Error:**
```
❌ Error: No previous backup index found. Cannot perform incremental backup.
```

**Solution:** Run a full backup first:
```bash
skylock backup --direct ~/Documents ~/Pictures
```

### Index Corruption

**Symptoms:**
- Unexpected files marked as "Added" or "Modified"
- Errors loading index file

**Solution:** Rebuild with full backup:
```bash
# Remove corrupted index
rm ~/.local/share/skylock/indexes/latest.index.json

# Create fresh full backup
skylock backup --direct ~/Documents ~/Pictures
```

### Incremental Backup Takes Too Long

**Possible Causes:**
1. Large number of changed files
2. First incremental backup after full
3. Index out of date

**Check what's changing:**
```bash
skylock changes --summary
```

If >50% of files are changing, consider a full backup.

### Files Not Detected as Changed

**Possible Issues:**

1. **Clock skew:** System time incorrect
   ```bash
   # Check system time
   timedatectl
   ```

2. **Timestamp-only changes:** File touched but not modified
   ```bash
   # This won't trigger backup (MetadataChanged, not Modified)
   touch file.txt
   ```

3. **Hash collision (extremely rare):** SHA-256 collision
   - Probability: ~1 in 2^256 (effectively impossible)

### Large Index Files

**Issue:** Index files consuming significant disk space

**Solution:**
```bash
# Check index sizes
du -h ~/.local/share/skylock/indexes/

# Remove old indexes (only if backups also deleted)
find ~/.local/share/skylock/indexes/ -name "backup_2024*.json" -delete
```

**Note:** Skylock automatically cleans up indexes when backups are deleted via `skylock cleanup`.

---

## Advanced Topics

### Manual Index Management

**Export Index:**
```bash
# Copy index for analysis
cp ~/.local/share/skylock/indexes/latest.index.json ~/backup_index_analysis.json

# Pretty-print index
jq . ~/.local/share/skylock/indexes/latest.index.json
```

**Inspect Specific File:**
```bash
# Check if file is in index
jq '.files["/home/user/Documents/report.pdf"]' \
    ~/.local/share/skylock/indexes/latest.index.json
```

### Cross-Machine Incremental Backups

**Scenario:** Backing up from multiple machines to same Hetzner account

**Setup:**
Each machine needs its own index directory to avoid conflicts:

```toml
# Machine 1: ~/.config/skylock-hybrid/config.toml
[backup]
index_dir = "/home/user/.local/share/skylock/indexes/machine1"

# Machine 2: ~/.config/skylock-hybrid/config.toml
[backup]
index_dir = "/home/user/.local/share/skylock/indexes/machine2"
```

Each machine tracks its own changes independently.

### Parallel Backup Paths

**Separate Indexes per Path:**

Currently, Skylock uses a single index for all tracked paths. For separate indexes:

```bash
# Backup Documents separately
skylock backup --direct --incremental ~/Documents

# Backup Pictures separately
skylock backup --direct --incremental ~/Pictures
```

Each backup maintains its own index based on the paths specified.

### Change Detection Algorithm Details

**Hash Computation:**
- Algorithm: SHA-256
- Computed for: Files with size/mtime differences
- Storage: Hex string (64 characters)

**Optimization:** Hashes are computed lazily:
1. First pass: Check size and mtime (fast)
2. Only if different: Compute hash (slower)

This makes incremental backups very fast when few files changed.

### Backup Manifest Structure

**With Base Backup:**
```json
{
  "backup_id": "backup_20251108_140000",
  "timestamp": "2025-11-08T14:00:00Z",
  "base_backup_id": "backup_20251108_120000",
  "files": [...],
  "total_files": 42,
  "total_size": 163840000
}
```

The `base_backup_id` field links incremental backups in a chain, but restores don't require walking the chain.

---

## Summary

### Key Takeaways

✅ **Incremental backups dramatically speed up routine backups**
✅ **Each backup is complete and independently restorable**
✅ **Change detection uses SHA-256 hashing for accuracy**
✅ **File indexes are stored locally for fast comparison**
✅ **Use `skylock changes` to preview before backing up**

### Quick Reference

```bash
# Create full backup
skylock backup --direct ~/Documents

# Create incremental backup
skylock backup --direct --incremental ~/Documents

# Check changes
skylock changes

# Restore (same for incremental or full)
skylock restore backup_20251108_140000 --target ~/restored

# Verify backup
skylock verify backup_20251108_140000 --full

# Clean up old backups
skylock cleanup --keep-days 30
```

---

**Need Help?** Check the [main README](README.md) or [USAGE.md](USAGE.md) for more information.

**Report Issues:** null@nullme.lol
