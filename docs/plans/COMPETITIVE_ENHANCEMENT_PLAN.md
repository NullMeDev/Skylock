# Skylock Competitive Enhancement Plan

**Created**: 2025-11-27  
**Status**: Awaiting Approval  
**Author**: AI Assistant

---

## Current State Summary

**Skylock v0.6.1** is a Rust-based encrypted backup system with:
- **~190,000 lines** of code across 203 Rust files
- **6 workspace crates**: skylock-core, skylock-backup, skylock-hetzner, skylock-monitor, skylock-sync, skylock-ui
- **Encryption**: AES-256-GCM / XChaCha20-Poly1305 with Argon2id KDF (64 MiB, t=4)
- **Storage**: Hetzner Storage Box (WebDAV/SFTP), with stubs for AWS, Azure, GCP, Backblaze
- **Features**: Direct upload, incremental backups, GFS retention, file change tracking, bandwidth limiting, resume support, encrypted manifest browsing
- **Security**: Perfect forward secrecy, key rotation, HSM provider interface (recently added)
- **115+ tests** passing in skylock-backup alone

**Key Gaps vs Competitors**:
- No desktop GUI or system tray
- No real-time sync (only scheduled backups)
- No multi-device sync
- No file sharing capabilities
- No mobile apps
- No document collaboration
- Limited storage provider support
- No ransomware detection

---

## Competitor Analysis

### Sync.com
**Strengths**: Zero-knowledge E2E encryption standard (free), $8/mo for 2TB, document collaboration, HIPAA compliance, password-protected links
**Weakness**: Slower sync speeds, no lifetime plans

### pCloud
**Strengths**: Lifetime plans ($199-399), 10GB free, media player, virtual drive, fast sync
**Weakness**: Crypto encryption costs extra ($150), Swiss-based but encryption optional

### iDrive
**Strengths**: Continuous backup, disk image backup, 30 versions, SaaS backup (M365/Google Workspace), physical shipping for recovery
**Weakness**: No zero-knowledge encryption, more backup than sync

### AWS S3/Glacier
**Strengths**: Unlimited scale, lifecycle policies, immutable storage, compliance certifications
**Weakness**: Complex, expensive egress, no client app

### ProtonDrive
**Strengths**: Swiss privacy laws, E2E by default, document collaboration (2024), ISO 27001, Proton ecosystem
**Weakness**: Limited storage (5GB free), newer service, fewer integrations

### Backblaze
**Strengths**: $9/mo unlimited backup, simple, fast, B2 at $6/TB/mo
**Weakness**: No sync, single-computer focus, no folder selection

---

## Differentiation Strategy

**Skylock's Unique Position**: "Open-source, self-hosted capable, zero-knowledge backup with enterprise security for privacy-conscious users"

**Key Differentiators**:
1. **Open Source** - Full transparency, auditable code (unlike Sync.com, pCloud, iDrive)
2. **Self-Hosted Option** - Bring your own storage (Hetzner, S3, local NAS)
3. **Military-Grade Crypto** - XChaCha20-Poly1305 + Argon2id + PFS (stronger than most)
4. **Rust Performance** - Memory-safe, fast, no GC pauses
5. **Modular Architecture** - Use only what you need

---

## 20+ Proposed Improvements

### Tier 1: Critical Competitive Features (High Impact)

#### 1. Multi-Provider Storage Support
**What**: Complete AWS S3, Backblaze B2, Google Cloud, Azure, local NAS, SFTP support
**Why**: Users want choice; pCloud/Sync.com lock you in
**Effort**: Medium (stubs exist)

#### 2. Real-Time File Sync (inotify/FSEvents)
**What**: Watch directories and upload changes immediately
**Why**: iDrive has continuous backup; Sync.com syncs instantly
**Effort**: Medium (notify crate ready)

#### 3. Desktop GUI with System Tray
**What**: Native GUI (egui/iced) with system tray, progress, notifications
**Why**: All competitors have GUI; CLI-only limits adoption
**Effort**: High

#### 4. Block-Level Deduplication
**What**: Content-defined chunking (CDC) with rolling hashes
**Why**: Reduces storage 60-80% for similar files; restic/borg have this
**Effort**: High

#### 5. Ransomware Detection & Protection
**What**: Detect mass encryption, immutable backups, snapshot isolation
**Why**: Top requested enterprise feature; Acronis, IDrive highlight this
**Effort**: Medium

#### 6. Secure File Sharing
**What**: Password-protected links, expiry dates, download limits, upload requests
**Why**: Sync.com's killer feature; essential for collaboration
**Effort**: Medium

### Tier 2: Competitive Parity Features (Medium Impact)

#### 7. Mobile Apps (iOS/Android)
**What**: Flutter or React Native app for backup/restore/browse
**Why**: All competitors have mobile; essential for photos backup
**Effort**: High

#### 8. SaaS Backup (M365, Google Workspace)
**What**: Backup email, Drive, OneDrive, SharePoint
**Why**: iDrive's differentiator; growing enterprise need
**Effort**: High (OAuth, APIs)

#### 9. Virtual Drive / FUSE Mount
**What**: Mount backups as read-only filesystem
**Why**: pCloud's virtual drive is beloved; Cryptomator users expect this
**Effort**: Medium

