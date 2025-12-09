use axum::{
    extract::State,
    http::{header::CONTENT_TYPE, Method},
    middleware,
    response::IntoResponse,
    routing::{get, post, put, delete},
    Router,
};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tokio;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tracing::{event, Level};
use tracing_subscriber;

use taskforge::{
    config::get_settings,
    database::establish_connection,
    middleware::auth::auth_middleware,
    routes::{
        auth_routes::create_auth_router,
        job_routes::create_job_router,
        queue_routes::create_queue_router,
        worker_routes::create_worker_router,
        api_key_routes::create_api_key_router,
        metrics_routes::create_metrics_router,
    },
    services::{
        authentication_service::AuthenticationService,
        authorization_service::AuthorizationService,
        job_service::JobService,
        queue_service::QueueService,
        worker_service::WorkerService,
        api_key_service::ApiKeyService,
        execution_service::ExecutionService,
        retry_service::RetryService,
        monitoring_service::MonitoringService,
    },
};

// Import semua model
use taskforge::models::*;

// Import konfigurasi
use taskforge::config::Settings;

// Import error types
use taskforge::errors::AppError;

// Import utility functions
use taskforge::utils::logger;

// Import database schema (jika menggunakan SQLX)
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Inisialisasi logger
    tracing_subscriber::fmt::init();
    
    // Dapatkan konfigurasi aplikasi
    let settings = get_settings().expect("Failed to read configuration");
    
    // Buat koneksi database
    let db_pool = establish_connection(&settings.database.connection_string().expose_secret())
        .await
        .expect("Failed to connect to database");
    
    // Jalankan migrasi database
    run_database_migrations(&db_pool).await?;
    
    // Inisialisasi layanan
    let auth_service = Arc::new(AuthenticationService::new(
        settings.authentication.clone(),
    ));
    
    let authz_service = Arc::new(AuthorizationService::new(
        db_pool.clone(),
        auth_service.clone(),
    ));
    
    let job_service = Arc::new(JobService::new(
        db_pool.clone(),
        authz_service.clone(),
    ));
    
    let queue_service = Arc::new(QueueService::new(
        db_pool.clone(),
        authz_service.clone(),
    ));
    
    let worker_service = Arc::new(WorkerService::new(
        db_pool.clone(),
        authz_service.clone(),
    ));
    
    let api_key_service = Arc::new(ApiKeyService::new(
        db_pool.clone(),
        authz_service.clone(),
    ));
    
    let execution_service = Arc::new(ExecutionService::new(
        db_pool.clone(),
        job_service.clone(),
        worker_service.clone(),
    ));
    
    let retry_service = Arc::new(RetryService::new(
        db_pool.clone(),
        job_service.clone(),
        queue_service.clone(),
    ));
    
    let monitoring_service = Arc::new(MonitoringService::new(
        db_pool.clone(),
    ));
    
    // Buat router utama
    let app = Router::new()
        // Rute kesehatan
        .route("/health", get(health_check))
        .route("/ready", get(ready_check))
        
        // API v1 routes
        .nest("/api/v1/auth", create_auth_router(auth_service.clone(), authz_service.clone()))
        .nest("/api/v1/jobs", create_job_router(job_service.clone(), authz_service.clone()))
        .nest("/api/v1/queues", create_queue_router(queue_service.clone(), authz_service.clone()))
        .nest("/api/v1/workers", create_worker_router(worker_service.clone(), authz_service.clone()))
        .nest("/api/v1/api-keys", create_api_key_router(api_key_service.clone(), authz_service.clone()))
        .nest("/api/v1/metrics", create_metrics_router(monitoring_service.clone()))
        
        // Middleware global
        .layer(
            ServiceBuilder::new()
                .layer(
                    CorsLayer::new()
                        .allow_origin(Any)
                        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
                        .allow_headers(Any)
                        .allow_credentials(true),
                )
                .layer(middleware::from_fn_with_state(
                    Arc::clone(&auth_service),
                    auth_middleware,
                ))
        )
        .with_state(AppState {
            db_pool,
            auth_service,
            authz_service,
            job_service,
            queue_service,
            worker_service,
            api_key_service,
            execution_service,
            retry_service,
            monitoring_service,
        });
    
    // Jalankan server
    let listener = tokio::net::TcpListener::bind(&settings.application.socket_addr())
        .await
        .expect("Failed to bind to address");
    
    event!(Level::INFO, "Starting server on {}", settings.application.base_url());
    
    axum::serve(listener, app)
        .await
        .expect("Failed to start server");
    
    Ok(())
}

async fn run_database_migrations(db_pool: &PgPool) -> Result<(), sqlx::Error> {
    event!(Level::INFO, "Running database migrations...");
    sqlx::migrate!("./migrations").run(db_pool).await?;
    event!(Level::INFO, "Database migrations completed successfully");
    Ok(())
}

async fn health_check() -> impl IntoResponse {
    (axum::http::StatusCode::OK, "OK")
}

async fn ready_check(State(state): State<AppState>) -> impl IntoResponse {
    // Cek koneksi database
    match sqlx::query("SELECT 1").fetch_one(&state.db_pool).await {
        Ok(_) => (axum::http::StatusCode::OK, "Ready"),
        Err(_) => (axum::http::StatusCode::SERVICE_UNAVAILABLE, "Not Ready"),
    }
}

// Struktur state aplikasi
#[derive(Clone)]
pub struct AppState {
    pub db_pool: PgPool,
    pub auth_service: Arc<AuthenticationService>,
    pub authz_service: Arc<AuthorizationService>,
    pub job_service: Arc<JobService>,
    pub queue_service: Arc<QueueService>,
    pub worker_service: Arc<WorkerService>,
    pub api_key_service: Arc<ApiKeyService>,
    pub execution_service: Arc<ExecutionService>,
    pub retry_service: Arc<RetryService>,
    pub monitoring_service: Arc<MonitoringService>,
}

// Handler error global
pub async fn handle_error(err: AppError) -> impl IntoResponse {
    use axum::Json;
    use serde_json::json;
    
    let status_code = match err {
        AppError::BadRequest(_) => axum::http::StatusCode::BAD_REQUEST,
        AppError::Unauthorized(_) => axum::http::StatusCode::UNAUTHORIZED,
        AppError::Forbidden(_) => axum::http::StatusCode::FORBIDDEN,
        AppError::NotFound(_) => axum::http::StatusCode::NOT_FOUND,
        AppError::InternalServerError(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        AppError::ValidationError(_) => axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        AppError::RateLimitExceeded(_) => axum::http::StatusCode::TOO_MANY_REQUESTS,
    };
    
    let error_response = json!({
        "success": false,
        "error": {
            "code": status_code.as_u16(),
            "message": err.to_string(),
        },
        "timestamp": chrono::Utc::now().to_rfc3339()
    });
    
    (status_code, Json(error_response))
}

// Konstanta aplikasi
pub const API_VERSION: &str = "v1";
pub const SERVICE_NAME: &str = "TaskForge";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tokio;
    use tower::ServiceExt; // for `oneshot`
    
    #[tokio::test]
    async fn test_health_endpoint() {
        let app = create_test_app().await;
        
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        
        assert_eq!(response.status(), StatusCode::OK);
    }
    
    async fn create_test_app() -> Router {
        // Dalam implementasi nyata, ini akan membuat aplikasi dengan konfigurasi test
        Router::new().route("/health", get(health_check))
    }
}