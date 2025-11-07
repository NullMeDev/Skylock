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

### Generate configuration
```bash
skylock config --output ~/.config/skylock-hybrid/config.toml
```

## 🔐 Direct Upload Mode vs Archive Mode

### Direct Upload (--direct flag)
✅ **Best for most use cases**
- Uploads each file individually with AES-256-GCM encryption
- No temp files or disk space issues
- Can restore individual files instantly
- Parallel uploads (4 threads)
- Smart compression for files >10MB

### Archive Mode (no --direct flag)
⚠️ **Only for very large backups**
- Creates a single tar.zst archive
- Requires local disk space for temporary files
- Must download entire archive to restore
- Better compression ratio
- Can cause system slowdown during backup

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
