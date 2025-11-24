# Security Fix Game Plan - Skylock v0.6.0 → v0.7.0
**Created**: 2025-01-24  
**Priority**: CRITICAL  
**Target Completion**: 30 days  
**Total Issues**: 47 (8 Critical, 15 High, 18 Medium, 6 Low)

---

## 🎯 EXECUTIVE SUMMARY

This game plan addresses all 47 security issues identified in the comprehensive security audit. Issues are organized by priority and dependencies, with clear timelines and success criteria.

**Critical Path**: Fix CRIT-001 (nonce reuse) FIRST - blocks all other encryption work.

---

## 📅 PHASE 1: CRITICAL FIXES (Days 1-7)

### Day 1: Nonce Reuse Fix (HIGHEST PRIORITY)
**Issue**: CRIT-001 - Nonce reuse in XChaCha20-Poly1305  
**Risk**: Complete encryption compromise  
**Effort**: 4-6 hours

**Tasks**:
1. ✅ Review current nonce generation in `skylock-core/src/encryption.rs:218-221`
2. ✅ Implement HKDF-derived nonces tied to chunk index
3. ✅ Add `chunk_index` parameter to `encrypt_block()` and `decrypt_block()`
4. ✅ Update all callers to pass chunk_index
5. ✅ Add test: encrypt same data 10,000 times, verify all nonces unique
6. ✅ Run full test suite
7. ✅ Document nonce derivation algorithm

**Code Changes**:
- `skylock-core/src/encryption.rs`: Add HKDF nonce derivation
- `skylock-core/src/encryption.rs`: Modify encrypt_block signature
- `skylock-core/src/encryption.rs`: Modify decrypt_block signature
- All callers: Add chunk index tracking

**Success Criteria**:
- [ ] All nonces are cryptographically unique (statistical test)
- [ ] Backward compatibility maintained (detect old format)
- [ ] Performance impact < 1% (benchmark)
- [ ] All tests pass

---

### Day 2: Memory Safety - Zeroization
**Issue**: CRIT-003 - Missing zeroization on BlockKey  
**Risk**: Key material leaks in memory dumps  
**Effort**: 3-4 hours

**Tasks**:
1. ✅ Add `zeroize` crate to dependencies (already present)
2. ✅ Add `#[derive(Zeroize, ZeroizeOnDrop)]` to BlockKey
3. ✅ Add `#[zeroize(skip)]` to non-secret fields
4. ✅ Review all key-holding structs for zeroization
5. ✅ Add custom Debug impl to redact secrets (CRIT-002)
6. ✅ Test memory wiping with valgrind/miri

**Files**:
- `skylock-core/src/encryption.rs`: BlockKey struct
- `skylock-backup/src/encryption.rs`: Check for key structs
- `src/crypto/signatures.rs`: SecureSigningKey (already has zeroize)

**Success Criteria**:
- [ ] BlockKey zeroized on drop (verified with test)
- [ ] Debug output never shows secret material
- [ ] Memory analysis shows no key remnants

---

### Day 3: Debug Output Sanitization
**Issue**: CRIT-002 - Secret material in debug output  
**Risk**: Key leakage through logs  
**Effort**: 2-3 hours

**Tasks**:
1. ✅ Remove `Debug` derive from BlockKey (or custom impl)
2. ✅ Audit all `#[derive(Debug)]` on secret-holding structs
3. ✅ Implement custom Debug for sensitive structs
4. ✅ Search codebase for `println!("{:?}", ...)` on secrets
5. ✅ Add lint rule to prevent Debug on secrets
6. ✅ Test: ensure no secrets in debug output

**Structs to Fix**:
- `BlockKey`
- `SecureKey`
- `EncryptionManager` (already done)
- `FileKeyStore` (already done)
- Any password/credential holders

**Success Criteria**:
- [ ] No struct containing secrets has auto-derived Debug
- [ ] All custom Debug impls redact secrets
- [ ] Grep for "REDACTED" confirms sanitization

---

