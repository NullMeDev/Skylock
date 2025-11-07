use async_trait::async_trait;
use std::time::Duration;
use tokio::time::sleep;

use crate::error::{Error, ErrorStatus};

#[async_trait]
pub trait CanHandle {
    /// Determines if this handler can handle the given error
    async fn can_handle(&self, error: &Error) -> bool;

    /// Gets the priority level of this handler
    fn priority(&self) -> i32 { 0 }
}

#[async_trait]
pub trait HandleError {
    /// Attempts to handle the error, returning Ok(()) if successful
    /// or the error (potentially modified) if handling failed
    async fn handle_error(&self, error: &Error) -> Result<(), Error>;

    /// Called after handling an error to perform any cleanup
    async fn post_handle(&self, _error: &Error) -> Result<(), Error> {
        Ok(())
    }
}

pub struct RetryHandler {
    max_retries: u32,
    base_delay: Duration,
    max_delay: Duration,
    jitter_factor: f64,
}

impl RetryHandler {
    pub fn new(max_retries: u32, base_delay: Duration) -> Self {
        Self {
            max_retries,
            base_delay,
            max_delay: Duration::from_secs(60), // 1 minute max delay
            jitter_factor: 0.2, // 20% jitter
        }
    }

    pub fn with_max_delay(mut self, max_delay: Duration) -> Self {
        self.max_delay = max_delay;
        self
    }

    pub fn with_jitter_factor(mut self, jitter_factor: f64) -> Self {
        self.jitter_factor = jitter_factor.clamp(0.0, 0.5);
        self
    }

    fn calculate_delay(&self, retry_count: u32) -> Duration {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        // Calculate exponential delay: base_delay * 2^retry_count
        let exp_delay = self.base_delay.as_millis() as f64
            * (2_f64.powi(retry_count as i32));

        // Apply max delay cap
        let capped_delay = exp_delay.min(self.max_delay.as_millis() as f64);

        // Apply jitter: delay ± (delay * jitter_factor)
        let jitter_range = capped_delay * self.jitter_factor;
        let jittered_delay = capped_delay + rng.gen_range(-jitter_range..=jitter_range);

        Duration::from_millis(jittered_delay.round() as u64)
    }
}

#[async_trait]
impl CanHandle for RetryHandler {
    async fn can_handle(&self, error: &Error) -> bool {
        error.is_retryable()
    }
}

#[async_trait]
impl HandleError for RetryHandler {
    async fn handle_error(&self, error: &Error) -> Result<(), Error> {
        if !error.is_retryable() {
            return Err(error.clone());
        }

        let mut error = error.clone();

        while error.retry_count < self.max_retries {
            error.increment_retry_count();
            error.status = ErrorStatus::Retrying;

            // Calculate and apply exponential backoff with jitter
            let delay = self.calculate_delay(error.retry_count);
            sleep(delay).await;

            // Try the operation again - in real code, this would retry the actual operation
            if error.retry_count >= self.max_retries {
                error.mark_non_retryable();
                return Err(error);
            }
        }

        error.mark_non_retryable();
        Err(error)
    }
}

pub struct ErrorHandlerRegistry {
    handlers: Vec<Box<dyn HandleError + Send + Sync>>,
}

impl ErrorHandlerRegistry {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    pub fn register<H>(&mut self, handler: H)
    where
        H: HandleError + Send + Sync + 'static,
    {
        self.handlers.push(Box::new(handler));
    }

    pub async fn handle_error(&self, error: &Error) -> Result<(), Error> {
        for handler in &self.handlers {
            if let Ok(()) = handler.handle_error(error).await {
                return Ok(());
            }
        }
        Err(error.clone())
    }
}

impl Default for ErrorHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
