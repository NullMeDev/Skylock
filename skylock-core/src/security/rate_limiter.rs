//! Rate Limiting for Authentication and API Endpoints
//!
//! Provides protection against brute-force attacks using a token bucket algorithm
//! with configurable lockout mechanisms.
//!
//! ## Features
//! - Token bucket rate limiting per identifier (IP, user, etc.)
//! - Configurable lockout after failed attempts
//! - Exponential backoff for repeated violations
//! - Automatic cleanup of expired entries
//! - Thread-safe async implementation

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{warn, info, debug};

/// Rate limiting result
#[derive(Debug, Clone, PartialEq)]
pub enum RateLimitResult {
    /// Request is allowed
    Allowed,
    /// Request is rate limited, includes wait duration
    Limited { wait_duration: Duration, reason: String },
    /// Account is locked out
    LockedOut { until: Instant, attempts: u32 },
}

impl RateLimitResult {
    /// Check if the request is allowed
    pub fn is_allowed(&self) -> bool {
        matches!(self, RateLimitResult::Allowed)
    }
    
    /// Get the wait duration if rate limited
    pub fn wait_duration(&self) -> Option<Duration> {
        match self {
            RateLimitResult::Limited { wait_duration, .. } => Some(*wait_duration),
            RateLimitResult::LockedOut { until, .. } => {
                let now = Instant::now();
                if *until > now {
                    Some(*until - now)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Configuration for rate limiting
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests allowed in the time window
    pub max_requests: u32,
    /// Time window for rate limiting
    pub window: Duration,
    /// Number of failed attempts before lockout
    pub lockout_threshold: u32,
    /// Duration of initial lockout
    pub lockout_duration: Duration,
    /// Maximum lockout duration (for exponential backoff)
    pub max_lockout_duration: Duration,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
    /// Whether to use exponential backoff
    pub exponential_backoff: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 10,                           // 10 requests
            window: Duration::from_secs(60),            // per minute
            lockout_threshold: 5,                       // lock after 5 failed attempts
            lockout_duration: Duration::from_secs(300), // 5 minutes initial lockout
            max_lockout_duration: Duration::from_secs(86400), // 24 hours max
            backoff_multiplier: 2.0,                    // double each time
            exponential_backoff: true,
        }
    }
}

impl RateLimitConfig {
    /// Create a strict rate limit config (for authentication)
    pub fn strict() -> Self {
        Self {
            max_requests: 5,
            window: Duration::from_secs(60),
            lockout_threshold: 3,
            lockout_duration: Duration::from_secs(900), // 15 minutes
            max_lockout_duration: Duration::from_secs(86400),
            backoff_multiplier: 3.0,
            exponential_backoff: true,
        }
    }
    
    /// Create a relaxed rate limit config (for general API)
    pub fn relaxed() -> Self {
        Self {
            max_requests: 100,
            window: Duration::from_secs(60),
            lockout_threshold: 20,
            lockout_duration: Duration::from_secs(60),
            max_lockout_duration: Duration::from_secs(3600),
            backoff_multiplier: 1.5,
            exponential_backoff: false,
        }
    }
}

/// State for a single rate-limited identifier
#[derive(Debug, Clone)]
struct RateLimitState {
    /// Timestamps of recent requests
    requests: Vec<Instant>,
    /// Number of consecutive failures
    failure_count: u32,
    /// Current lockout end time (if locked)
    locked_until: Option<Instant>,
    /// Number of times this identifier has been locked out
    lockout_count: u32,
    /// Last activity time
    last_activity: Instant,
}

impl RateLimitState {
    fn new() -> Self {
        Self {
            requests: Vec::new(),
            failure_count: 0,
            locked_until: None,
            lockout_count: 0,
            last_activity: Instant::now(),
        }
    }
    
    /// Check if currently locked out
    fn is_locked(&self) -> bool {
        self.locked_until.map(|until| Instant::now() < until).unwrap_or(false)
    }
    
    /// Clean up old request timestamps
    fn cleanup_old_requests(&mut self, window: Duration) {
        let cutoff = Instant::now() - window;
        self.requests.retain(|&t| t > cutoff);
    }
}

/// Rate limiter with token bucket algorithm and lockout support
pub struct RateLimiter {
    config: RateLimitConfig,
    /// State per identifier (IP address, user ID, etc.)
    state: Arc<RwLock<HashMap<String, RateLimitState>>>,
    /// Last cleanup time
    last_cleanup: Arc<RwLock<Instant>>,
}

impl RateLimiter {
    /// Create a new rate limiter with the given configuration
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(HashMap::new())),
            last_cleanup: Arc::new(RwLock::new(Instant::now())),
        }
    }
    
    /// Create a rate limiter with default configuration
    pub fn default_limiter() -> Self {
        Self::new(RateLimitConfig::default())
    }
    
    /// Create a strict rate limiter for authentication
    pub fn authentication_limiter() -> Self {
        Self::new(RateLimitConfig::strict())
    }
    
    /// Check if a request from the given identifier is allowed
    pub async fn check(&self, identifier: &str) -> RateLimitResult {
        // Periodic cleanup
        self.maybe_cleanup().await;
        
        let mut state_map = self.state.write().await;
        let state = state_map.entry(identifier.to_string()).or_insert_with(RateLimitState::new);
        
        // Update last activity
        state.last_activity = Instant::now();
        
        // Check if locked out
        if state.is_locked() {
            let until = state.locked_until.unwrap();
            warn!(
                "Rate limit: {} is locked out until {:?}",
                identifier,
                until.duration_since(Instant::now())
            );
            return RateLimitResult::LockedOut {
                until,
                attempts: state.failure_count,
            };
        }
        
        // Clean up old requests
        state.cleanup_old_requests(self.config.window);
        
        // Check request count
        if state.requests.len() >= self.config.max_requests as usize {
            let oldest = state.requests.first().copied().unwrap_or_else(Instant::now);
            let wait = self.config.window.saturating_sub(oldest.elapsed());
            
            debug!(
                "Rate limit: {} exceeded {} requests in {:?}",
                identifier, self.config.max_requests, self.config.window
            );
            
            return RateLimitResult::Limited {
                wait_duration: wait,
                reason: format!(
                    "Rate limit exceeded: {} requests per {:?}",
                    self.config.max_requests, self.config.window
                ),
            };
        }
        
        // Record this request
        state.requests.push(Instant::now());
        
        RateLimitResult::Allowed
    }
    
    /// Record a successful attempt (resets failure count)
    pub async fn record_success(&self, identifier: &str) {
        let mut state_map = self.state.write().await;
        if let Some(state) = state_map.get_mut(identifier) {
            state.failure_count = 0;
            // Don't reset lockout_count - that persists to enable exponential backoff
            info!("Rate limit: {} - successful attempt, failure count reset", identifier);
        }
    }
    
    /// Record a failed attempt (may trigger lockout)
    pub async fn record_failure(&self, identifier: &str) -> RateLimitResult {
        let mut state_map = self.state.write().await;
        let state = state_map.entry(identifier.to_string()).or_insert_with(RateLimitState::new);
        
        state.failure_count += 1;
        state.last_activity = Instant::now();
        
        warn!(
            "Rate limit: {} - failure #{}/{}",
            identifier, state.failure_count, self.config.lockout_threshold
        );
        
        // Check if we should lock out
        if state.failure_count >= self.config.lockout_threshold {
            let lockout_duration = if self.config.exponential_backoff && state.lockout_count > 0 {
                let multiplier = self.config.backoff_multiplier.powi(state.lockout_count as i32);
                let duration_secs = (self.config.lockout_duration.as_secs_f64() * multiplier) as u64;
                Duration::from_secs(duration_secs.min(self.config.max_lockout_duration.as_secs()))
            } else {
                self.config.lockout_duration
            };
            
            let until = Instant::now() + lockout_duration;
            state.locked_until = Some(until);
            state.lockout_count += 1;
            state.failure_count = 0; // Reset for next lockout period
            
            warn!(
                "Rate limit: {} locked out for {:?} (lockout #{})",
                identifier, lockout_duration, state.lockout_count
            );
            
            return RateLimitResult::LockedOut {
                until,
                attempts: self.config.lockout_threshold,
            };
        }
        
        RateLimitResult::Allowed
    }
    
    /// Check rate limit and record failure in one operation (for authentication)
    pub async fn check_and_record_failure(&self, identifier: &str) -> RateLimitResult {
        let check_result = self.check(identifier).await;
        if !check_result.is_allowed() {
            return check_result;
        }
        
        self.record_failure(identifier).await
    }
    
    /// Manually unlock an identifier (admin action)
    pub async fn unlock(&self, identifier: &str) {
        let mut state_map = self.state.write().await;
        if let Some(state) = state_map.get_mut(identifier) {
            state.locked_until = None;
            state.failure_count = 0;
            info!("Rate limit: {} manually unlocked", identifier);
        }
    }
    
    /// Get the current state for an identifier (for monitoring)
    pub async fn get_state(&self, identifier: &str) -> Option<(u32, bool, u32)> {
        let state_map = self.state.read().await;
        state_map.get(identifier).map(|s| (s.failure_count, s.is_locked(), s.lockout_count))
    }
    
    /// Clear all rate limiting state (for testing)
    pub async fn clear(&self) {
        let mut state_map = self.state.write().await;
        state_map.clear();
    }
    
    /// Periodic cleanup of expired entries
    async fn maybe_cleanup(&self) {
        const CLEANUP_INTERVAL: Duration = Duration::from_secs(300); // 5 minutes
        const MAX_IDLE_TIME: Duration = Duration::from_secs(3600);   // 1 hour
        
        {
            let last = self.last_cleanup.read().await;
            if last.elapsed() < CLEANUP_INTERVAL {
                return;
            }
        }
        
        {
            let mut last = self.last_cleanup.write().await;
            *last = Instant::now();
        }
        
        let mut state_map = self.state.write().await;
        let before_count = state_map.len();
        
        state_map.retain(|_, state| {
            // Keep if recently active or currently locked
            state.last_activity.elapsed() < MAX_IDLE_TIME || state.is_locked()
        });
        
        let removed = before_count - state_map.len();
        if removed > 0 {
            debug!("Rate limiter cleanup: removed {} idle entries", removed);
        }
    }
}

