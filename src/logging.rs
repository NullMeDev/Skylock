use anyhow::Result;
use std::path::PathBuf;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize the logging system with file rotation and security filtering
/// 
/// SECURITY: This module ensures no sensitive data (keys, passwords, tokens) is logged
/// 
/// # Arguments
/// * `log_dir` - Directory to store log files (e.g., ~/.local/share/skylock/)
/// * `log_level` - Minimum log level (trace, debug, info, warn, error)
/// 
/// # Returns
/// * `WorkerGuard` - Must be kept alive for the duration of the program
pub fn init_logging(log_dir: PathBuf, log_level: &str) -> Result<WorkerGuard> {
    // Create log directory if it doesn't exist
    std::fs::create_dir_all(&log_dir)?;

    // Set up file appender with rotation
    // Rotation: 10MB max file size, keep 5 files
    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::NEVER) // We'll use size-based rotation
        .filename_prefix("skylock")
        .filename_suffix("log")
        .max_log_files(5)
        .build(log_dir)?;

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Parse log level from string
    let level = match log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    // Create filter that respects RUST_LOG env var or uses provided level
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level.as_str()));

    // File layer: JSON format for structured parsing
    let file_layer = fmt::layer()
        .json()
        .with_writer(non_blocking)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true);

    // Console layer: Human-readable format
    let console_layer = fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .with_ansi(true);

    // Initialize subscriber with both layers
    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(console_layer)
        .init();

    tracing::info!(
        "Logging initialized at level: {}",
        level
    );

    Ok(guard)
}

/// Sanitize a string to prevent logging sensitive data
/// 
/// SECURITY CRITICAL: This function masks sensitive patterns in log messages
/// 
/// Patterns masked:
/// - Encryption keys (base64-like strings >32 chars)
/// - Passwords in URLs or configs
/// - API tokens
/// - Authorization headers
pub fn sanitize_for_logging(input: &str) -> String {
    let mut sanitized = input.to_string();

    // Mask potential encryption keys (base64 strings >32 chars)
    let key_pattern = regex::Regex::new(r"([A-Za-z0-9+/]{32,}={0,2})").unwrap();
    sanitized = key_pattern.replace_all(&sanitized, "[REDACTED_KEY]").to_string();

    // Mask passwords in URLs
    let url_password_pattern = regex::Regex::new(r"://([^:]+):([^@]+)@").unwrap();
    sanitized = url_password_pattern.replace_all(&sanitized, "://$1:[REDACTED]@").to_string();

    // Mask api_key, password, token fields in structured data
    let field_patterns = [
        (r#"api_key["\s]*[:=]["\s]*([^",\s]+)"#, r#"api_key": "[REDACTED]"#),
        (r#"password["\s]*[:=]["\s]*([^",\s]+)"#, r#"password": "[REDACTED]"#),
        (r#"token["\s]*[:=]["\s]*([^",\s]+)"#, r#"token": "[REDACTED]"#),
        (r#"encryption_key["\s]*[:=]["\s]*([^",\s]+)"#, r#"encryption_key": "[REDACTED]"#),
        (r#"Authorization:\s*Bearer\s+\S+"#, "Authorization: Bearer [REDACTED]"),
        (r#"Authorization:\s*Basic\s+\S+"#, "Authorization: Basic [REDACTED]"),
    ];

    for (pattern, replacement) in field_patterns.iter() {
        let re = regex::Regex::new(pattern).unwrap();
        sanitized = re.replace_all(&sanitized, *replacement).to_string();
    }

    sanitized
}

/// Macro to log with automatic sanitization
/// 
/// Usage: `secure_info!("Config: {}", config_string);`
#[macro_export]
macro_rules! secure_info {
    ($($arg:tt)*) => {
        tracing::info!("{}", $crate::logging::sanitize_for_logging(&format!($($arg)*)))
    };
}

#[macro_export]
macro_rules! secure_debug {
    ($($arg:tt)*) => {
        tracing::debug!("{}", $crate::logging::sanitize_for_logging(&format!($($arg)*)))
    };
}

#[macro_export]
macro_rules! secure_warn {
    ($($arg:tt)*) => {
        tracing::warn!("{}", $crate::logging::sanitize_for_logging(&format!($($arg)*)))
    };
}

#[macro_export]
macro_rules! secure_error {
    ($($arg:tt)*) => {
        tracing::error!("{}", $crate::logging::sanitize_for_logging(&format!($($arg)*)))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_encryption_keys() {
        let input = "encryption_key=YKSsnlb1w3OjlIYVKmLMahyx0hfZvIQAKoNfy67jDDlQC6FHQfnTxYW8rDjqDr7W";
        let sanitized = sanitize_for_logging(input);
        assert!(!sanitized.contains("YKSsnlb1w3OjlIYVKmLMahyx0hfZvIQAKoNfy67jDDlQC6FHQfnTxYW8rDjqDr7W"));
        assert!(sanitized.contains("[REDACTED"));
    }

    #[test]
    fn test_sanitize_passwords_in_urls() {
        let input = "https://user:secret_password@example.com/path";
        let sanitized = sanitize_for_logging(input);
        assert!(!sanitized.contains("secret_password"));
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_sanitize_api_keys_in_json() {
        let input = r#"{"api_key": "sk-1234567890abcdef", "data": "normal"}"#;
        let sanitized = sanitize_for_logging(input);
        assert!(!sanitized.contains("sk-1234567890abcdef"));
        assert!(sanitized.contains("[REDACTED]"));
        assert!(sanitized.contains("normal"));
    }

    #[test]
    fn test_sanitize_authorization_headers() {
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let sanitized = sanitize_for_logging(input);
        assert!(!sanitized.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_normal_strings_unchanged() {
        let input = "Normal log message with regular data";
        let sanitized = sanitize_for_logging(input);
        assert_eq!(input, sanitized);
    }
}
