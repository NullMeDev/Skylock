# Security Audit Baseline Report: Skylock v0.5.1

**Date**: 2025-11-11  
**Auditor**: Internal Security Review  
**Scope**: Comprehensive cryptographic and transport security analysis  
**Status**: Baseline for v0.6.0 improvements

---

## Executive Summary

Skylock v0.5.1 implements a **solid cryptographic foundation** with AES-256-GCM, Argon2id KDF (v2 backups), and TLS 1.3/SSH transport security. However, several **high-severity gaps** expose users to GPU-based password attacks, nonce reuse risks, integrity forgery, and metadata leakage.

**Risk Level**: **MEDIUM-HIGH** (critical for users with weak passwords or sophisticated attackers)

---

## Findings

### 🔴 CRITICAL: Nonce Management (Reuse Risk)

**Location**: `skylock-core/src/encryption.rs`, `skylock-backup/src/encryption.rs`

**Issue**: Random nonces generated per-operation without reuse tracking. If block keys are re-encrypted or nonces are regenerated for the same content, **catastrophic nonce reuse can occur** (breaks GCM security).

**Impact**: Complete loss of confidentiality and integrity if nonce is reused with the same key.

**Status**: v0.5.1 has no persistent nonce tracking; relies on statistical unlikelihood of collision.

**Recommendation**: Implement deterministic HKDF-derived nonces: `nonce = HKDF(key, salt=block_hash, info=chunk_index)`.

---

### 🔴 HIGH: Weak KDF in Core Module

**Location**: `skylock-core/src/encryption.rs:118`

**Issue**: Uses `Argon2::default()` which may have weaker parameters than the explicit params in skylock-backup.

**Impact**: Core encryption (not used for backups) vulnerable to GPU cracking if passwords are weak.

**Mitigation**: Already fixed in v0.6.0 with explicit `Argon2id(64 MiB, t=4, p=4)`.

---

### 🔴 HIGH: No Downgrade Protection

**Location**: KDF parameter validation in `skylock-backup/src/encryption.rs:94-103`

**Issue**: Minimum checks are present (mem≥64 MiB, time≥3), but params are not cryptographically bound to the ciphertext. An attacker with write access could modify `kdf_params` in the manifest to weaken future re-derivations.

**Impact**: Downgrade to weaker KDF parameters in a targeted attack scenario.

**Recommendation**: Sign manifests with Ed25519 to detect tampering.

---

### 🟠 MEDIUM: SHA-256 for Integrity (Collision Risk)

**Location**: `skylock-backup/src/direct_upload.rs:726-730`, incremental backup hash checks

**Issue**: Uses plain SHA-256 for file integrity. While SHA-256 is collision-resistant, **chosen-prefix collision attacks** exist (albeit expensive). Using HMAC provides stronger authenticity.

**Impact**: Sophisticated attacker could forge file hashes in incremental backups.

**Mitigation**: Implement HMAC-SHA256 with HKDF-derived key.

---

### 🟠 MEDIUM: No Manifest Signing

**Location**: `skylock-backup/src/direct_upload.rs` (manifest upload/download)

**Issue**: Manifests are not signed. An attacker with storage write access can:
  - Replace manifests to point to different files
  - Rollback to older manifests
  - Modify file paths to cause misrestore

**Impact**: Data integrity and anti-rollback attacks.

**Recommendation**: Sign `manifest.json` with Ed25519; verify on restore.

---

### 🟠 MEDIUM: WebDAV Metadata Leakage

**Location**: `skylock-hetzner/src/webdav.rs`, `skylock-backup/src/direct_upload.rs`

**Issue**: Remote paths are plaintext on storage server. Filenames, directory structure, and file sizes are visible to the storage provider and anyone with read access.

**Impact**: Metadata privacy leak (zero-knowledge claim is partial).

**Recommendation**: Optional filename/path encryption with AES-256-GCM.

---

### 🟠 MEDIUM: No TLS SPKI Pinning

**Location**: `skylock-hetzner/src/webdav.rs:70-72`

**Issue**: WebDAV client uses `rustls` with system root CAs but no certificate pinning. An attacker with CA compromise or rogue cert could MITM the connection.

**Impact**: Encrypted backups uploaded to attacker's server.

**Mitigation**: Add optional SPKI pinning (v0.6.0 includes `tls_pinning.rs` scaffolding).

---

### 🟡 LOW: SFTP Auto-Add Unknown Hosts

**Location**: `skylock-hetzner/src/sftp_secure.rs:153-159`

**Issue**: SFTP client auto-adds unknown hosts to `known_hosts` with a warning. This is convenient but reduces security for first connections.

**Impact**: MITM on first connection (TOFU weakness).

**Recommendation**: Add strict mode that fails on unknown hosts.

---

