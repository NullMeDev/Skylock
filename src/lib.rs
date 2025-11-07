//! Skylock Hybrid Library
//! 
//! This module provides access to cross-platform backup and synchronization functionality.

pub mod platform;
pub mod config;
pub mod backup;
pub mod stubs;
pub mod crypto;
pub mod compression;
pub mod deduplication;
pub mod logging;

// Re-export commonly used types
pub use skylock_core::{Result, Error};
pub use stubs::*;
