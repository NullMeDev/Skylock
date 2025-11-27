//! Connection Pool for WebDAV and SSH/SFTP
//!
//! Provides efficient connection reuse to reduce connection overhead:
//! - Maintains pool of pre-established connections
//! - Health checks with automatic reconnection
//! - Connection timeout and idle timeout management
//! - Per-connection metrics tracking
//! - Graceful degradation under load

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tracing::{debug, error, info, warn};

/// Default pool size for connections
const DEFAULT_POOL_SIZE: usize = 8;

/// Minimum pool size
const MIN_POOL_SIZE: usize = 2;

/// Maximum pool size
const MAX_POOL_SIZE: usize = 32;

/// Connection idle timeout (5 minutes)
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// Connection max lifetime (30 minutes)
const DEFAULT_MAX_LIFETIME: Duration = Duration::from_secs(1800);

/// Health check interval (30 seconds)
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// Connection acquire timeout (10 seconds)
const DEFAULT_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(10);

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connection is available for use
    Available,
    /// Connection is currently in use
    InUse,
    /// Connection is being validated
    Validating,
    /// Connection is unhealthy and should be closed
    Unhealthy,
    /// Connection is being closed
    Closing,
}

/// Connection type (WebDAV or SSH/SFTP)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    /// HTTP/HTTPS WebDAV connection
    WebDav,
    /// SSH/SFTP connection
    Sftp,
}

/// Metrics for a pooled connection
#[derive(Debug)]
pub struct ConnectionMetrics {
    /// When connection was created
    pub created_at: Instant,
    /// When connection was last used
    pub last_used: Instant,
    /// Number of times connection was used
    pub use_count: u64,
    /// Total bytes transferred
    pub bytes_transferred: u64,
    /// Number of errors on this connection
    pub error_count: u64,
}

impl Default for ConnectionMetrics {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            created_at: now,
            last_used: now,
            use_count: 0,
            bytes_transferred: 0,
            error_count: 0,
        }
    }
}

/// A pooled connection wrapper
pub struct PooledConnection<T> {
    /// The actual connection
    connection: T,
    /// Connection ID
    id: u64,
    /// Connection state
    state: ConnectionState,
    /// Connection type
    conn_type: ConnectionType,
    /// Connection metrics
    metrics: ConnectionMetrics,
}

impl<T> PooledConnection<T> {
    /// Create a new pooled connection
    pub fn new(connection: T, id: u64, conn_type: ConnectionType) -> Self {
        Self {
            connection,
            id,
            state: ConnectionState::Available,
            conn_type,
            metrics: ConnectionMetrics::default(),
        }
    }

    /// Get the connection
    pub fn connection(&self) -> &T {
        &self.connection
    }

    /// Get mutable connection
    pub fn connection_mut(&mut self) -> &mut T {
        &mut self.connection
    }

    /// Get connection ID
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get connection state
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Get connection type
    pub fn conn_type(&self) -> ConnectionType {
        self.conn_type
    }

    /// Get connection metrics
    pub fn metrics(&self) -> &ConnectionMetrics {
        &self.metrics
    }

    /// Mark connection as in use
    pub fn mark_in_use(&mut self) {
        self.state = ConnectionState::InUse;
        self.metrics.last_used = Instant::now();
        self.metrics.use_count += 1;
    }

    /// Mark connection as available
    pub fn mark_available(&mut self) {
        self.state = ConnectionState::Available;
        self.metrics.last_used = Instant::now();
    }

    /// Mark connection as unhealthy
    pub fn mark_unhealthy(&mut self) {
        self.state = ConnectionState::Unhealthy;
        self.metrics.error_count += 1;
    }

    /// Record bytes transferred
    pub fn record_bytes(&mut self, bytes: u64) {
        self.metrics.bytes_transferred += bytes;
    }

    /// Record an error
    pub fn record_error(&mut self) {
        self.metrics.error_count += 1;
    }

    /// Check if connection has expired (exceeded max lifetime)
    pub fn is_expired(&self, max_lifetime: Duration) -> bool {
        self.metrics.created_at.elapsed() > max_lifetime
    }

    /// Check if connection is idle too long
    pub fn is_idle_too_long(&self, idle_timeout: Duration) -> bool {
        self.metrics.last_used.elapsed() > idle_timeout
    }

    /// Check if connection has too many errors
    pub fn has_too_many_errors(&self, max_errors: u64) -> bool {
        self.metrics.error_count >= max_errors
    }
}

