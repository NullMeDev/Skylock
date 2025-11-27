//! Security Audit Logging Module
//!
//! Provides comprehensive audit logging for security-relevant events.
//!
//! ## Features
//! - Structured audit events with timestamps and metadata
//! - Tamper-evident logging with HMAC integrity
//! - Multiple output backends (file, syslog, remote)
//! - Async, non-blocking logging
//! - Event filtering and severity levels

pub mod events;
pub mod logger;
pub mod storage;

pub use events::{AuditEvent, AuditEventType, EventSeverity, EventOutcome};
pub use logger::{AuditLogger, AuditLoggerConfig};
pub use storage::{AuditStorage, FileAuditStorage};
