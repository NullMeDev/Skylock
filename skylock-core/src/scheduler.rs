use chrono::{DateTime, Utc, Timelike, Datelike};
use crate::{
    Result,
    Error,
    error_types::SystemErrorType,
    ErrorCategory,
    ErrorSeverity,
};

#[derive(Debug, Clone)]
pub struct CronExpression {
    minute: CronField,
    hour: CronField,
    day_of_month: CronField,
    month: CronField,
    day_of_week: CronField,
}

#[derive(Debug, Clone)]
enum CronField {
    All,
    Single(u32),
    List(Vec<u32>),
    Range { start: u32, end: u32 },
    Step { start: u32, step: u32 },
}

impl CronExpression {
    pub fn new(expression: &str) -> Result<Self> {
        let parts: Vec<&str> = expression.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(Error::new(
                ErrorCategory::System(SystemErrorType::InvalidConfiguration),
                ErrorSeverity::High,
                format!("Invalid cron expression: expected 5 fields, got {}", parts.len()),
                "scheduler".to_string(),
            ).into());
        }

        Ok(Self {
            minute: Self::parse_field(parts[0], 0, 59)?,
            hour: Self::parse_field(parts[1], 0, 23)?,
            day_of_month: Self::parse_field(parts[2], 1, 31)?,
            month: Self::parse_field(parts[3], 1, 12)?,
            day_of_week: Self::parse_field(parts[4], 0, 6)?,
        })
    }

    fn parse_field(field: &str, min: u32, max: u32) -> Result<CronField> {
        if field == "*" {
            return Ok(CronField::All);
        }

        if let Ok(value) = field.parse::<u32>() {
            if value >= min && value <= max {
                return Ok(CronField::Single(value));
            }
        }

        if field.contains(',') {
            let values: std::result::Result<Vec<u32>, _> = field
                .split(',')
                .map(|s| s.parse::<u32>().map_err(|_| Error::new(
                    ErrorCategory::System(SystemErrorType::InvalidConfiguration),
                    ErrorSeverity::High,
                    format!("Invalid number in list: {}", s),
                    "scheduler".to_string(),
                )))
                .collect();

            let values = values.map_err(|e| e)?;
            if values.iter().all(|&v| v >= min && v <= max) {
                return Ok(CronField::List(values));
            }
        }

        if field.contains('-') {
            let parts: Vec<&str> = field.split('-').collect();
            if parts.len() == 2 {
                if let (Ok(start), Ok(end)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    if start >= min && end <= max && start <= end {
                        return Ok(CronField::Range { start, end });
                    }
                }
            }
        }

        if field.contains('/') {
            let parts: Vec<&str> = field.split('/').collect();
            if parts.len() == 2 {
                let start = if parts[0] == "*" {
                    min
                } else {
                    parts[0].parse::<u32>().map_err(|_| Error::system(
                        SystemErrorType::InternalError,
                        format!("Invalid start value in step: {}", parts[0]),
                        "scheduler".to_string(),
                    ))?
                };

                let step = parts[1].parse::<u32>().map_err(|_| Error::system(
                    SystemErrorType::InternalError,
                    format!("Invalid step value: {}", parts[1]),
                    "scheduler".to_string(),
                ))?;

                if start >= min && start <= max && step > 0 {
                    return Ok(CronField::Step { start, step });
                }
            }
        }

        Err(Error::system(
            SystemErrorType::InternalError,
            format!("Invalid cron field: {}", field),
            "scheduler".to_string(),
        ).into())
    }

    pub fn next_occurrence(&self, after: DateTime<Utc>) -> Result<DateTime<Utc>> {
        let mut current = after;
        loop {
            if self.matches_time(current)? {
                return Ok(current);
            }
            current = match current.checked_add_signed(chrono::Duration::minutes(1)) {
                Some(next_time) => next_time,
                _none => return Err(Error::system(
                    SystemErrorType::InternalError,
                    format!("Datetime overflow occurred while calculating next occurrence. Current time: {}", current),
                    "scheduler".to_string(),
                ).into()),
            };
            // Prevent infinite loops
            if current > after + chrono::Duration::days(366) {
                return Err(Error::system(
                    SystemErrorType::InternalError,
                    "No valid occurrence found within one year".to_string(),
                    "scheduler".to_string(),
                ).into());
            }
        }
    }

    fn matches_time(&self, time: DateTime<Utc>) -> Result<bool> {
        Ok(
            self.matches_field(&self.minute, time.minute() as u32)? &&
            self.matches_field(&self.hour, time.hour() as u32)? &&
            self.matches_field(&self.day_of_month, time.day() as u32)? &&
            self.matches_field(&self.month, time.month() as u32)? &&
            self.matches_field(&self.day_of_week, time.weekday().num_days_from_monday() as u32)?
        )
    }

    fn matches_field(&self, field: &CronField, value: u32) -> Result<bool> {
        match field {
            CronField::All => Ok(true),
            CronField::Single(v) => Ok(*v == value),
            CronField::List(values) => Ok(values.contains(&value)),
            CronField::Range { start, end } => Ok(value >= *start && value <= *end),
            CronField::Step { start, step } => Ok(value >= *start && (value - *start) % *step == 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_parsing() {
        let expression = CronExpression::new("*/15 0 1,15 * 1-5").unwrap();
        match expression {
            CronExpression {
                minute: CronField::Step { start: 0, step: 15 },
                hour: CronField::Single(0),
                day_of_month: CronField::List(days),
                month: CronField::All,
                day_of_week: CronField::Range { start: 1, end: 5 },
            } => {
                assert_eq!(days, vec![1, 15]);
            }
            _ => panic!("Unexpected parse result"),
        }
    }

    #[test]
    fn test_next_occurrence() {
        let expression = CronExpression::new("0 0 * * *").unwrap(); // Daily at midnight
        let now = Utc::now();
        let next = expression.next_occurrence(now).unwrap();
        assert!(next > now);
        assert_eq!(next.hour(), 0);
        assert_eq!(next.minute(), 0);
    }

    #[test]
    fn test_invalid_expressions() {
        assert!(CronExpression::new("60 * * * *").is_err()); // Invalid minute
        assert!(CronExpression::new("* 24 * * *").is_err()); // Invalid hour
        assert!(CronExpression::new("* * 32 * *").is_err()); // Invalid day
        assert!(CronExpression::new("* * * 13 *").is_err()); // Invalid month
        assert!(CronExpression::new("* * * * 7").is_err());  // Invalid day of week
    }
}
