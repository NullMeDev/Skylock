# Skylock Restore Guide

Complete guide for restoring encrypted backups with Skylock.

## Table of Contents

- [Quick Start](#quick-start)
- [Preview Before Restore](#preview-before-restore)
- [Full Backup Restore](#full-backup-restore)
- [Individual File Restore](#individual-file-restore)
- [Integrity Verification](#integrity-verification)
- [Advanced Usage](#advanced-usage)
- [Troubleshooting](#troubleshooting)

## Quick Start

### 1. List Available Backups

```bash
skylock list
```

This shows all available backups with their IDs, dates, sizes, and source paths.

### 2. Preview a Backup

```bash
skylock preview <backup_id>
```

View backup contents before restoring:
- Backup metadata (date, size, file count)
- Complete file listing organized by directory
- Compression and encryption status for each file

### 3. Restore a Backup

```bash
skylock restore <backup_id> --target /path/to/restore
```

Restores the entire backup to the specified directory.

## Preview Before Restore

The preview command lets you inspect backup contents without downloading anything.

### Basic Preview

```bash
skylock preview 20251107_022016
```

Shows:
- Backup date and total size
- Number of files
- Directory structure
- Individual file sizes and timestamps
- Encryption/compression indicators

### Check for Conflicts

Preview with a target directory to detect file conflicts:

```bash
skylock preview 20251107_022016 --target /home/user/restore_dir
```

This will:
- Show all backup contents
- Check if any files already exist at the target location
- List all potential conflicts
- Help you decide whether to proceed

Example output:
```
⚠️  File Conflicts Detected
   5 files already exist

   The following files will be overwritten:
   1. /home/user/restore_dir/file1.dat
   2. /home/user/restore_dir/file2.dat
   ...
```

## Full Backup Restore

Restore an entire backup with real-time progress tracking.

### Basic Restore

```bash
skylock restore 20251107_022016 --target /home/user/restore_dir
```

Features:
- **Progress Bars**: Real-time download and decryption progress
- **Integrity Checks**: SHA-256 verification for every file
- **Smart Decryption**: Automatic handling of compressed files
- **Directory Creation**: Automatically creates target directories

### Progress Tracking

During restore, you'll see:

```
📦 Overall Progress
████████████████████░░░░░░░░░░░░ 3/5 files (60%) ETA: 10s

⬇️  Restoring: file3.dat
████████████████████████████████ 2.0 MB/2.0 MB (1.5 MB/s) ETA: 0s
```

### Restore to Current Directory

If you don't specify a target, Skylock creates a timestamped directory:

```bash
skylock restore 20251107_022016
# Creates: restore_20251107_025530/
```

## Individual File Restore

Restore a single file from any backup without downloading the entire backup.

```bash
skylock restore-file <backup_id> <file_path> --output /path/to/save.dat
```

Example:

```bash
skylock restore-file 20251107_022016 /tmp/progress_test/file1.dat --output ~/recovered_file.dat
```

This:
- Downloads only the requested file
- Decrypts and decompresses automatically
- Verifies integrity
- Saves to the specified location

## Integrity Verification

Every file is automatically verified during restore.

### How It Works

1. **SHA-256 Hash**: Computed during backup
2. **Download**: Encrypted file downloaded from storage
3. **Decrypt**: AES-256-GCM decryption
4. **Decompress**: If file was compressed (>10 MB)
5. **Verify**: Hash computed and compared to original
6. **Write**: Only written if hash matches

### If Verification Fails

If a file fails integrity check:

```
⚠️  Failed to restore /path/to/file.dat: Integrity check failed
     Expected: a1b2c3d4...
     Got:      e5f6g7h8...
```

This indicates:
- Corruption during download or storage
- Tampering attempt detected
- Encryption key mismatch

**Solution**: Try restoring again. If it fails repeatedly, the backup may be corrupted.

## Advanced Usage

### Restore with Logging

Enable detailed logging:

```bash
RUST_LOG=info skylock restore 20251107_022016 --target /restore
```

Logs are saved to: `~/.local/share/skylock/logs/`

### Batch Restore

Restore multiple backups:

```bash
#!/bin/bash
for backup in $(skylock list | grep backup_ | awk '{print $1}'); do
    echo "Restoring $backup..."
    skylock restore "$backup" --target "/backups/$backup"
done
```

### Verify Before Restore

Always preview first:

```bash
# 1. Preview backup
skylock preview 20251107_022016

# 2. Check for conflicts
skylock preview 20251107_022016 --target /restore

# 3. Restore if safe
skylock restore 20251107_022016 --target /restore
```

## Restore Process Details

### What Happens During Restore

1. **Load Configuration**
   - Connects to Hetzner Storage Box
   - Verifies credentials

2. **Download Manifest**
   - Fetches backup metadata (manifest.json)
   - Lists all files and their properties

3. **Create Target Directory**
   - Creates directory structure
   - Preserves original paths

4. **Restore Each File**
   - Download encrypted file
   - Decrypt with AES-256-GCM
   - Decompress if needed (zstd)
   - Verify SHA-256 hash
   - Write to target location

5. **Complete**
   - Report statistics
   - Show duration and transfer rates

### Performance

- **Parallel Downloads**: Currently sequential for reliability
- **Streaming**: Files processed in chunks
- **Memory Efficient**: Temporary files cleaned up automatically

Typical speeds:
- Download: Network-dependent (usually 5-50 MB/s)
- Decryption: ~200-500 MB/s
- Decompression: ~300-800 MB/s
- Writing: Disk-dependent (usually 100-500 MB/s)

## Troubleshooting

### Common Issues

#### 1. Backup Not Found

```
Error: 404 Not Found
```

**Cause**: Backup ID doesn't exist or manifest is missing

**Solution**:
- Run `skylock list` to see available backups
- Ensure you're using the correct backup ID

#### 2. Credentials Error

```
Error: Hetzner credentials not configured
```

**Solution**:
- Check config file: `~/.config/skylock-hybrid/config.toml`
- Verify username, password, and endpoint are correct

#### 3. Connection Failed

```
Error: Failed to initialize Hetzner client
```

**Solution**:
- Check internet connection
- Verify Hetzner Storage Box is accessible
- Test with: `skylock test hetzner`

#### 4. Integrity Check Failed

```
Error: Integrity check failed: hash mismatch
```

**Cause**: File corruption or wrong encryption key

**Solution**:
- Verify encryption key in config matches the one used for backup
- Try downloading again (may be transient network error)
- If persistent, backup may be corrupted

#### 5. Permission Denied

```
Error: Permission denied
```

**Solution**:
- Ensure you have write permissions to target directory
- Use a different target directory
- Run with appropriate permissions

### Getting Help

If you encounter issues:

1. **Enable Debug Logging**:
   ```bash
   RUST_LOG=debug skylock restore <backup_id> --target /restore 2>&1 | tee restore.log
   ```

2. **Check Logs**:
   ```bash
   tail -100 ~/.local/share/skylock/logs/*.log
   ```

3. **Test Connection**:
   ```bash
   skylock test all
   ```

4. **Verify Configuration**:
   ```bash
   cat ~/.config/skylock-hybrid/config.toml
   ```

## Security Notes

### Encryption Key

- **Critical**: You must have the same encryption key used during backup
- **Backup Your Key**: Store it securely offline
- **Lost Key = Lost Data**: No recovery possible without the key

### Verification

- All files are verified with SHA-256 hashes
- AES-256-GCM provides authenticated encryption
- Tampering is automatically detected

### Safe Restore Practices

1. **Preview First**: Always preview before restoring
2. **Check Conflicts**: Use `--target` flag with preview
3. **Verify Space**: Ensure sufficient disk space
4. **Test Small First**: Restore a single file before full restore
5. **Keep Backups**: Don't delete original backup until verified

## Examples

### Example 1: Preview and Restore

```bash
# List all backups
skylock list

# Preview specific backup
skylock preview 20251107_022016

# Check for conflicts
skylock preview 20251107_022016 --target ~/restore

# Restore if safe
skylock restore 20251107_022016 --target ~/restore

# Verify restored files
ls -lh ~/restore/
```

### Example 2: Selective File Restore

```bash
# Preview to find file path
skylock preview 20251107_022016

# Restore just one file
skylock restore-file 20251107_022016 /tmp/progress_test/important.dat --output ~/important.dat

# Verify
sha256sum ~/important.dat
```

### Example 3: Emergency Restore

```bash
# Fastest restore to temporary location
mkdir -p /tmp/emergency_restore
skylock restore <most_recent_backup_id> --target /tmp/emergency_restore

# Check what was restored
find /tmp/emergency_restore -type f

# Copy needed files
cp /tmp/emergency_restore/path/to/important/file ~/safe_location/
```

## Next Steps

After successfully restoring:

1. **Verify Restored Data**: Check that all files are accessible and valid
2. **Update Backup Strategy**: Consider retention policies
3. **Set Up Automation**: Schedule regular backups
4. **Document Recovery Process**: Keep restore instructions handy

## See Also

- [README.md](README.md) - Main documentation
- [USAGE.md](USAGE.md) - General usage guide
- [SECURITY.md](SECURITY.md) - Security best practices