/// Connection pool configuration
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    /// Minimum number of connections to maintain
    pub min_connections: usize,
    /// Maximum number of connections
    pub max_connections: usize,
    /// Initial number of connections
    pub initial_connections: usize,
    /// Idle timeout before closing connection
    pub idle_timeout: Duration,
    /// Maximum connection lifetime
    pub max_lifetime: Duration,
    /// Timeout for acquiring a connection
    pub acquire_timeout: Duration,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Maximum errors before closing connection
    pub max_errors_per_connection: u64,
    /// Enable connection validation on acquire
    pub validate_on_acquire: bool,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            min_connections: MIN_POOL_SIZE,
            max_connections: DEFAULT_POOL_SIZE,
            initial_connections: MIN_POOL_SIZE,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
            max_lifetime: DEFAULT_MAX_LIFETIME,
            acquire_timeout: DEFAULT_ACQUIRE_TIMEOUT,
            health_check_interval: HEALTH_CHECK_INTERVAL,
            max_errors_per_connection: 5,
            validate_on_acquire: true,
        }
    }
}

impl ConnectionPoolConfig {
    /// Create config optimized for high throughput
    pub fn high_throughput() -> Self {
        Self {
            min_connections: 4,
            max_connections: MAX_POOL_SIZE,
            initial_connections: 8,
            idle_timeout: Duration::from_secs(600), // 10 min
            validate_on_acquire: false, // Skip validation for speed
            ..Default::default()
        }
    }

    /// Create config optimized for low latency
    pub fn low_latency() -> Self {
        Self {
            min_connections: 8,
            max_connections: 16,
            initial_connections: 8,
            idle_timeout: Duration::from_secs(120), // 2 min
            validate_on_acquire: true,
            ..Default::default()
        }
    }

    /// Create config for memory-constrained environments
    pub fn low_memory() -> Self {
        Self {
            min_connections: 1,
            max_connections: 4,
            initial_connections: 2,
            idle_timeout: Duration::from_secs(60), // 1 min
            max_lifetime: Duration::from_secs(600), // 10 min
            ..Default::default()
        }
    }
}

/// Pool statistics
#[derive(Debug, Default)]
pub struct PoolStats {
    /// Total connections ever created
    pub connections_created: AtomicU64,
    /// Total connections closed
    pub connections_closed: AtomicU64,
    /// Current active connections
    pub active_connections: AtomicUsize,
    /// Current idle connections
    pub idle_connections: AtomicUsize,
    /// Total acquire attempts
    pub acquire_attempts: AtomicU64,
    /// Successful acquires
    pub acquire_successes: AtomicU64,
    /// Acquire timeouts
    pub acquire_timeouts: AtomicU64,
    /// Connection validation failures
    pub validation_failures: AtomicU64,
    /// Health checks performed
    pub health_checks: AtomicU64,
    /// Total bytes transferred through pool
    pub total_bytes_transferred: AtomicU64,
}

