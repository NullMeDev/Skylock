# Security Advisory: Skylock v0.6.0

**Release Date**: 2025-01-12  
**Severity**: MODERATE - Security Hardening Release  
**Impact**: Enhanced cryptographic security, no critical vulnerabilities fixed

## Executive Summary

Skylock v0.6.0 is a major security hardening release that implements comprehensive cryptographic improvements and defense-in-depth measures. While no critical vulnerabilities were found in v0.5.1, this release significantly strengthens the security posture against theoretical attacks and implements industry best practices.

## What Changed

### 🔒 Cryptographic Hardening (Phase 1 - COMPLETE)

#### 1. Stronger KDF Defaults
**Status**: ✅ IMPLEMENTED  
**Risk Mitigated**: GPU-accelerated brute-force attacks

- **Previous**: Argon2id(64 MiB, t=3, p=4)
- **New Default**: Argon2id(64 MiB, t=4, p=4)
- **Impact**: ~33% increase in brute-force resistance
- **Paranoid Preset**: Upgraded to Argon2id(512 MiB, t=8, p=8)
- **Files**: `skylock-backup/src/encryption.rs`, `skylock-core/src/encryption.rs`

#### 2. Deterministic Nonce Derivation
**Status**: ✅ IMPLEMENTED  
**Risk Mitigated**: Nonce reuse catastrophic failure in AES-GCM

- **Previous**: Random per-file nonces stored in manifest
- **New**: HKDF-derived deterministic nonces
- **Algorithm**: `nonce = HKDF(block_key, salt=block_hash, info=chunk_index||"skylock-nonce-gcm")`
- **Benefit**: Cryptographically guaranteed uniqueness, no storage overhead
- **Files**: `skylock-core/src/security/nonce_derivation.rs`

#### 3. HMAC-Based Integrity Verification
**Status**: ✅ IMPLEMENTED  
**Risk Mitigated**: Hash collision attacks (SHA-256 collision vulnerability)

- **Previous**: SHA-256 for file integrity
- **New**: HMAC-SHA256 with HKDF-derived key
- **Key Derivation**: `hmac_key = HKDF(encryption_key, "skylock-hmac-v1")`
- **Benefit**: Prevents collision-based forgery attacks
- **Backward Compatibility**: Automatic detection and support for v1 SHA-256 backups
- **Files**: `skylock-backup/src/hmac_integrity.rs`

#### 4. TLS/SSH Transport Security
**Status**: ✅ IMPLEMENTED (Infrastructure)  
**Risk Mitigated**: Man-in-the-middle attacks, certificate substitution

- **WebDAV**: SPKI pinning infrastructure for certificate pinning
- **SFTP**: Strict host key verification mode
- **TLS**: Enforced TLS 1.3 with strong cipher suites
- **Files**: `skylock-hetzner/src/tls_pinning.rs`

### 🛠️ User-Facing Features (NEW)

#### 5. Encrypted File Browser
**Status**: ✅ IMPLEMENTED  
**Feature**: Terminal-based backup browsing with key validation

```bash
# Browse backup contents
skylock browse backup_20250112_020000

# Preview specific file
skylock preview-file backup_20250112_020000 Documents/report.pdf
```

- **Key Validation**: Automatically validates encryption key before displaying files
- **Visual Indicators**: Shows jumbled text if key invalid (security feature)
- **Metadata Display**: Encryption version, KDF params, compression status
- **Color-Coded Output**: Files grouped by directory with compression/encryption indicators
- **Files**: `skylock-backup/src/browser.rs`

#### 6. Configurable Compression
**Status**: ✅ IMPLEMENTED  
**Feature**: Adjustable compression levels to optimize performance

- **Levels**: None(0), Fast(1), Balanced(3), Good(6), Best(9), Custom(0-22)
- **Default**: Balanced (level 3), 10MB threshold
- **Statistics**: Compression ratio tracking and reporting
- **Files**: `skylock-backup/src/compression_config.rs`

### 📊 Security Audit Results

A comprehensive security audit was performed on v0.5.1 baseline:

- **Critical Issues**: 0
- **High Issues**: 0
- **Medium Issues**: 5 (all addressed in Phase 1)
- **Low Issues**: 8 (planned for Phase 2)
- **Audit Report**: `docs/security/AUDIT_v0_5_1.md`

### 🔄 Backward Compatibility

**100% backward compatible** - All existing v1 and v2 backups restore correctly:

- ✅ v1 backups (SHA-256, random nonces): Fully supported
- ✅ v2 backups (previous format): Fully supported
- ✅ New v2 backups (HMAC, HKDF nonces): Default for new backups
- ✅ Automatic version detection during restore
- ✅ No migration required for existing backups

## Upgrade Guidance

### For New Users
- Install v0.6.0 - all security improvements active by default
- Generate strong encryption key: `openssl rand -base64 32`
- Configure in `~/.config/skylock-hybrid/config.toml`

### For Existing Users (v0.5.x)
1. **Backup Configuration**: Save your current `config.toml`
2. **Update Binary**: `cargo build --release` or download new binary
3. **Verify Compatibility**: Run `skylock test hetzner`
4. **Create New Backup**: Next backup uses new security features automatically
5. **Old Backups**: Remain accessible with automatic version detection

### For Existing Users (v0.4.x or older)
1. **CRITICAL**: Update immediately (v0.4.x uses weak KDF)
2. Follow upgrade steps above
3. **Recommended**: Create new backups with v0.6.0
4. **Optional**: Keep old backups as archive, use new backups going forward

## What's NOT Fixed Yet

### Planned for Phase 2 (v0.7.0)
The following security enhancements are planned but not yet implemented:

1. **Manifest Signing** (Ed25519) - Prevents backup tampering
2. **WebDAV Metadata Encryption** - Hides filenames from server
3. **Memory Hardening** (secrecy::Secret, zeroize) - Prevents key leakage
4. **Password Strength Validation** (zxcvbn) - Enforces strong passwords
5. **Audit Logging** - Hash-chained immutable operation logs
6. **Key Rotation** - Re-encrypt backups with new keys
7. **Shamir's Secret Sharing** - Split keys across multiple shares

### No Known Vulnerabilities
- No critical security vulnerabilities identified
- No CVEs assigned
- No active exploitation observed
- This is a proactive hardening release

## Testing Recommendations

### Quick Verification
```bash
# Test connection
skylock test hetzner

# Create test backup
skylock backup --direct ~/Documents/test

# Browse backup
skylock browse <backup_id>

# Verify integrity
skylock verify <backup_id> --full

# Restore test
skylock restore <backup_id> --target /tmp/restore_test
```

### Expected Behavior
- ✅ Backups complete successfully
- ✅ Browse shows clear file listings (if key valid)
- ✅ Verification passes with no errors
- ✅ Restore completes with integrity checks passing

## References

### Security Standards
- NIST SP 800-175B (Cryptographic Key Management)
- RFC 9106 (Argon2 Memory-Hard Function)
- NIST SP 800-38D (AES-GCM Authenticated Encryption)
- RFC 5869 (HKDF Key Derivation)
- RFC 2104 (HMAC)

### Audit Documentation
- `docs/security/AUDIT_v0_5_1.md` - Baseline security audit
- `SECURITY.md` - Security architecture and best practices
- `CHANGELOG.md` - Detailed change log

## Contact

**Security Issues**: null@nullme.lol  
**General Questions**: GitHub Issues

**GPG Key**: Available on request  
**Response Time**: 24-48 hours for security issues

## Acknowledgments

This security hardening release was guided by industry best practices and recommendations from:
- OWASP Cryptographic Storage Cheat Sheet
- NIST Cybersecurity Framework
- Rust Secure Coding Guidelines
- libsodium Design Principles

---

**Recommendation**: Upgrade to v0.6.0 at your convenience. While no critical vulnerabilities exist, the security improvements provide valuable defense-in-depth protection.

**Risk Assessment**: LOW - This is a proactive security hardening release with no critical fixes required.