#### 10. Disk Image Backup
**What**: Full system image with bare-metal restore
**Why**: iDrive Mirror feature; essential for disaster recovery
**Effort**: High

#### 11. Two-Factor Authentication
**What**: TOTP, hardware keys (FIDO2), recovery codes
**Why**: All enterprise services require this
**Effort**: Low-Medium

#### 12. Web Dashboard (Complete)
**What**: Finish the React dashboard with real-time WebSocket
**Why**: Design spec exists; provides remote management
**Effort**: Medium (partially done)

### Tier 3: Innovation Features (Differentiation)

#### 13. Zero-Knowledge File Search
**What**: Client-side index with encrypted keyword search
**Why**: No competitor offers searchable E2E encrypted storage
**Effort**: High (research)

#### 14. Decentralized Backup Network
**What**: Optional peer-to-peer backup to friends' nodes
**Why**: Unique; Storj/Filecoin concepts but simpler
**Effort**: Very High

#### 15. AI-Powered Smart Backup
**What**: ML to predict important files, optimize schedules, detect anomalies
**Why**: No competitor does this; modern differentiator
**Effort**: High

#### 16. Offline-First Conflict Resolution
**What**: Git-like merge for document conflicts
**Why**: ProtonDrive has collaboration; better conflict handling needed
**Effort**: Medium

#### 17. Time Machine-Style UI
**What**: Visual timeline to browse backup history
**Why**: Apple Time Machine UX is gold standard
**Effort**: Medium

### Tier 4: Polish & Ecosystem

#### 18. Plugin System
**What**: Hooks for custom storage providers, notifiers, processors
**Why**: Extensibility without core changes
**Effort**: Medium

#### 19. Prometheus/Grafana Integration
**What**: Metrics export, alerting, dashboards
**Why**: Enterprise monitoring requirement
**Effort**: Low

#### 20. Delta Encoding / Binary Diff
**What**: Upload only changed bytes, not whole files
**Why**: Reduces bandwidth 90%+ for large files
**Effort**: Medium

#### 21. Multi-Account / Team Management
**What**: Admin console, user roles, shared folders
**Why**: Sync.com Teams, pCloud Business have this
**Effort**: High

#### 22. Hardware Security Key Support
**What**: YubiKey, Ledger for key storage
**Why**: HSM provider interface exists; add PKCS#11 implementation
**Effort**: Medium

#### 23. Post-Quantum Cryptography
**What**: Optional Kyber/Dilithium hybrid encryption
**Why**: Future-proof; no competitor has this yet
**Effort**: Medium (libraries exist)

#### 24. Container Backup
**What**: Docker/Podman volume backup with consistent snapshots
**Why**: DevOps audience; unique feature
**Effort**: Medium

#### 25. Database Backup Agents
**What**: Postgres, MySQL, MongoDB consistent backup plugins
**Why**: Enterprise requirement; iDrive has SQL Server
**Effort**: Medium

---

## Implementation Priority Matrix

| Feature | Impact | Effort | Priority |
|---------|--------|--------|----------|
| Multi-Provider Storage | High | Medium | **P0** |
| Desktop GUI + Tray | High | High | **P0** |
| Real-Time Sync | High | Medium | **P1** |
| Secure File Sharing | High | Medium | **P1** |
| Block-Level Dedup | High | High | **P1** |
| Ransomware Detection | High | Medium | **P1** |
| 2FA | Medium | Low | **P2** |
| Web Dashboard | Medium | Medium | **P2** |
| Virtual Drive | Medium | Medium | **P2** |
| Delta Encoding | Medium | Medium | **P2** |
| Mobile Apps | High | High | **P3** |
| Disk Image Backup | Medium | High | **P3** |
| SaaS Backup | Medium | High | **P3** |

---

## Recommended Roadmap

### Phase 1: Core Competitive (4-6 weeks)
1. Complete AWS S3 and Backblaze B2 providers
2. Add real-time file watching with debouncing
3. Implement secure file sharing with link generation
4. Add ransomware detection (entropy analysis, rate limiting)

### Phase 2: User Experience (6-8 weeks)
5. Build desktop GUI with egui/iced
6. Implement system tray with notifications
7. Complete web dashboard
8. Add 2FA (TOTP)

### Phase 3: Performance (4-6 weeks)
9. Implement CDC block-level deduplication
10. Add delta encoding for binary files
11. Optimize parallel operations further

### Phase 4: Advanced (8-12 weeks)
12. Mobile app (start with read-only browse/restore)
13. Virtual drive mount (FUSE)
14. Plugin system architecture

---

## Success Metrics

- **Adoption**: 1,000 GitHub stars, 100 active users
- **Performance**: 200 MB/s throughput, <100ms small file latency
- **Security**: Zero CVEs, pass external audit
- **Reliability**: 99.9% backup success rate
- **User Satisfaction**: >4.5 star reviews

---

## Open Questions

1. Should we prioritize GUI (broader adoption) or dedup (power users)?
2. License: Stay MIT or consider AGPL for SaaS protection?
3. Cloud hosting: Offer managed Skylock service?
4. Funding: Open Collective, GitHub Sponsors, commercial license?

---

*This plan is awaiting user approval before implementation begins.*