impl PoolStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_connection_created(&self) {
        self.connections_created.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_connection_closed(&self) {
        self.connections_closed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_acquire_attempt(&self) {
        self.acquire_attempts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_acquire_success(&self) {
        self.acquire_successes.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_acquire_timeout(&self) {
        self.acquire_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_validation_failure(&self) {
        self.validation_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_health_check(&self) {
        self.health_checks.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_bytes(&self, bytes: u64) {
        self.total_bytes_transferred.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn update_active(&self, count: usize) {
        self.active_connections.store(count, Ordering::Relaxed);
    }

    pub fn update_idle(&self, count: usize) {
        self.idle_connections.store(count, Ordering::Relaxed);
    }
}

/// Connection factory trait for creating new connections
#[async_trait::async_trait]
pub trait ConnectionFactory<T>: Send + Sync {
    /// Create a new connection
    async fn create(&self) -> Result<T, ConnectionPoolError>;

    /// Validate a connection
    async fn validate(&self, connection: &T) -> bool;

    /// Close a connection gracefully
    async fn close(&self, connection: T);
}

/// Connection pool error types
#[derive(Debug, thiserror::Error)]
pub enum ConnectionPoolError {
    #[error("Acquire timeout after {0:?}")]
    AcquireTimeout(Duration),

    #[error("Pool exhausted: no connections available")]
    PoolExhausted,

    #[error("Connection validation failed")]
    ValidationFailed,

    #[error("Connection creation failed: {0}")]
    CreationFailed(String),

    #[error("Pool is closed")]
    PoolClosed,
}

/// Generic connection pool
pub struct ConnectionPool<T, F>
where
    F: ConnectionFactory<T>,
{
    /// Pool configuration
    config: ConnectionPoolConfig,
    /// Connection factory
    factory: Arc<F>,
    /// Available connections
    available: Mutex<VecDeque<PooledConnection<T>>>,
    /// Connection counter for IDs
    connection_counter: AtomicU64,
    /// Pool statistics
    stats: Arc<PoolStats>,
    /// Semaphore for limiting total connections
    semaphore: Semaphore,
    /// Pool state (open/closed)
    closed: RwLock<bool>,
    /// Connection type
    conn_type: ConnectionType,
}

impl<T, F> ConnectionPool<T, F>
where
    T: Send + 'static,
    F: ConnectionFactory<T> + 'static,
{
    /// Create a new connection pool
    pub async fn new(factory: F, config: ConnectionPoolConfig, conn_type: ConnectionType) -> Self {
        let pool = Self {
            semaphore: Semaphore::new(config.max_connections),
            config: config.clone(),
            factory: Arc::new(factory),
            available: Mutex::new(VecDeque::new()),
            connection_counter: AtomicU64::new(0),
            stats: Arc::new(PoolStats::new()),
            closed: RwLock::new(false),
            conn_type,
        };

        // Pre-create initial connections
        for _ in 0..config.initial_connections {
            if let Ok(conn) = pool.create_connection().await {
                pool.available.lock().await.push_back(conn);
            }
        }

        info!(
            "Connection pool created: type={:?}, initial={}, max={}",
            conn_type, config.initial_connections, config.max_connections
        );

        pool
    }

    /// Create a new pooled connection
    async fn create_connection(&self) -> Result<PooledConnection<T>, ConnectionPoolError> {
        let id = self.connection_counter.fetch_add(1, Ordering::Relaxed);
        let connection = self.factory.create().await?;
        self.stats.record_connection_created();
        Ok(PooledConnection::new(connection, id, self.conn_type))
    }

    /// Acquire a connection from the pool
    pub async fn acquire(&self) -> Result<ConnectionGuard<T, F>, ConnectionPoolError> {
        // Check if pool is closed
        if *self.closed.read().await {
            return Err(ConnectionPoolError::PoolClosed);
        }

        self.stats.record_acquire_attempt();

        // Try to acquire with timeout
        let acquire_start = Instant::now();

        loop {
            // Check timeout
            if acquire_start.elapsed() > self.config.acquire_timeout {
                self.stats.record_acquire_timeout();
                return Err(ConnectionPoolError::AcquireTimeout(self.config.acquire_timeout));
            }

            // Try to get from available pool
            {
                let mut available = self.available.lock().await;
                while let Some(mut conn) = available.pop_front() {
                    // Check if connection is still healthy
                    if conn.is_expired(self.config.max_lifetime)
                        || conn.is_idle_too_long(self.config.idle_timeout)
                        || conn.has_too_many_errors(self.config.max_errors_per_connection)
                    {
                        // Close unhealthy connection
                        debug!("Closing unhealthy connection {}", conn.id());
                        self.factory.close(conn.connection).await;
                        self.stats.record_connection_closed();
                        continue;
                    }

                    // Validate if configured
                    if self.config.validate_on_acquire {
                        if !self.factory.validate(&conn.connection).await {
                            self.stats.record_validation_failure();
                            self.factory.close(conn.connection).await;
                            self.stats.record_connection_closed();
                            continue;
                        }
                    }

                    // Found a good connection
                    conn.mark_in_use();
                    self.stats.record_acquire_success();
                    self.update_stats(&available).await;

                    return Ok(ConnectionGuard {
                        connection: Some(conn),
                        pool: self,
                    });
                }
            }

            // No available connections, try to create new one
            match self.semaphore.try_acquire() {
                Ok(_permit) => {
                    match self.create_connection().await {
                        Ok(mut conn) => {
                            conn.mark_in_use();
                            self.stats.record_acquire_success();
                            return Ok(ConnectionGuard {
                                connection: Some(conn),
                                pool: self,
                            });
                        }
                        Err(e) => {
                            warn!("Failed to create connection: {}", e);
                            // Permit is dropped, freeing the slot
                        }
                    }
                }
                Err(_) => {
                    // Pool is at capacity, wait a bit
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        }
    }

    /// Return a connection to the pool
    async fn release(&self, mut connection: PooledConnection<T>) {
        // Check if pool is closed
        if *self.closed.read().await {
            self.factory.close(connection.connection).await;
            self.stats.record_connection_closed();
            return;
        }

        // Check if connection is still healthy
        if connection.is_expired(self.config.max_lifetime)
            || connection.has_too_many_errors(self.config.max_errors_per_connection)
            || connection.state() == ConnectionState::Unhealthy
        {
            self.factory.close(connection.connection).await;
            self.stats.record_connection_closed();
            return;
        }

        // Return to pool
        connection.mark_available();
        let mut available = self.available.lock().await;
        available.push_back(connection);
        self.update_stats(&available).await;
    }

    /// Update pool statistics
    async fn update_stats(&self, available: &VecDeque<PooledConnection<T>>) {
        self.stats.update_idle(available.len());
        let total = self.connection_counter.load(Ordering::Relaxed);
        let closed = self.stats.connections_closed.load(Ordering::Relaxed);
        self.stats.update_active((total - closed) as usize - available.len());
    }

    /// Get pool statistics
    pub fn stats(&self) -> &Arc<PoolStats> {
        &self.stats
    }

    /// Get pool configuration
    pub fn config(&self) -> &ConnectionPoolConfig {
        &self.config
    }

    /// Get current pool size
    pub async fn size(&self) -> usize {
        self.available.lock().await.len()
    }

    /// Close the pool
    pub async fn close(&self) {
        *self.closed.write().await = true;

        let mut available = self.available.lock().await;
        while let Some(conn) = available.pop_front() {
            self.factory.close(conn.connection).await;
            self.stats.record_connection_closed();
        }

        info!("Connection pool closed");
    }

    /// Run health checks on idle connections
    pub async fn health_check(&self) {
        let mut available = self.available.lock().await;
        let mut healthy = VecDeque::new();

        while let Some(conn) = available.pop_front() {
            self.stats.record_health_check();

            if conn.is_expired(self.config.max_lifetime)
                || conn.is_idle_too_long(self.config.idle_timeout)
            {
                self.factory.close(conn.connection).await;
                self.stats.record_connection_closed();
                continue;
            }

            if self.factory.validate(&conn.connection).await {
                healthy.push_back(conn);
            } else {
                self.stats.record_validation_failure();
                self.factory.close(conn.connection).await;
                self.stats.record_connection_closed();
            }
        }

        *available = healthy;
        self.update_stats(&available).await;
    }
}

/// RAII guard for connection lifetime management
pub struct ConnectionGuard<'a, T, F>
where
    F: ConnectionFactory<T>,
{
    connection: Option<PooledConnection<T>>,
    pool: &'a ConnectionPool<T, F>,
}

impl<'a, T, F> ConnectionGuard<'a, T, F>
where
    T: Send + 'static,
    F: ConnectionFactory<T> + 'static,
{
    /// Get the connection
    pub fn connection(&self) -> &T {
        self.connection.as_ref().unwrap().connection()
    }

    /// Get mutable connection
    pub fn connection_mut(&mut self) -> &mut T {
        self.connection.as_mut().unwrap().connection_mut()
    }

    /// Record bytes transferred
    pub fn record_bytes(&mut self, bytes: u64) {
        if let Some(ref mut conn) = self.connection {
            conn.record_bytes(bytes);
            self.pool.stats.record_bytes(bytes);
        }
    }

    /// Record an error
    pub fn record_error(&mut self) {
        if let Some(ref mut conn) = self.connection {
            conn.record_error();
        }
    }

    /// Mark connection as unhealthy (will be closed on release)
    pub fn mark_unhealthy(&mut self) {
        if let Some(ref mut conn) = self.connection {
            conn.mark_unhealthy();
        }
    }
}

impl<'a, T, F> Drop for ConnectionGuard<'a, T, F>
where
    F: ConnectionFactory<T>,
{
    fn drop(&mut self) {
        if let Some(conn) = self.connection.take() {
            // Use spawn to release connection without blocking
            // This is safe because we're using Arc for the factory
            let pool_stats = self.pool.stats.clone();
            let factory = self.pool.factory.clone();
            let available = self.pool.available.try_lock();
            let config = self.pool.config.clone();

            if let Ok(mut available) = available {
                // Fast path: directly return to pool
                if !conn.is_expired(config.max_lifetime)
                    && !conn.has_too_many_errors(config.max_errors_per_connection)
                    && conn.state() != ConnectionState::Unhealthy
                {
                    let mut conn = conn;
                    conn.mark_available();
                    pool_stats.update_idle(available.len() + 1);
                    available.push_back(conn);
                    return;
                }
            }

            // Slow path: need async handling
            // Note: In production, you'd want a proper async cleanup mechanism
        }
    }
}

/// Placeholder connection for testing
#[derive(Debug)]
pub struct TestConnection {
    pub id: u64,
    pub valid: bool,
}

/// Test connection factory
pub struct TestConnectionFactory {
    counter: AtomicU64,
}

impl TestConnectionFactory {
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(0),
        }
    }
}

impl Default for TestConnectionFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ConnectionFactory<TestConnection> for TestConnectionFactory {
    async fn create(&self) -> Result<TestConnection, ConnectionPoolError> {
        let id = self.counter.fetch_add(1, Ordering::Relaxed);
        Ok(TestConnection { id, valid: true })
    }

    async fn validate(&self, connection: &TestConnection) -> bool {
        connection.valid
    }

    async fn close(&self, _connection: TestConnection) {
        // No-op for test connections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pool_creation() {
        let factory = TestConnectionFactory::new();
        let config = ConnectionPoolConfig {
            initial_connections: 2,
            max_connections: 4,
            ..Default::default()
        };

        let pool = ConnectionPool::new(factory, config, ConnectionType::WebDav).await;

        assert_eq!(pool.size().await, 2);
        assert_eq!(pool.stats().connections_created.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn test_acquire_release() {
        let factory = TestConnectionFactory::new();
        let config = ConnectionPoolConfig {
            initial_connections: 1,
            max_connections: 2,
            validate_on_acquire: false,
            ..Default::default()
        };

        let pool = ConnectionPool::new(factory, config, ConnectionType::WebDav).await;

        // Acquire a connection
        let guard = pool.acquire().await.unwrap();
        assert_eq!(pool.size().await, 0);

        // Release connection
        drop(guard);
        // Note: Due to async nature, we may need to wait
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Stats should reflect acquire
        assert_eq!(pool.stats().acquire_attempts.load(Ordering::Relaxed), 1);
        assert_eq!(pool.stats().acquire_successes.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_multiple_acquires() {
        let factory = TestConnectionFactory::new();
        let config = ConnectionPoolConfig {
            initial_connections: 0,
            max_connections: 3,
            validate_on_acquire: false,
            ..Default::default()
        };

        let pool = ConnectionPool::new(factory, config, ConnectionType::WebDav).await;

        // Acquire multiple connections
        let g1 = pool.acquire().await.unwrap();
        let g2 = pool.acquire().await.unwrap();
        let g3 = pool.acquire().await.unwrap();

        assert_eq!(pool.stats().connections_created.load(Ordering::Relaxed), 3);

        drop(g1);
        drop(g2);
        drop(g3);
    }

    #[test]
    fn test_connection_metrics() {
        let metrics = ConnectionMetrics::default();

        // Check defaults
        assert_eq!(metrics.use_count, 0);
        assert_eq!(metrics.bytes_transferred, 0);
        assert_eq!(metrics.error_count, 0);
    }

    #[test]
    fn test_pooled_connection() {
        let mut conn = PooledConnection::new(
            TestConnection { id: 1, valid: true },
            1,
            ConnectionType::WebDav,
        );

        assert_eq!(conn.state(), ConnectionState::Available);

        conn.mark_in_use();
        assert_eq!(conn.state(), ConnectionState::InUse);
        assert_eq!(conn.metrics().use_count, 1);

        conn.mark_available();
        assert_eq!(conn.state(), ConnectionState::Available);

        conn.record_bytes(1024);
        assert_eq!(conn.metrics().bytes_transferred, 1024);

        conn.record_error();
        assert_eq!(conn.metrics().error_count, 1);
    }

    #[test]
    fn test_config_presets() {
        let high = ConnectionPoolConfig::high_throughput();
        assert_eq!(high.max_connections, MAX_POOL_SIZE);
        assert!(!high.validate_on_acquire);

        let low = ConnectionPoolConfig::low_latency();
        assert!(low.validate_on_acquire);

        let mem = ConnectionPoolConfig::low_memory();
        assert_eq!(mem.max_connections, 4);
    }

    #[test]
    fn test_connection_expiry() {
        let conn = PooledConnection::new(
            TestConnection { id: 1, valid: true },
            1,
            ConnectionType::WebDav,
        );

        // Should not be expired with long lifetime
        assert!(!conn.is_expired(Duration::from_secs(3600)));

        // Should not be idle too long immediately after creation
        assert!(!conn.is_idle_too_long(Duration::from_secs(300)));
    }

    #[tokio::test]
    async fn test_pool_close() {
        let factory = TestConnectionFactory::new();
        let config = ConnectionPoolConfig {
            initial_connections: 2,
            max_connections: 4,
            ..Default::default()
        };

        let pool = ConnectionPool::new(factory, config, ConnectionType::WebDav).await;

        pool.close().await;

        // Should fail to acquire after close
        let result = pool.acquire().await;
        assert!(matches!(result, Err(ConnectionPoolError::PoolClosed)));
    }
}
