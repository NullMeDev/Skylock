// Skylock Hybrid - Web Dashboard Backend
// Modern REST API server with WebSocket support for real-time dashboard

use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    http::{header, HeaderMap, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::broadcast,
    time::{interval, Instant},
};
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};
use uuid::Uuid;

// ============================================================================
// CORE TYPES AND STATE
// ============================================================================

/// Application state shared across all handlers
#[derive(Clone)]
pub struct AppState {
    pub backups: Arc<Mutex<HashMap<String, BackupInfo>>>,
    pub system_metrics: Arc<Mutex<SystemMetrics>>,
    pub config: Arc<Mutex<SystemConfig>>,
    pub websocket_tx: Arc<broadcast::Sender<WebSocketMessage>>,
    pub activity_log: Arc<Mutex<Vec<ActivityEntry>>>,
}

/// Backup information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub status: BackupStatus,
    pub created_at: SystemTime,
    pub completed_at: Option<SystemTime>,
    pub path: String,
    pub file_count: u32,
    pub progress: f32, // 0.0 to 100.0
    pub error_message: Option<String>,
    pub backup_type: BackupType,
    pub retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackupType {
    Full,
    Incremental,
    Differential,
    Snapshot,
}

/// System metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub timestamp: SystemTime,
    pub cpu_usage: f32,
    pub memory_usage: MemoryUsage,
    pub disk_usage: DiskUsage,
    pub network_io: NetworkIO,
    pub backup_throughput: BackupThroughput,
    pub active_operations: u32,
    pub system_health: HealthStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUsage {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub cache_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskUsage {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub backup_storage_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkIO {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupThroughput {
    pub bytes_per_second: u64,
    pub files_per_second: u32,
    pub compression_ratio: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

/// System configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub backup_settings: BackupSettings,
    pub storage_settings: StorageSettings,
    pub notification_settings: NotificationSettings,
    pub security_settings: SecuritySettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSettings {
    pub default_retention_days: u32,
    pub max_concurrent_backups: u32,
    pub compression_enabled: bool,
    pub encryption_enabled: bool,
    pub schedule: Option<String>, // Cron expression
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSettings {
    pub backend_type: String,
    pub local_path: Option<String>,
    pub remote_config: Option<HashMap<String, String>>,
    pub max_storage_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSettings {
    pub email_enabled: bool,
    pub webhook_url: Option<String>,
    pub alert_on_failure: bool,
    pub alert_on_success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritySettings {
    pub auth_enabled: bool,
    pub mfa_required: bool,
    pub session_timeout_minutes: u32,
    pub allowed_origins: Vec<String>,
}

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WebSocketMessage {
    BackupProgress { backup_id: String, progress: f32 },
    SystemMetrics(SystemMetrics),
    BackupCompleted { backup_id: String, status: BackupStatus },
    Alert { level: AlertLevel, message: String },
    LogEntry(LogEntry),
    HealthUpdate { component: String, status: HealthStatus },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertLevel {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: SystemTime,
    pub level: LogLevel,
    pub component: String,
    pub message: String,
    pub context: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
}

/// Activity log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub id: String,
    pub timestamp: SystemTime,
    pub action: String,
    pub description: String,
    pub user: Option<String>,
    pub metadata: HashMap<String, String>,
}

// ============================================================================
// API RESPONSE TYPES
// ============================================================================

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub timestamp: SystemTime,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: SystemTime::now(),
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
            timestamp: SystemTime::now(),
        }
    }
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub active_backups: u32,
    pub total_backups: u32,
    pub storage_used_bytes: u64,
    pub system_health: HealthStatus,
}

#[derive(Serialize)]
pub struct BackupListResponse {
    pub backups: Vec<BackupInfo>,
    pub total_count: usize,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Deserialize)]
pub struct CreateBackupRequest {
    pub name: String,
    pub path: String,
    pub backup_type: BackupType,
    pub retention_days: Option<u32>,
    pub schedule: Option<String>,
}

#[derive(Deserialize)]
pub struct BackupListQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    pub status: Option<BackupStatus>,
    pub search: Option<String>,
}

#[derive(Deserialize)]
pub struct RestoreRequest {
    pub backup_id: String,
    pub files: Vec<String>,
    pub destination: String,
    pub overwrite: bool,
}

// ============================================================================
// MAIN APPLICATION
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::init();

    // Create broadcast channel for WebSocket communication
    let (websocket_tx, _) = broadcast::channel(1000);

    // Initialize application state
    let app_state = AppState {
        backups: Arc::new(Mutex::new(HashMap::new())),
        system_metrics: Arc::new(Mutex::new(create_initial_metrics())),
        config: Arc::new(Mutex::new(create_default_config())),
        websocket_tx: Arc::new(websocket_tx.clone()),
        activity_log: Arc::new(Mutex::new(Vec::new())),
    };

    // Populate with sample data
    populate_sample_data(&app_state).await;

    // Start background tasks
    let metrics_state = app_state.clone();
    tokio::spawn(async move {
        metrics_collection_task(metrics_state).await;
    });

    // Build API routes
    let api_routes = Router::new()
        // System endpoints
        .route("/status", get(get_system_status))
        .route("/metrics", get(get_system_metrics))
        .route("/health", get(health_check))
        .route("/logs", get(get_system_logs))
        
        // Backup endpoints
        .route("/backups", get(list_backups))
        .route("/backups", post(create_backup))
        .route("/backups/:id", get(get_backup))
        .route("/backups/:id", delete(delete_backup))
        .route("/backups/:id/files", get(list_backup_files))
        
        // Restore endpoints
        .route("/restore", post(start_restore))
        .route("/restore/:id/status", get(get_restore_status))
        
        // Configuration endpoints
        .route("/config", get(get_configuration))
        .route("/config", put(update_configuration))
        
        // WebSocket endpoint
        .route("/ws", get(websocket_handler))
        
        .with_state(app_state.clone());

    // Main application router
    let app = Router::new()
        .nest("/api/v1", api_routes)
        // Serve static files for frontend
        .fallback_service(ServeDir::new("static"))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(security_headers));

    // Start server
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    println!("🚀 Skylock Web Dashboard starting on http://{}", addr);
    println!("📊 API Documentation: http://{}/api/v1/status", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// ============================================================================
// API HANDLERS
// ============================================================================

/// Get system status overview
async fn get_system_status(State(state): State<AppState>) -> Json<ApiResponse<StatusResponse>> {
    let backups = state.backups.lock().unwrap();
    let metrics = state.system_metrics.lock().unwrap();
    
    let active_backups = backups.values()
        .filter(|b| matches!(b.status, BackupStatus::Running))
        .count() as u32;
    
    let storage_used = backups.values()
        .map(|b| b.size_bytes)
        .sum::<u64>();

    let response = StatusResponse {
        status: "operational".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: 3600, // Placeholder
        active_backups,
        total_backups: backups.len() as u32,
        storage_used_bytes: storage_used,
        system_health: metrics.system_health.clone(),
    };

    Json(ApiResponse::success(response))
}

/// Get current system metrics
async fn get_system_metrics(State(state): State<AppState>) -> Json<ApiResponse<SystemMetrics>> {
    let metrics = state.system_metrics.lock().unwrap().clone();
    Json(ApiResponse::success(metrics))
}

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

/// Get system logs
async fn get_system_logs(State(state): State<AppState>) -> Json<ApiResponse<Vec<LogEntry>>> {
    // Mock log entries
    let logs = vec![
        LogEntry {
            timestamp: SystemTime::now(),
            level: LogLevel::Info,
            component: "backup_engine".to_string(),
            message: "Backup completed successfully".to_string(),
            context: None,
        },
        LogEntry {
            timestamp: SystemTime::now(),
            level: LogLevel::Warning,
            component: "storage".to_string(),
            message: "Storage usage above 80%".to_string(),
            context: None,
        },
    ];
    
    Json(ApiResponse::success(logs))
}

/// List backups with filtering and pagination
async fn list_backups(
    State(state): State<AppState>,
    Query(query): Query<BackupListQuery>,
) -> Json<ApiResponse<BackupListResponse>> {
    let backups = state.backups.lock().unwrap();
    let mut filtered_backups: Vec<_> = backups.values().cloned().collect();

    // Apply status filter
    if let Some(status) = &query.status {
        filtered_backups.retain(|b| std::mem::discriminant(&b.status) == std::mem::discriminant(status));
    }

    // Apply search filter
    if let Some(search) = &query.search {
        let search_lower = search.to_lowercase();
        filtered_backups.retain(|b| {
            b.name.to_lowercase().contains(&search_lower) ||
            b.path.to_lowercase().contains(&search_lower)
        });
    }

    // Sort by created_at (newest first)
    filtered_backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // Apply pagination
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(20).min(100); // Max 100 items per page
    let start_idx = ((page - 1) * page_size) as usize;
    let end_idx = (start_idx + page_size as usize).min(filtered_backups.len());

    let paginated_backups = if start_idx < filtered_backups.len() {
        filtered_backups[start_idx..end_idx].to_vec()
    } else {
        Vec::new()
    };

    let response = BackupListResponse {
        backups: paginated_backups,
        total_count: filtered_backups.len(),
        page,
        page_size,
    };

    Json(ApiResponse::success(response))
}

/// Create a new backup
async fn create_backup(
    State(state): State<AppState>,
    Json(request): Json<CreateBackupRequest>,
) -> Json<ApiResponse<BackupInfo>> {
    let backup_id = Uuid::new_v4().to_string();
    
    let backup = BackupInfo {
        id: backup_id.clone(),
        name: request.name,
        size_bytes: 0,
        status: BackupStatus::Pending,
        created_at: SystemTime::now(),
        completed_at: None,
        path: request.path,
        file_count: 0,
        progress: 0.0,
        error_message: None,
        backup_type: request.backup_type,
        retention_days: request.retention_days.unwrap_or(30),
    };

    // Add to backups
    state.backups.lock().unwrap().insert(backup_id.clone(), backup.clone());

    // Log activity
    log_activity(&state, "backup_created", &format!("Created backup: {}", backup.name), None).await;

    // Start backup simulation in background
    let backup_state = state.clone();
    let backup_id_clone = backup_id.clone();
    tokio::spawn(async move {
        simulate_backup_progress(backup_state, backup_id_clone).await;
    });

    Json(ApiResponse::success(backup))
}

/// Get backup details
async fn get_backup(
    State(state): State<AppState>,
    Path(backup_id): Path<String>,
) -> Result<Json<ApiResponse<BackupInfo>>, StatusCode> {
    let backups = state.backups.lock().unwrap();
    
    match backups.get(&backup_id) {
        Some(backup) => Ok(Json(ApiResponse::success(backup.clone()))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// Delete a backup
async fn delete_backup(
    State(state): State<AppState>,
    Path(backup_id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let mut backups = state.backups.lock().unwrap();
    
    match backups.remove(&backup_id) {
        Some(backup) => {
            drop(backups);
            log_activity(&state, "backup_deleted", &format!("Deleted backup: {}", backup.name), None).await;
            Ok(StatusCode::NO_CONTENT)
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// List files in a backup
async fn list_backup_files(
    State(_state): State<AppState>,
    Path(backup_id): Path<String>,
) -> Json<ApiResponse<Vec<String>>> {
    // Mock file list
    let files = vec![
        "/home/user/documents/report.pdf".to_string(),
        "/home/user/documents/presentation.pptx".to_string(),
        "/home/user/photos/vacation.jpg".to_string(),
        "/home/user/code/project/main.rs".to_string(),
    ];
    
    Json(ApiResponse::success(files))
}

/// Start restore operation
async fn start_restore(
    State(state): State<AppState>,
    Json(request): Json<RestoreRequest>,
) -> Json<ApiResponse<String>> {
    let restore_id = Uuid::new_v4().to_string();
    
    // Log activity
    log_activity(
        &state,
        "restore_started",
        &format!("Started restore operation for backup: {}", request.backup_id),
        None
    ).await;

    Json(ApiResponse::success(restore_id))
}

/// Get restore operation status
async fn get_restore_status(
    State(_state): State<AppState>,
    Path(restore_id): Path<String>,
) -> Json<ApiResponse<HashMap<String, String>>> {
    let mut status = HashMap::new();
    status.insert("restore_id".to_string(), restore_id);
    status.insert("status".to_string(), "in_progress".to_string());
    status.insert("progress".to_string(), "45.2".to_string());
    
    Json(ApiResponse::success(status))
}

/// Get system configuration
async fn get_configuration(State(state): State<AppState>) -> Json<ApiResponse<SystemConfig>> {
    let config = state.config.lock().unwrap().clone();
    Json(ApiResponse::success(config))
}

/// Update system configuration
async fn update_configuration(
    State(state): State<AppState>,
    Json(new_config): Json<SystemConfig>,
) -> Json<ApiResponse<SystemConfig>> {
    *state.config.lock().unwrap() = new_config.clone();
    
    log_activity(&state, "config_updated", "System configuration updated", None).await;
    
    Json(ApiResponse::success(new_config))
}

/// WebSocket handler for real-time updates
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(|socket| websocket_connection(socket, state))
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Create initial system metrics
fn create_initial_metrics() -> SystemMetrics {
    SystemMetrics {
        timestamp: SystemTime::now(),
        cpu_usage: 15.5,
        memory_usage: MemoryUsage {
            total_bytes: 16 * 1024 * 1024 * 1024, // 16GB
            used_bytes: 8 * 1024 * 1024 * 1024,  // 8GB
            available_bytes: 8 * 1024 * 1024 * 1024, // 8GB
            cache_bytes: 2 * 1024 * 1024 * 1024, // 2GB
        },
        disk_usage: DiskUsage {
            total_bytes: 1024 * 1024 * 1024 * 1024, // 1TB
            used_bytes: 800 * 1024 * 1024 * 1024,   // 800GB
            available_bytes: 224 * 1024 * 1024 * 1024, // 224GB
            backup_storage_bytes: 500 * 1024 * 1024 * 1024, // 500GB
        },
        network_io: NetworkIO {
            bytes_sent: 1024 * 1024 * 100, // 100MB
            bytes_received: 1024 * 1024 * 150, // 150MB
            packets_sent: 50000,
            packets_received: 75000,
        },
        backup_throughput: BackupThroughput {
            bytes_per_second: 1024 * 1024 * 50, // 50MB/s
            files_per_second: 250,
            compression_ratio: 0.7,
        },
        active_operations: 2,
        system_health: HealthStatus::Healthy,
    }
}

/// Create default system configuration
fn create_default_config() -> SystemConfig {
    SystemConfig {
        backup_settings: BackupSettings {
            default_retention_days: 30,
            max_concurrent_backups: 3,
            compression_enabled: true,
            encryption_enabled: true,
            schedule: Some("0 2 * * *".to_string()), // Daily at 2 AM
        },
        storage_settings: StorageSettings {
            backend_type: "local".to_string(),
            local_path: Some("/var/lib/skylock/backups".to_string()),
            remote_config: None,
            max_storage_bytes: Some(1024 * 1024 * 1024 * 1024), // 1TB
        },
        notification_settings: NotificationSettings {
            email_enabled: true,
            webhook_url: None,
            alert_on_failure: true,
            alert_on_success: false,
        },
        security_settings: SecuritySettings {
            auth_enabled: true,
            mfa_required: false,
            session_timeout_minutes: 60,
            allowed_origins: vec!["http://localhost:3000".to_string()],
        },
    }
}

/// Populate sample data
async fn populate_sample_data(state: &AppState) {
    let sample_backups = vec![
        BackupInfo {
            id: "backup-001".to_string(),
            name: "Documents Backup".to_string(),
            size_bytes: 2_500_000_000, // 2.5GB
            status: BackupStatus::Completed,
            created_at: SystemTime::now() - Duration::from_secs(7200), // 2 hours ago
            completed_at: Some(SystemTime::now() - Duration::from_secs(7000)),
            path: "/home/user/documents".to_string(),
            file_count: 1247,
            progress: 100.0,
            error_message: None,
            backup_type: BackupType::Full,
            retention_days: 30,
        },
        BackupInfo {
            id: "backup-002".to_string(),
            name: "Full System Backup".to_string(),
            size_bytes: 45_000_000_000, // 45GB
            status: BackupStatus::Running,
            created_at: SystemTime::now() - Duration::from_secs(300), // 5 minutes ago
            completed_at: None,
            path: "/".to_string(),
            file_count: 0,
            progress: 65.2,
            error_message: None,
            backup_type: BackupType::Full,
            retention_days: 90,
        },
        BackupInfo {
            id: "backup-003".to_string(),
            name: "Photos Archive".to_string(),
            size_bytes: 12_000_000_000, // 12GB
            status: BackupStatus::Failed,
            created_at: SystemTime::now() - Duration::from_secs(86400), // 1 day ago
            completed_at: None,
            path: "/home/user/photos".to_string(),
            file_count: 3492,
            progress: 23.8,
            error_message: Some("Insufficient disk space".to_string()),
            backup_type: BackupType::Incremental,
            retention_days: 365,
        },
    ];

    let mut backups = state.backups.lock().unwrap();
    for backup in sample_backups {
        backups.insert(backup.id.clone(), backup);
    }
}

/// Background task for metrics collection
async fn metrics_collection_task(state: AppState) {
    let mut interval = interval(Duration::from_secs(5)); // Update every 5 seconds
    
    loop {
        interval.tick().await;
        
        // Update metrics with some randomness to simulate real data
        let mut metrics = state.system_metrics.lock().unwrap();
        metrics.timestamp = SystemTime::now();
        metrics.cpu_usage = 10.0 + (rand::random::<f32>() * 20.0); // 10-30%
        metrics.memory_usage.used_bytes += rand::random::<u64>() % (1024 * 1024 * 100); // Fluctuate
        
        // Send metrics via WebSocket
        let ws_message = WebSocketMessage::SystemMetrics(metrics.clone());
        let _ = state.websocket_tx.send(ws_message);
        
        drop(metrics);
    }
}

/// Simulate backup progress
async fn simulate_backup_progress(state: AppState, backup_id: String) {
    let mut interval = interval(Duration::from_millis(500));
    
    // Update status to running
    {
        let mut backups = state.backups.lock().unwrap();
        if let Some(backup) = backups.get_mut(&backup_id) {
            backup.status = BackupStatus::Running;
        }
    }
    
    for progress in (0..=100).step_by(5) {
        interval.tick().await;
        
        // Update progress
        {
            let mut backups = state.backups.lock().unwrap();
            if let Some(backup) = backups.get_mut(&backup_id) {
                backup.progress = progress as f32;
                backup.size_bytes += rand::random::<u64>() % (1024 * 1024 * 10); // Simulate size increase
                backup.file_count += rand::random::<u32>() % 50;
            }
        }
        
        // Send progress update via WebSocket
        let ws_message = WebSocketMessage::BackupProgress {
            backup_id: backup_id.clone(),
            progress: progress as f32,
        };
        let _ = state.websocket_tx.send(ws_message);
    }
    
    // Mark as completed
    {
        let mut backups = state.backups.lock().unwrap();
        if let Some(backup) = backups.get_mut(&backup_id) {
            backup.status = BackupStatus::Completed;
            backup.completed_at = Some(SystemTime::now());
            backup.progress = 100.0;
        }
    }
    
    // Send completion notification
    let ws_message = WebSocketMessage::BackupCompleted {
        backup_id: backup_id.clone(),
        status: BackupStatus::Completed,
    };
    let _ = state.websocket_tx.send(ws_message);
}

/// Log activity
async fn log_activity(
    state: &AppState,
    action: &str,
    description: &str,
    user: Option<String>,
) {
    let entry = ActivityEntry {
        id: Uuid::new_v4().to_string(),
        timestamp: SystemTime::now(),
        action: action.to_string(),
        description: description.to_string(),
        user,
        metadata: HashMap::new(),
    };
    
    state.activity_log.lock().unwrap().push(entry);
}

/// WebSocket connection handler
async fn websocket_connection(
    socket: axum::extract::ws::WebSocket,
    state: AppState,
) {
    use axum::extract::ws::Message;
    use futures_util::{SinkExt, StreamExt};
    
    let (mut sender, mut receiver) = socket.split();
    let mut ws_rx = state.websocket_tx.subscribe();
    
    // Spawn task to handle WebSocket messages from client
    let recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    println!("Received WebSocket message: {}", text);
                }
                Ok(Message::Close(_)) => break,
                Err(e) => {
                    println!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });
    
    // Spawn task to send broadcast messages to client
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = ws_rx.recv().await {
            let json_msg = serde_json::to_string(&msg).unwrap();
            if sender.send(Message::Text(json_msg)).await.is_err() {
                break;
            }
        }
    });
    
    // Wait for either task to complete
    tokio::select! {
        _ = recv_task => {},
        _ = send_task => {},
    }
}

/// Security headers middleware
async fn security_headers<B>(
    request: axum::http::Request<B>,
    next: axum::middleware::Next<B>,
) -> Response {
    let mut response = next.run(request).await;
    
    let headers = response.headers_mut();
    headers.insert("X-Frame-Options", "DENY".parse().unwrap());
    headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
    headers.insert("Referrer-Policy", "strict-origin-when-cross-origin".parse().unwrap());
    headers.insert(
        "Content-Security-Policy",
        "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'".parse().unwrap(),
    );
    
    response
}