/// Global rate limiters for different purposes
pub struct RateLimiters {
    /// Rate limiter for authentication attempts
    pub auth: RateLimiter,
    /// Rate limiter for API requests
    pub api: RateLimiter,
    /// Rate limiter for backup operations
    pub backup: RateLimiter,
}

impl RateLimiters {
    /// Create default rate limiters
    pub fn new() -> Self {
        Self {
            auth: RateLimiter::authentication_limiter(),
            api: RateLimiter::new(RateLimitConfig::relaxed()),
            backup: RateLimiter::new(RateLimitConfig {
                max_requests: 10,
                window: Duration::from_secs(3600), // 10 backups per hour
                lockout_threshold: 15,
                lockout_duration: Duration::from_secs(1800), // 30 minutes
                max_lockout_duration: Duration::from_secs(7200), // 2 hours
                backoff_multiplier: 1.5,
                exponential_backoff: false,
            }),
        }
    }
}

impl Default for RateLimiters {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_basic_rate_limiting() {
        let config = RateLimitConfig {
            max_requests: 3,
            window: Duration::from_secs(60),
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);
        
        // First 3 requests should be allowed
        assert!(limiter.check("test").await.is_allowed());
        assert!(limiter.check("test").await.is_allowed());
        assert!(limiter.check("test").await.is_allowed());
        