### Day 4: DoS Protection - Memory Limits
**Issue**: CRIT-004 - Unbounded memory growth in decrypt_file  
**Risk**: OOM crash via malicious input  
**Effort**: 3-4 hours

**Tasks**:
1. ✅ Add file size limit (10GB) to decrypt_file
2. ✅ Fix hasher accumulation - use per-chunk hashing
3. ✅ Add progress reporting for large files
4. ✅ Add memory limit check at start
5. ✅ Test with large file (9GB, 11GB)
6. ✅ Document limits in docs/

**Files**:
- `skylock-core/src/encryption.rs:349-377` (decrypt_file)
- `skylock-core/src/encryption.rs:277-328` (encrypt_file)

**Success Criteria**:
- [ ] Files >10GB rejected with clear error
- [ ] Memory usage stays constant regardless of file size
- [ ] Performance unchanged for normal files

---

### Day 5: Input Validation - Path Traversal
**Issue**: CRIT-008 - Directory traversal in manifest paths  
**Risk**: Write files outside backup directory  
**Effort**: 4-5 hours

**Tasks**:
1. ✅ Create `validate_path()` function
2. ✅ Reject absolute paths
3. ✅ Reject `..` components
4. ✅ Reject drive letters (Windows)
5. ✅ Canonicalize and verify within base dir
6. ✅ Apply validation to all path inputs
7. ✅ Add comprehensive path traversal tests

**Files**:
- `skylock-backup/src/direct_upload.rs`: Add validation
- `skylock-backup/src/lib.rs`: path validation module
- `src/restore/mod.rs`: Apply validation on restore

**Test Cases**:
- `../etc/passwd` → reject
- `/etc/passwd` → reject
- `C:\Windows\System32\...` → reject
- `normal/path/file.txt` → accept
- Symlinks → reject

**Success Criteria**:
- [ ] All path traversal attempts blocked
- [ ] Symlinks not followed
- [ ] Tests cover 20+ malicious path patterns

---

### Day 6: Timing Attack Protection
**Issue**: CRIT-005 - Tag verification timing leak  
**Risk**: Timing attack on AEAD verification  
**Effort**: 3-4 hours

**Tasks**:
1. ✅ Import `subtle::ConstantTimeEq`
2. ✅ Replace all `==` comparisons on secrets
3. ✅ Add constant-time delay on decrypt failure
4. ✅ Audit all crypto verification paths
5. ✅ Add timing test (measure variance)

**Files**:
- `skylock-core/src/encryption.rs`: decrypt_block
- `skylock-backup/src/hmac_integrity.rs`: HMAC verification
- `skylock-backup/src/manifest_signing.rs`: Signature verification

**Success Criteria**:
- [ ] All secret comparisons use constant-time
- [ ] Timing variance <5% between valid/invalid
- [ ] No early returns on verification failure

---

### Day 7: Error Handling - Remove unwrap()
**Issue**: CRIT-006 - Excessive unwrap() in production  
**Risk**: Panic on malformed input → DoS  
**Effort**: 6-8 hours

**Tasks**:
1. ✅ Identify all unwrap() calls in production code (grep results)
2. ✅ Priority files:
   - `skylock-backup/src/direct_upload.rs` (16 calls)
   - `skylock-backup/src/encryption.rs` (11 calls)
   - `src/crypto/signatures.rs` (23 calls - mostly tests)
   - `src/crypto/rsa_keys.rs` (24 calls)
3. ✅ Replace with `?` or `unwrap_or_else()`
4. ✅ Add fallback for template strings
5. ✅ Document remaining unwrap() in tests

**Success Criteria**:
- [ ] Zero unwrap() in production code paths
- [ ] All template strings have fallbacks
- [ ] Fuzzing reveals no panics

---

## 📅 PHASE 2: HIGH PRIORITY (Days 8-14)

### Day 8: Rate Limiting
**Issue**: HIGH-002 - No rate limiting on failed auth  
**Risk**: Online brute-force attack  
**Effort**: 4-5 hours

