# Skylock - Simple Usage Guide

Skylock is now installed system-wide! You can run it from any directory.

## 📋 Basic Commands

### List all backups
```bash
skylock list
```

### Create a backup (direct upload mode - recommended)
```bash
# Backup specific paths
skylock backup --direct ~/Documents ~/Pictures

# Incremental backup (only changed files since last backup)
skylock backup --direct --incremental ~/Documents ~/Pictures

# Backup paths from config file
skylock backup --direct

# Backup a single directory
skylock backup --direct ~/.ssh
```

### Create a backup (archive mode - for very large backups)
```bash
# Without --direct flag, creates encrypted tar.zst archives
skylock backup ~/Documents
```

### Check what files changed since last backup
```bash
# Show all changes
skylock changes

# Show summary only
skylock changes --summary

# Check specific paths
skylock changes ~/Documents ~/Pictures
```

### Compare two backups
```bash
# Show differences between backups
skylock diff backup_20251025_120000 backup_20251025_140000
```

### Verify backup integrity
```bash
# Quick verification (checks file existence)
skylock verify backup_20251025_042725

# Full verification (downloads and checks hashes)
skylock verify backup_20251025_042725 --full
```

### List backups with details
```bash
skylock list --detailed
```

### Restore a backup
```bash
# Restore entire backup
skylock restore backup_20251025_042725 --target ~/restored_files

# Restore specific file
skylock restore-file backup_20251025_042725 "/home/null/.ssh/id_ed25519" --output ~/my_key
```

### Test connection
```bash
skylock test hetzner
```

### Clean up old backups
```bash
# Preview what would be deleted
skylock cleanup --dry-run

# Interactive cleanup (prompts for confirmation)
skylock cleanup

# Force cleanup without confirmation
skylock cleanup --force
```

### Generate configuration
```bash
skylock config --output ~/.config/skylock-hybrid/config.toml
```

### Validate cron schedule
```bash
# Check if cron expression is valid
skylock schedule "0 2 * * *"
```

## 🔐 Direct Upload Mode vs Archive Mode

### Direct Upload (--direct flag)
✅ **Best for most use cases**
- Uploads each file individually with AES-256-GCM encryption
- No temp files or disk space issues
- Can restore individual files instantly
- Parallel uploads (4 threads)
- Smart compression for files >10MB
- Supports incremental backups

### Archive Mode (no --direct flag)
⚠️ **Only for very large backups**
- Creates a single tar.zst archive
- Requires local disk space for temporary files
- Must download entire archive to restore
- Better compression ratio
- Can cause system slowdown during backup
- No incremental backup support

## ⚡ Incremental Backups

### What are Incremental Backups?
Incremental backups only upload files that have changed since the last backup. This dramatically reduces:
- Backup time (only process changed files)
- Bandwidth usage (only upload what changed)
- Storage costs (though each backup is still complete)

### How to Use
```bash
# First backup (full)
skylock backup --direct ~/Documents

# Subsequent backups (incremental)
skylock backup --direct --incremental ~/Documents
```

### How It Works
1. **File Tracking**: After each backup, Skylock saves a file index with SHA-256 hashes
2. **Change Detection**: On incremental backup, compares current files with last index
3. **Selective Upload**: Only uploads added or modified files
4. **Complete Backups**: Each backup is still complete and independent

### Checking Changes
```bash
# See what changed since last backup
skylock changes

# Output shows:
# ✅ Added: new_file.txt
# 📝 Modified: document.pdf
# ❌ Removed: old_file.txt
```

### When to Use
- ✅ **Use incremental** for daily/frequent backups of large datasets
- ✅ **Use incremental** when bandwidth is limited
- ✅ **Use incremental** for automated scheduled backups
- ⚠️ **Use full backup** for first backup or after major changes
- ⚠️ **Use full backup** if you want to force re-upload everything

## 📁 Configuration

Your config is at: `~/.config/skylock-hybrid/config.toml`

Edit it to set:
- Backup paths
- Hetzner credentials
- Encryption key
- Schedule settings

## 💡 Tips

1. **Always use `--direct` for normal backups** - it's faster and more reliable
2. Run `skylock list` to see all your backups
3. Your encryption key is critical - never lose it!
4. Test restore periodically to verify backups work
5. The system creates unique backup IDs based on timestamp (YYYYMMDD_HHMMSS)

## 🚀 Quick Start

```bash
# 1. List current backups
skylock list

# 2. Backup your important files
skylock backup --direct ~/.ssh ~/.config ~/Documents

# 3. Verify backup was created
skylock list

# 4. Test restore (to a temp location)
skylock restore backup_20251025_042725 --target /tmp/test_restore
```

## 📞 Getting Help

```bash
# Main help
skylock --help

# Command-specific help
skylock backup --help
skylock restore --help
skylock list --help
```

That's it! Skylock is now as simple as `skylock backup` and `skylock list` from anywhere on your system.