        // 4th request should be limited
        let result = limiter.check("test").await;
        assert!(!result.is_allowed());
        match result {
            RateLimitResult::Limited { .. } => {},
            _ => panic!("Expected Limited result"),
        }
    }
    
    #[tokio::test]
    async fn test_different_identifiers() {
        let config = RateLimitConfig {
            max_requests: 2,
            window: Duration::from_secs(60),
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);
        
        // Different identifiers should have separate limits
        assert!(limiter.check("user1").await.is_allowed());
        assert!(limiter.check("user1").await.is_allowed());
        assert!(!limiter.check("user1").await.is_allowed());
        
        // user2 should still have full quota
        assert!(limiter.check("user2").await.is_allowed());
        assert!(limiter.check("user2").await.is_allowed());
    }
    
    #[tokio::test]
    async fn test_lockout() {
        let config = RateLimitConfig {
            max_requests: 10,
            window: Duration::from_secs(60),
            lockout_threshold: 3,
            lockout_duration: Duration::from_millis(100),
            exponential_backoff: false,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);
        
        // Record failures until lockout
        assert!(limiter.record_failure("test").await.is_allowed());
        assert!(limiter.record_failure("test").await.is_allowed());
        
        let result = limiter.record_failure("test").await;
        match result {
            RateLimitResult::LockedOut { .. } => {},
            _ => panic!("Expected LockedOut result"),
        }
        
        // Should be locked
        let result = limiter.check("test").await;
        match result {
            RateLimitResult::LockedOut { .. } => {},
            _ => panic!("Expected LockedOut result"),
        }
        
        // Wait for lockout to expire
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Should be allowed again
        assert!(limiter.check("test").await.is_allowed());
    }
    
    #[tokio::test]
    async fn test_success_resets_failures() {
        let config = RateLimitConfig {
            lockout_threshold: 3,
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);
        
        // Record 2 failures
        limiter.record_failure("test").await;
        limiter.record_failure("test").await;
        
        // Record success - should reset count
        limiter.record_success("test").await;
        
        // Should need 3 more failures to lock out
        let state = limiter.get_state("test").await.unwrap();
        assert_eq!(state.0, 0); // failure_count should be 0
    }
    
    #[tokio::test]
    async fn test_manual_unlock() {
        let config = RateLimitConfig {
            lockout_threshold: 1,
            lockout_duration: Duration::from_secs(3600),
            ..Default::default()
        };
        let limiter = RateLimiter::new(config);
        
        // Trigger lockout
        limiter.record_failure("test").await;
        assert!(!limiter.check("test").await.is_allowed());
        
        // Manual unlock
        limiter.unlock("test").await;
        assert!(limiter.check("test").await.is_allowed());
    }
}