**Tasks**:
1. ✅ Create RateLimiter struct
2. ✅ Max 5 attempts per hour per identifier
3. ✅ Exponential backoff on repeated failures
4. ✅ Store attempts in-memory (tokio::sync::Mutex)
5. ✅ Add cleanup for old attempts (>1 hour)
6. ✅ Integrate with EncryptionManager::new()
7. ✅ Test: verify backoff increases exponentially

**Files**:
- `skylock-core/src/security/rate_limiter.rs` (new)
- `skylock-core/src/encryption.rs`: Apply rate limiting

**Success Criteria**:
- [ ] 6th attempt in 1 hour triggers backoff
- [ ] Backoff increases: 2s, 4s, 8s, 16s...
- [ ] Rate limiter thread-safe

---

### Day 9: Constant-Time Comparisons
**Issue**: HIGH-001 - Missing constant-time for secrets  
**Risk**: Timing attack on HMAC/signatures  
**Effort**: 2-3 hours

**Tasks**:
1. ✅ Audit all `==` comparisons on hashes/MACs/signatures
2. ✅ Replace with `ct_eq()` from `subtle` crate
3. ✅ Files to fix:
   - `skylock-backup/src/hmac_integrity.rs`
   - `skylock-backup/src/manifest_signing.rs`
   - Any fingerprint comparisons

**Success Criteria**:
- [ ] All secret comparisons constant-time
- [ ] Timing test shows <5% variance

---

### Day 10: File Permissions Hardening
**Issue**: HIGH-003 - Weak permissions on key storage  
**Risk**: Key theft by local attacker  
**Effort**: 3-4 hours

**Tasks**:
1. ✅ Set 0600 (Unix) on all key files
2. ✅ Set FILE_ATTRIBUTE_ENCRYPTED (Windows)
3. ✅ Apply to:
   - `~/.local/share/skylock/keys/**`
   - Config files with secrets
4. ✅ Test: verify permissions after creation
5. ✅ Check parent directory permissions

**Files**:
- `skylock-core/src/encryption.rs:448-481` (store_block_key)
- `src/crypto/signatures.rs`: Key saving

**Success Criteria**:
- [ ] Unix: `ls -la` shows 0600
- [ ] Windows: encrypted attribute set
- [ ] Parent dirs have 0700

---

### Day 11: Config Integrity Checks
**Issue**: HIGH-004 - No integrity check on config  
**Risk**: Malicious config modification  
**Effort**: 4-5 hours

**Tasks**:
1. ✅ Create SignedKeyMetadata struct
2. ✅ Add HMAC-SHA256 signature field
3. ✅ Sign metadata on creation
4. ✅ Verify signature on load
5. ✅ Use constant-time comparison
6. ✅ Test: tampered config → rejected

**Files**:
- `skylock-core/src/encryption.rs:161-208`
- New: `skylock-core/src/security/config_integrity.rs`

**Success Criteria**:
- [ ] Config tampering detected
- [ ] Signature verification uses constant-time
- [ ] Backward compat: unsigned configs get signed

---

### Day 12: Input Validation - Backup Paths
**Issue**: HIGH-005 - Missing validation on backup paths  
**Risk**: Symlink attack, TOCTOU race  
**Effort**: 3-4 hours

**Tasks**:
1. ✅ Don't follow symlinks in WalkDir
2. ✅ Set max_depth(100) to prevent deep recursion
3. ✅ Skip special files (sockets, fifos)
4. ✅ Canonicalize paths before use
5. ✅ Verify within allowed directories
6. ✅ Test: symlink attack rejected

**Files**:
- `skylock-backup/src/direct_upload.rs:321-341`

**Success Criteria**:
- [ ] Symlinks not followed
- [ ] Max depth enforced
- [ ] Special files skipped

---

### Day 13: Password Protection
**Issue**: CRIT-007 - Password in process args  
**Risk**: Password visible in ps/proc  
**Effort**: 3-4 hours

**Tasks**:
1. ✅ Add `secrecy` crate dependency
2. ✅ Change signature: `password: Secret<String>`
3. ✅ Use `.expose_secret()` only when needed
4. ✅ Update all callers
5. ✅ Test: `ps aux` doesn't show password

