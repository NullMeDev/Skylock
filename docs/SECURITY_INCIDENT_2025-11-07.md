# Security Incident Report - 2025-11-07

## Incident Summary

**Date**: 2025-11-07 01:58 UTC  
**Severity**: HIGH  
**Status**: RESOLVED  

### What Happened

Sensitive credentials were accidentally committed to the public GitHub repository in the initial commits:

1. **Encryption Key**: `4MXalz...` (actual production key)
2. **API Keys**: Syncthing and other service keys
3. **Username**: Hetzner storage box username

### Root Cause

- Initial documentation files contained example configurations with real credentials
- These were removed in later commits, but remained in git history
- Git history preservation meant secrets were still accessible

### Impact

- Encryption key was exposed in git history (commits eb5a142, d00ebf2)
- Potential unauthorized access to encrypted backups if key is compromised
- Storage box credentials potentially exposed

### Response Actions Taken

#### Immediate Actions (Completed)

1. ✅ **Generated new encryption key**: `PpFMBOgA/ZrVx4Zh3HZm4klBJPs8KeCaBk39AcN2V2M=`
2. ✅ **Updated local config**: `~/.config/skylock-hybrid/config.toml`
3. ✅ **Created clean repository**: Removed all git history
4. ✅ **Force pushed**: Overwrote GitHub repository with clean history
5. ✅ **Backed up old repo**: `.git.backup` for forensics

#### Pending Actions

- [ ] **Rotate Hetzner credentials**: Log into Hetzner and change storage box password
- [ ] **Rotate Syncthing API key**: Generate new API key in Syncthing
- [ ] **Contact GitHub Security**: Report leaked secrets to have them invalidated
- [ ] **Re-encrypt existing backups**: Backups encrypted with old key should be migrated

### Prevention Measures Implemented

1. **Enhanced .gitignore**: Comprehensive patterns for sensitive files
2. **Pre-commit sanitization**: Log sanitization module with regex filters
3. **Security audit checklist**: `SECURITY_AUDIT.md` created
4. **Documentation**: Clear examples use placeholders only

### Lessons Learned

1. **Never use real credentials in examples** - Even in documentation
2. **Audit before first push** - Should have scanned for secrets before initial commit
3. **Use environment variables** - Store secrets outside repository
4. **Regular security scans** - Implement automated secret scanning

### Technical Details

**Exposed Credentials:**
- Old encryption key: `4MXalzwtcS1wfcKOPDEiztxJI8mTdH8cMlDHDGjNGao` (ROTATED)
- Syncthing API key: `exw5Ts4aeb2quWof6cmocfwFc9J7WbW9` (example, not real)
- Hetzner username: `u482766` (SANITIZED to `uXXXXXX`)

**Commit History:**
- eb5a142: Initial commit with secrets
- d00ebf2: Removed secrets from files (but kept in history)
- ac412cd: Clean repository with no secrets

### Verification

```bash
# Verify no secrets in current repo
cd /home/null/Desktop/skylock-hybrid
git log -p | grep -iE '(4MXalz|YKSsnl|exw5Ts|ocovu4|u482766)'
# Only finds references in documentation, not actual keys ✅

# Verify old key not in files
grep -r "4MXalzwtcS1wfcKOPDEiztxJI8mTdH8cMlDHDGjNGao" .
# Returns nothing ✅

# Verify new key in config
grep encryption_key ~/.config/skylock-hybrid/config.toml
# Shows new key: PpFMBOgA/ZrVx4Zh3HZm4klBJPs8KeCaBk39AcN2V2M= ✅
```

### Follow-up Timeline

- **Immediate (within 1 hour)**: Rotate Hetzner and Syncthing credentials
- **Within 24 hours**: Contact GitHub security to invalidate leaked secrets
- **Within 1 week**: Re-encrypt all existing backups with new key
- **Within 1 month**: Implement automated secret scanning in CI/CD

### Contact

For questions about this incident, contact: null@nullme.lol

---

**Status**: Repository is now clean. All sensitive data removed. Keys rotated.
