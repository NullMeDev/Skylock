# Backup Verification Guide

How to verify your backups for integrity and recoverability.

---

## Why Verify?

Verification ensures your backups are complete and readable. It detects issues like:
- Missing or partial files
- Corrupted uploads
- Encryption/Decryption failures
- Network or storage issues during backup

---

## Verification Modes

### Quick Verification (Fast)

Checks that files exist on the remote storage.

```bash
# Verify backup existence only
skylock verify <backup_id>
```

- Speed: Very fast (no downloads)
- Use for: Routine checks after backup
- Detects: Missing files
- Does not detect: Corruption

### Full Verification (Thorough)

Downloads, decrypts, decompresses, and verifies SHA-256 hashes for every file.

```bash
# Full integrity verification
skylock verify <backup_id> --full
```

- Speed: Slower (downloads all files)
- Use for: Monthly verification, before deleting local copies, before major changes
- Detects: Missing files, hash mismatches, decryption/decompression errors

---

## Example Outputs

### Quick Verification

```
🔍 Verifying backup: backup_20251108_140000 (quick)

✔ Files exist on remote: 5,230 / 5,230
✖ Missing files: 0

✅ Verification PASSED (Quick)
```

### Full Verification

```
🔍 Verifying backup: backup_20251108_140000 (full)

✔ Files verified: 5,230 / 5,230
✖ Missing files: 0
✖ Corrupted files: 0

✅ Verification PASSED (Full)
```

### Failure Cases

```
🔍 Verifying backup: backup_20251108_140000 (full)

✔ Files verified: 5,228 / 5,230
✖ Missing files: 1
✖ Corrupted files: 1

❌ Verification FAILED

Missing files (1):
  /home/user/Documents/notes.txt

Corrupted files (1):
  /home/user/Pictures/photo.jpg (hash mismatch)

🔧 Suggestions:
- Re-run backup to re-upload missing files
- Check network connectivity and storage quota
- Consider a full backup if issues persist
```

---

## Scheduling Verification

Add a monthly verification to your schedule:

```bash
# Verify most recent backup (quick)
0 5 * * 1 skylock verify $(skylock list | head -1 | awk '{print $1}')

# Verify most recent backup (full) once a month
0 3 1 * * skylock verify $(skylock list | head -1 | awk '{print $1}') --full
```

---

## Best Practices

- Run quick verification after each backup
- Run full verification monthly or before major changes
- Re-run backups after failures to re-upload missing files
- Keep an eye on storage quota and network reliability
- Verify before cleaning up old local copies

---

## Advanced Options

- Limit concurrency to reduce load (if supported): `--concurrency 2`
- Verify a subset of files (future feature): `--path <prefix>`
- Export verification report: `--output verification_report.json` (future)

---

## Troubleshooting

### "File missing" errors
- Cause: Network errors, interruptions, deleted remote files
- Fix: Re-run backup for the affected paths

### "Hash mismatch" errors
- Cause: Corruption during upload/download, disk issues
- Fix: Re-run backup; check disk health; test local file integrity

### "Decryption failed"
- Cause: Wrong encryption key or corrupted file
- Fix: Verify your encryption key; re-run backup

---

## Quick Commands

```bash
# Quick verification
skylock verify <backup_id>

# Full verification
skylock verify <backup_id> --full

# Verify latest backup
skylock verify $(skylock list | head -1 | awk '{print $1}')

# Verify and save report (future)
skylock verify <backup_id> --full --output verify.json
```