**Files**:
- `skylock-core/src/encryption.rs:107`
- All functions taking passwords as `&str`

**Success Criteria**:
- [ ] Password never in plain String in memory
- [ ] Auto-zeroized on drop
- [ ] Not visible in process listing

---

### Day 14: Integration Testing
**Effort**: 4-6 hours

**Tasks**:
1. ✅ Run full test suite with all fixes
2. ✅ Add security-specific tests
3. ✅ Benchmark performance impact
4. ✅ Test backward compatibility
5. ✅ Update documentation

**Success Criteria**:
- [ ] All tests pass
- [ ] Performance degradation <5%
- [ ] Old backups still restore correctly

---

## 📅 PHASE 3: MEDIUM PRIORITY (Days 15-21)

### Day 15-16: Manifest Signature Verification
**Issue**: MED-001 - Signature not verified before use  
**Effort**: 4-5 hours

**Tasks**:
1. ✅ Parse signature FIRST
2. ✅ Verify before deserializing full manifest
3. ✅ Use constant-time comparison
4. ✅ Add test: tampered manifest rejected

---

### Day 17: Integer Overflow Protection
**Issue**: MED-002 - Integer overflow in size calc  
**Effort**: 2-3 hours

**Tasks**:
1. ✅ Use `checked_add()` for all size calculations
2. ✅ Use `try_fold()` instead of `sum()`
3. ✅ Test with SIZE_MAX

---

### Day 18: Network Timeouts
**Issue**: MED-003 - No timeout on network ops  
**Effort**: 2-3 hours

**Tasks**:
1. ✅ Add timeout to reqwest Client (300s)
2. ✅ Add connect_timeout (30s)
3. ✅ Add pool_idle_timeout (60s)
4. ✅ Test: slow server → timeout

---

### Day 19-20: Performance Optimizations
**Issues**: PERF-001, PERF-002  
**Effort**: 4-5 hours

**Tasks**:
1. ✅ Fix hash cloning in decrypt_file
2. ✅ Use HashMap entry API to avoid double lookup
3. ✅ Benchmark improvements

---

### Day 21: Code Review & Documentation
**Effort**: 4-6 hours

**Tasks**:
1. ✅ Review all changes
2. ✅ Update CHANGELOG.md
3. ✅ Update security docs
4. ✅ Create migration guide

---

## 📅 PHASE 4: LOW PRIORITY & ENHANCEMENTS (Days 22-30)

### Day 22-24: Fuzzing Infrastructure
**Effort**: 8-10 hours

**Tasks**:
1. ✅ Set up cargo-fuzz
2. ✅ Create fuzzing targets:
   - Manifest parsing
   - Encryption/decryption
   - Path validation
3. ✅ Run fuzzing overnight
4. ✅ Fix any crashes found

---

### Day 25-26: Security Test Suite
**Effort**: 6-8 hours

**Tasks**:
1. ✅ Nonce uniqueness test (10,000 encryptions)
2. ✅ Timing attack resistance test
3. ✅ Path traversal test (20+ patterns)
4. ✅ Memory leak test (valgrind)
5. ✅ Zeroization test

---

### Day 27-28: CI/CD Security Checks
**Effort**: 4-6 hours

**Tasks**:
1. ✅ Add `cargo audit` to CI
2. ✅ Add `cargo deny` with deny.toml
3. ✅ Add security linters
4. ✅ Enable Dependabot
5. ✅ Add SAST scanning

---

### Day 29-30: Final Review & Release
**Effort**: 6-8 hours

**Tasks**:
1. ✅ Complete security checklist
2. ✅ Run full test suite
3. ✅ Performance benchmarks
4. ✅ Update version to v0.7.0
5. ✅ Create release notes
6. ✅ Tag and release

---

## 📋 TRACKING CHECKLIST

### Critical Issues (Must Complete)
- [ ] CRIT-001: Nonce reuse fixed
- [ ] CRIT-002: Debug output sanitized
- [ ] CRIT-003: Zeroization added
- [ ] CRIT-004: Memory limits enforced
- [ ] CRIT-005: Timing attacks prevented
- [ ] CRIT-006: unwrap() removed
- [ ] CRIT-007: Password protection added
- [ ] CRIT-008: Path traversal blocked

