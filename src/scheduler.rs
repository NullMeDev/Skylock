//! Advanced scheduler with cron expression support
//!
//! Provides flexible scheduling capabilities using standard cron expressions.
//! Supports validation, parsing, and execution time calculation.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use cron::Schedule;
use std::str::FromStr;

/// Validate a cron expression
///
/// # Arguments
/// * `expression` - Cron expression to validate (e.g., "0 2 * * *")
///
/// # Returns
/// * `Ok(())` if valid
/// * `Err` with details if invalid
///
/// # Examples
/// ```
/// validate_cron_expression("0 2 * * *").unwrap(); // Daily at 2 AM
/// validate_cron_expression("*/15 * * * *").unwrap(); // Every 15 minutes
/// ```
pub fn validate_cron_expression(expression: &str) -> Result<()> {
    Schedule::from_str(expression)
        .context(format!("Invalid cron expression: '{}'", expression))?;
    Ok(())
}

/// Parse a cron expression and return the Schedule
pub fn parse_cron_expression(expression: &str) -> Result<Schedule> {
    Schedule::from_str(expression)
        .context(format!("Failed to parse cron expression: '{}'", expression))
}

/// Check if a backup should run now based on the cron schedule
///
/// # Arguments
/// * `schedule` - Cron expression (e.g., "0 2 * * *")
/// * `now` - Current time
/// * `last_run` - Last time the backup ran (None if never run)
///
/// # Returns
/// * `true` if backup should run now
/// * `false` otherwise
pub fn should_run_backup(
    schedule: &str,
    now: DateTime<Utc>,
    last_run: Option<DateTime<Utc>>,
) -> bool {
    let schedule = match Schedule::from_str(schedule) {
        Ok(s) => s,
        Err(_) => return false,
    };

    // Get the next scheduled time after the last run (or a long time ago if never run)
    let reference_time = last_run.unwrap_or_else(|| Utc::now() - chrono::Duration::days(365));
    
    // Find the next scheduled time after the reference
    if let Some(next_time) = schedule.after(&reference_time).next() {
        // Should run if the next scheduled time is in the past or now
        next_time <= now
    } else {
        false
    }
}

/// Get the next scheduled backup time
///
/// # Arguments
/// * `schedule` - Cron expression
/// * `after` - Calculate next run after this time
///
/// # Returns
/// * `Some(DateTime<Utc>)` with next scheduled time
/// * `None` if schedule is invalid or no future runs
pub fn get_next_run(schedule: &str, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let schedule = Schedule::from_str(schedule).ok()?;
    schedule.after(&after).next()
}

/// Get human-readable description of cron schedule
///
/// # Arguments
/// * `expression` - Cron expression
///
/// # Returns
/// * Human-readable description
pub fn describe_schedule(expression: &str) -> String {
    // Simple descriptions for common patterns (6-field format)
    match expression {
        "0 0 * * * *" => "Every hour".to_string(),
        "0 0 */2 * * *" => "Every 2 hours".to_string(),
        "0 0 */6 * * *" => "Every 6 hours".to_string(),
        "0 0 */12 * * *" => "Every 12 hours".to_string(),
        "0 0 0 * * *" | "0 0 2 * * *" => "Daily".to_string(),
        "0 0 2 * * 7" | "0 0 0 * * 7" => "Weekly (Sunday)".to_string(),
        "0 0 2 * * 1" | "0 0 0 * * 1" => "Weekly (Monday)".to_string(),
        "0 0 2 1 * *" | "0 0 0 1 * *" => "Monthly (1st of month)".to_string(),
        _ => {
            // Try to parse and show next run times
            if let Ok(schedule) = Schedule::from_str(expression) {
                let now = Utc::now();
                let next_runs: Vec<_> = schedule
                    .upcoming(Utc)
                    .take(3)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                    .collect();
                
                if !next_runs.is_empty() {
                    format!("Next runs: {}", next_runs.join(", "))
                } else {
                    expression.to_string()
                }
            } else {
                format!("Invalid: {}", expression)
            }
        }
    }
}

/// Common cron expression presets
/// Note: Uses 6-field format (seconds minute hour day month weekday)
pub mod presets {
    /// Every hour at minute 0
    pub const HOURLY: &str = "0 0 * * * *";
    
    /// Every 2 hours
    pub const EVERY_2_HOURS: &str = "0 0 */2 * * *";
    
    /// Every 6 hours
    pub const EVERY_6_HOURS: &str = "0 0 */6 * * *";
    
    /// Every 12 hours
    pub const EVERY_12_HOURS: &str = "0 0 */12 * * *";
    
    /// Daily at 2 AM
    pub const DAILY_2AM: &str = "0 0 2 * * *";
    
    /// Daily at midnight
    pub const DAILY_MIDNIGHT: &str = "0 0 0 * * *";
    
    /// Weekly on Sunday at 2 AM (7 = Sunday)
    pub const WEEKLY_SUNDAY: &str = "0 0 2 * * 7";
    
    /// Weekly on Monday at 2 AM
    pub const WEEKLY_MONDAY: &str = "0 0 2 * * 1";
    
    /// Monthly on the 1st at 2 AM
    pub const MONTHLY_1ST: &str = "0 0 2 1 * *";
    
    /// Every 15 minutes
    pub const EVERY_15_MIN: &str = "0 */15 * * * *";
    
    /// Every 30 minutes
    pub const EVERY_30_MIN: &str = "0 */30 * * * *";
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validate_valid_expressions() {
        assert!(validate_cron_expression("0 0 2 * * *").is_ok());
        assert!(validate_cron_expression("0 */15 * * * *").is_ok());
        assert!(validate_cron_expression("0 0 0 * * 7").is_ok()); // Sunday = 7
        assert!(validate_cron_expression("0 0 2 1 * *").is_ok());
    }
    
    #[test]
    fn test_validate_invalid_expressions() {
        assert!(validate_cron_expression("invalid").is_err());
        assert!(validate_cron_expression("0 60 * * * *").is_err()); // Invalid minute
        assert!(validate_cron_expression("0 2 * * *").is_err()); // Too few fields
    }
    
    #[test]
    fn test_should_run_backup() {
        let now = Utc::now();
        let past = now - chrono::Duration::hours(25);
        
        // Daily at 2 AM - should run if more than 24 hours since last run
        assert!(should_run_backup("0 0 2 * * *", now, Some(past)));
        
        // Should not run if just ran recently
        let recent = now - chrono::Duration::minutes(5);
        assert!(!should_run_backup("0 0 2 * * *", now, Some(recent)));
    }
    
    #[test]
    fn test_get_next_run() {
        let now = Utc::now();
        let next = get_next_run("0 0 2 * * *", now);
        
        assert!(next.is_some());
        let next_time = next.unwrap();
        assert!(next_time > now);
    }
    
    #[test]
    fn test_describe_schedule() {
        assert_eq!(describe_schedule("0 0 * * * *"), "Every hour");
        assert_eq!(describe_schedule("0 0 0 * * *"), "Daily");
        assert_eq!(describe_schedule("0 0 0 * * 7"), "Weekly (Sunday)"); // Sunday = 7
    }
    
    #[test]
    fn test_presets() {
        assert!(validate_cron_expression(presets::HOURLY).is_ok());
        assert!(validate_cron_expression(presets::DAILY_2AM).is_ok());
        assert!(validate_cron_expression(presets::WEEKLY_SUNDAY).is_ok());
        assert!(validate_cron_expression(presets::MONTHLY_1ST).is_ok());
    }
}