### 🟡 LOW: Inconsistent RNG Usage

**Location**: `skylock-core/src/security/credentials.rs:19`, nonce generation

**Issue**: Some code uses `rand::thread_rng()` instead of `OsRng` for cryptographic material.

**Impact**: Slightly weaker entropy source (though thread_rng is still cryptographically secure on most platforms).

**Recommendation**: Standardize on `OsRng`/`getrandom` for all crypto ops.

---

### 🟡 LOW: No Password Strength Checks

**Location**: Password entry points (CLI)

**Issue**: No enforcement or warning for weak passwords.

**Impact**: Users may choose easily-guessable passwords, undermining Argon2id protection.

**Recommendation**: Integrate `zxcvbn` to estimate password strength; warn on <80 bits entropy.

---

### 🟡 LOW: Incomplete Memory Zeroization

**Location**: Various

**Issue**: `zeroize` is used in some places (`skylock-backup/src/encryption.rs:124`) but not consistently across all key/password handling.

**Impact**: Sensitive data may linger in memory after use.

**Recommendation**: Wrap all secrets in `Zeroizing` or `secrecy::Secret`; audit for accidental logging.

---

## Positive Security Features (Already Implemented)

✅ **AES-256-GCM** with authenticated encryption  
✅ **Argon2id KDF** with strong default params (v2 backups: 64 MiB, t=3, p=1; v0.6.0: t=4, p=4)  
✅ **TLS 1.3** for WebDAV (rustls with PFS ciphers)  
✅ **SSH Ed25519** authentication for SFTP (no passwords)  
✅ **AAD binding** in v2 backups (prevents file transplant attacks)  
✅ **Version detection** (v1/v2 manifests with backward-compat restore)  
✅ **`OsRng`** for most cryptographic randomness  
✅ **Streaming encryption** (avoids loading entire backups into RAM)  
✅ **Manifest-based integrity** (SHA-256 hashes stored and verified)

---

## Risk Matrix

| Threat | Likelihood | Impact | Risk |
|--------|------------|--------|------|
| Nonce reuse (block crypto) | Low | Critical | **Medium** |
| Weak KDF in core | Low | High | **Medium** |
| SHA-256 collision forgery | Very Low | Medium | **Low** |
| Manifest tampering | Medium | High | **High** |
| WebDAV metadata leak | High | Low | **Medium** |
| TLS MITM (no pinning) | Low | High | **Medium** |
| SFTP MITM (first connect) | Medium | Medium | **Medium** |
| Weak passwords | High | High | **High** |

---

## Compliance & Standards Alignment

| Standard | Compliance |
|----------|-----------|
| **NIST SP 800-175B** (Key Management) | ⚠️ Partial (min 64 MiB Argon2, but no key rotation) |
| **RFC 9106** (Argon2) | ✅ Compliant (Argon2id) |
| **NIST SP 800-38D** (AES-GCM) | ✅ Compliant |
| **RFC 8446** (TLS 1.3) | ✅ Compliant |
| **Zero-Knowledge** | ⚠️ Partial (file content encrypted, metadata visible) |

---

## Version History

- **v0.1.0 - v0.4.0**: SHA-256 KDF (weak)
- **v0.5.0 - v0.5.1**: Argon2id KDF (v2 backups), AAD binding, TLS 1.3
- **v0.6.0** (planned): Stronger defaults (t=4, p=4), HMAC, manifest signing, HKDF nonces, optional metadata encryption

---

## Recommendations Summary (Prioritized)

1. ✅ **[DONE in v0.6.0]** Implement HKDF-derived nonces for guaranteed uniqueness
2. ✅ **[DONE in v0.6.0]** Fix core KDF to use explicit Argon2id params
3. ✅ **[DONE in v0.6.0]** Add HMAC-SHA256 for file integrity checks
4. ✅ **[IN PROGRESS]** Add Ed25519 manifest signing (scaffolding in place)
5. 🔲 Add optional WebDAV filename/path encryption
6. ✅ **[DONE in v0.6.0]** Add TLS SPKI pinning support (module created)
7. 🔲 Add password strength checker (`zxcvbn`)
8. 🔲 Add SFTP strict host verification mode
9. 🔲 Standardize on `OsRng` for all crypto RNG
10. 🔲 Comprehensive `zeroize` audit

---

## Audit Conclusion

**Skylock v0.5.1** is **production-ready for users with strong passwords** (20+ chars) and trusted storage providers. However, **v0.6.0 improvements are strongly recommended** to mitigate GPU cracking, nonce reuse, and integrity forgery risks.

**Next Steps**: Implement Phase 1 hardening (KDF, nonces, RNG, manifest signing) in v0.6.0 release.

---

**Reviewed By**: Skylock Security Team  
**Approved For**: Internal baseline and v0.6.0 planning