### High Priority Issues (Should Complete)
- [ ] HIGH-001: Constant-time comparisons
- [ ] HIGH-002: Rate limiting
- [ ] HIGH-003: File permissions
- [ ] HIGH-004: Config integrity
- [ ] HIGH-005: Input validation

### Medium Priority Issues (Nice to Have)
- [ ] MED-001: Manifest verification order
- [ ] MED-002: Integer overflow checks
- [ ] MED-003: Network timeouts

### Enhancements
- [ ] Fuzzing infrastructure
- [ ] Security test suite
- [ ] CI/CD security checks

---

## 🔍 TESTING STRATEGY

### Unit Tests (Per Feature)
- Test normal operation
- Test edge cases
- Test malicious input
- Test error handling

### Integration Tests (End-to-End)
- Full backup/restore with all fixes
- Backward compatibility
- Performance benchmarks
- Security scenarios

### Security Tests (Specific)
- Nonce uniqueness (statistical)
- Timing attack resistance
- Path traversal attempts
- Memory leak detection
- Fuzzing (continuous)

---

## 📊 SUCCESS METRICS

### Code Quality
- [ ] Zero unwrap() in production code
- [ ] 100% test coverage on security-critical paths
- [ ] All clippy warnings resolved
- [ ] No secrets in debug output

### Security Posture
- [ ] All CRITICAL issues resolved
- [ ] All HIGH issues resolved
- [ ] 90%+ MEDIUM issues resolved
- [ ] Fuzzing runs 24h without crashes

### Performance
- [ ] Encryption performance degradation <5%
- [ ] Memory usage unchanged
- [ ] Network overhead <2%

### Compliance
- [ ] OWASP Top 10: Fully compliant
- [ ] NIST SP 800-175B: Fully compliant
- [ ] CWE-Top 25: Zero issues

---

## 🚨 RISK MITIGATION

### If Timeline Slips
**Priority 1**: CRIT-001, CRIT-003, CRIT-008 (encryption core)  
**Priority 2**: CRIT-006, HIGH-002 (DoS prevention)  
**Priority 3**: Everything else

### Rollback Plan
- Keep v0.6.0 tag for emergency rollback
- Document all breaking changes
- Provide migration script
- Test downgrade path

---

## 📝 DOCUMENTATION UPDATES

### Files to Update
- [ ] CHANGELOG.md (v0.7.0 entry)
- [ ] README.md (security features)
- [ ] docs/security/SECURITY.md (architecture)
- [ ] docs/SECURITY_ADVISORY_0.7.0.md (new advisory)
- [ ] WARP.md (development guide)

### Migration Guide
- [ ] Breaking changes list
- [ ] Upgrade procedure
- [ ] Backward compatibility notes
- [ ] New security features explanation

---

## 🎯 RELEASE CRITERIA (v0.7.0)

**Must Have**:
- ✅ All 8 CRITICAL issues fixed
- ✅ All 15 HIGH issues fixed
- ✅ Test suite passes 100%
- ✅ Performance benchmarks acceptable
- ✅ Documentation complete

**Nice to Have**:
- ✅ 90%+ MEDIUM issues fixed
- ✅ Fuzzing infrastructure operational
- ✅ CI/CD security checks enabled

**Blockers**:
- ❌ Any CRITICAL issue unresolved
- ❌ Backward compatibility broken
- ❌ Performance degradation >10%

---

## 📞 CONTACTS & RESOURCES

**Security Audit Report**: `docs/security/SECURITY_AUDIT_COMPREHENSIVE.md`  
**Issue Tracker**: GitHub Issues with `security` label  
**Contact**: null@nullme.lol  

**External Resources**:
- NIST SP 800-175B (Key Management)
- RFC 8032 (Ed25519)
- OWASP Top 10 (2021)
- Rust Cryptography Guidelines

---

**Game Plan Version**: 1.0  
**Last Updated**: 2025-01-24  
**Next Review**: After Phase 1 completion (Day 7)
