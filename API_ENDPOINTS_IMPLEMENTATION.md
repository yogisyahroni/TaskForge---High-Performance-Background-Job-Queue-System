# Implementasi API Endpoints (REST dan gRPC) untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan implementasi API endpoints untuk aplikasi TaskForge, mencakup baik REST API maupun gRPC endpoints. Sistem ini menyediakan antarmuka untuk berinteraksi dengan berbagai komponen sistem seperti queue, job, worker, dan organisasi.

## 2. Arsitektur API

### 2.1. Pendekatan API-First

TaskForge menerapkan pendekatan API-first dengan desain yang konsisten dan dokumentasi yang komprehensif. API dirancang untuk:

- Menyediakan akses programatik ke semua fitur utama sistem
- Mendukung berbagai bahasa pemrograman klien
- Menyediakan kinerja tinggi melalui gRPC untuk komunikasi internal
- Menyediakan kemudahan penggunaan melalui REST API untuk integrasi eksternal

### 2.2. Struktur API

```
API Gateway
├── /api/v1/
│   ├── /auth/          # Otentikasi dan otorisasi
│   ├── /organizations/ # Manajemen organisasi
│   ├── /projects/      # Manajemen proyek
│   ├── /queues/        # Manajemen queue
│   ├── /jobs/          # Manajemen job
│   ├── /workers/       # Manajemen worker
│   ├── /api-keys/      # Manajemen API key
│   └── /metrics/       # Endpoint metrik
└── /grpc/              # gRPC endpoints
```

## 3. Implementasi REST API

### 3.1. Struktur Dasar REST API

```rust
// File: src/api/mod.rs
pub mod auth;
pub mod organizations;
pub mod projects;
pub mod queues;
pub mod jobs;
pub mod workers;
pub mod api_keys;
pub mod metrics;

use axum::{
    extract::State,
    http::Method,
    middleware,
    routing::{delete, get, patch, post},
    Router,
};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};

use crate::{
    services::{
        authentication_service::AuthenticationService,
        authorization_service::AuthorizationService,
        metrics_service::MetricsService,
    },
    middleware::{auth_middleware, metrics_middleware},
};

pub fn create_api_router(
    auth_service: Arc<AuthenticationService>,
    authz_service: Arc<AuthorizationService>,
    metrics_service: Arc<MetricsService>,
) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::PATCH])
        .allow_headers(Any);

    let middleware_stack = ServiceBuilder::new()
        .layer(cors)
        .layer(middleware::from_fn_with_state(
            Arc::clone(&metrics_service),
            metrics_middleware,
        ));

    Router::new()
        .nest("/auth", auth::create_auth_router(auth_service))
        .nest("/organizations", organizations::create_router(authz_service.clone()))
        .nest("/projects", projects::create_router(authz_service.clone()))
        .nest("/queues", queues::create_router(authz_service.clone()))
        .nest("/jobs", jobs::create_router(authz_service.clone()))
        .nest("/workers", workers::create_router(authz_service.clone()))
        .nest("/api-keys", api_keys::create_router(authz_service.clone()))
        .route("/metrics", get(metrics::metrics_handler))
        .layer(middleware_stack)
}
```

### 3.2. Endpoint Otentikasi

```rust
// File: src/api/auth.rs
use axum::{
    extract::{State, Json},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    models::{User, UserRole},
    services::authentication_service::{AuthenticationService, LoginRequest, LoginResponse},
};

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub organization_name: String,
}

#[derive(Serialize)]
pub struct RegisterResponse {
    pub user_id: Uuid,
    pub organization_id: Uuid,
    pub message: String,
}

pub fn create_auth_router(auth_service: Arc<AuthenticationService>) -> Router {
    Router::new()
        .route("/login", post(login_handler))
        .route("/register", post(register_handler))
        .route("/refresh", post(refresh_handler))
        .route("/validate", get(validate_token_handler))
        .with_state(auth_service)
}

pub async fn login_handler(
    State(auth_service): State<Arc<AuthenticationService>>,
    Json(request): Json<LoginRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    match auth_service.authenticate_user(request).await {
        Ok(response) => Ok(Json(response)),
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}

pub async fn register_handler(
    State(auth_service): State<Arc<AuthenticationService>>,
    Json(request): Json<RegisterRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    match auth_service.register_user(
        &request.email,
        &request.password,
        &request.organization_name,
    ).await {
        Ok(user) => Ok(Json(RegisterResponse {
            user_id: user.id,
            organization_id: user.organization_id,
            message: "User registered successfully".to_string(),
        })),
        Err(_) => Err(StatusCode::BAD_REQUEST),
    }
}

pub async fn refresh_handler(
    State(auth_service): State<Arc<AuthenticationService>>,
    Json(payload): Json<serde_json::Value>,
) -> Result<impl IntoResponse, StatusCode> {
    let refresh_token = payload["refresh_token"]
        .as_str()
        .ok_or(StatusCode::BAD_REQUEST)?;
    
    match auth_service.refresh_token(refresh_token).await {
        Ok(new_token) => Ok(Json(serde_json::json!({
            "access_token": new_token,
            "token_type": "Bearer"
        }))),
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}

pub async fn validate_token_handler(
    State(auth_service): State<Arc<AuthenticationService>>,
    auth_header: axum::http::HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    let auth_header = auth_header
        .get("authorization")
        .and_then(|header| header.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(StatusCode::UNAUTHORIZED)?
        .trim();
    
    match auth_service.validate_token(token).await {
        Ok(claims) => Ok(Json(serde_json::json!({
            "valid": true,
            "user_id": claims.sub,
            "organization_id": claims.org,
            "expires_at": claims.exp
        }))),
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}
```

### 3.3. Endpoint Queue dan Job

```rust
// File: src/api/queues.rs
use axum::{
    extract::{Path, Query, State, Json},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    models::JobQueue,
    services::{
        authorization_service::AuthorizationService,
        queue_service::{QueueService, QueueStats},
    },
};

#[derive(Deserialize)]
pub struct CreateQueueRequest {
    pub name: String,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub settings: serde_json::Value,
}

#[derive(Deserialize)]
pub struct UpdateQueueRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub settings: Option<serde_json::Value>,
    pub is_active: Option<bool>,
}

#[derive(Deserialize)]
pub struct GetQueuesQuery {
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

pub fn create_router(authz_service: Arc<AuthorizationService>) -> Router {
    Router::new()
        .route("/", get(get_queues_handler).post(create_queue_handler))
        .route(
            "/:queue_id",
            get(get_queue_handler)
                .put(update_queue_handler)
                .delete(delete_queue_handler),
        )
        .route("/:queue_id/stats", get(get_queue_stats_handler))
        .route("/:queue_id/pause", post(pause_queue_handler))
        .route("/:queue_id/resume", post(resume_queue_handler))
        .with_state(authz_service)
}

pub async fn get_queues_handler(
    State(authz_service): State<Arc<AuthorizationService>>,
    Query(query): Query<GetQueuesQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    // Implementasi untuk mendapatkan daftar queue
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    let page = query.page.unwrap_or(1);
    let limit = query.limit.unwrap_or(10).min(10); // Maks 100 per halaman
    
    match authz_service.get_queues_for_user(user.id, page, limit).await {
        Ok(queues) => Ok(Json(queues)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn create_queue_handler(
    State(authz_service): State<Arc<AuthorizationService>>,
    Json(request): Json<CreateQueueRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "queue:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let queue = JobQueue::new(
        user.organization_id,
        request.name,
        request.description,
        request.priority.unwrap_or(0),
        request.settings,
    );
    
    match authz_service.create_queue(queue).await {
        Ok(created_queue) => Ok(Json(created_queue)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_queue_handler(
    State(authz_service): State<Arc<AuthorizationService>>,
    Path(queue_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "queue:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match authz_service.get_queue_by_id(queue_id).await {
        Ok(Some(queue)) => {
            // Pastikan queue milik organisasi user
            if queue.project_id != user.organization_id {
                return Err(StatusCode::FORBIDDEN);
            }
            Ok(Json(queue))
        },
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn update_queue_handler(
    State(authz_service): State<Arc<AuthorizationService>>,
    Path(queue_id): Path<Uuid>,
    Json(request): Json<UpdateQueueRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "queue:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match authz_service.get_queue_by_id(queue_id).await {
        Ok(Some(mut queue)) => {
            // Pastikan queue milik organisasi user
            if queue.project_id != user.organization_id {
                return Err(StatusCode::FORBIDDEN);
            }
            
            // Update field yang disediakan
            if let Some(name) = request.name {
                queue.name = name;
            }
            if let Some(description) = request.description {
                queue.description = Some(description);
            }
            if let Some(priority) = request.priority {
                queue.priority = priority;
            }
            if let Some(settings) = request.settings {
                queue.settings = settings;
            }
            if let Some(is_active) = request.is_active {
                queue.is_active = is_active;
            }
            
            match authz_service.update_queue(queue).await {
                Ok(updated_queue) => Ok(Json(updated_queue)),
                Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
            }
        },
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn delete_queue_handler(
    State(authz_service): State<Arc<AuthorizationService>>,
    Path(queue_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "queue:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match authz_service.get_queue_by_id(queue_id).await {
        Ok(Some(queue)) => {
            // Pastikan queue milik organisasi user
            if queue.project_id != user.organization_id {
                return Err(StatusCode::FORBIDDEN);
            }
            
            // Pastikan tidak ada job aktif di queue
            if authz_service.has_active_jobs(queue_id).await? {
                return Err(StatusCode::CONFLICT);
            }
            
            match authz_service.delete_queue(queue_id).await {
                Ok(_) => Ok(StatusCode::NO_CONTENT),
                Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
            }
        },
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_queue_stats_handler(
    State(authz_service): State<Arc<AuthorizationService>>,
    Path(queue_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "queue:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match authz_service.get_queue_stats(queue_id).await {
        Ok(Some(stats)) => Ok(Json(stats)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn pause_queue_handler(
    State(authz_service): State<Arc<AuthorizationService>>,
    Path(queue_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "queue:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match authz_service.pause_queue(queue_id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn resume_queue_handler(
    State(authz_service): State<Arc<AuthorizationService>>,
    Path(queue_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "queue:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match authz_service.resume_queue(queue_id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
```

### 3.4. Endpoint Job

```rust
// File: src/api/jobs.rs
use axum::{
    extract::{Path, Query, State, Json},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    models::{Job, JobStatus},
    services::{
        authorization_service::AuthorizationService,
        job_service::{JobService, JobStats},
    },
};

#[derive(Deserialize)]
pub struct CreateJobRequest {
    pub queue_name: String,
    pub job_type: String,
    pub payload: serde_json::Value,
    pub priority: Option<i32>,
    pub max_attempts: Option<i32>,
    pub scheduled_for: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Deserialize)]
pub struct GetJobsQuery {
    pub status: Option<String>,
    pub queue_id: Option<Uuid>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

pub fn create_router(authz_service: Arc<AuthorizationService>) -> Router {
    Router::new()
        .route("/", post(create_job_handler))
        .route("/", get(get_jobs_handler))
        .route("/:job_id", get(get_job_handler))
        .route("/:job_id/cancel", post(cancel_job_handler))
        .route("/:job_id/retry", post(retry_job_handler))
        .route("/:job_id/stats", get(get_job_stats_handler))
        .with_state(authz_service)
}

pub async fn create_job_handler(
    State(authz_service): State<Arc<AuthorizationService>>,
    Json(request): Json<CreateJobRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "job:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Dapatkan queue berdasarkan nama dan organisasi
    let queue = authz_service
        .get_queue_by_name_and_org(&request.queue_name, user.organization_id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;
    
    let job = if let Some(scheduled_for) = request.scheduled_for {
        Job::new_scheduled(
            queue.id,
            request.job_type,
            request.payload,
            request.priority.unwrap_or(0),
            scheduled_for,
            request.max_attempts,
        )
    } else {
        Job::new(
            queue.id,
            request.job_type,
            request.payload,
            request.priority.unwrap_or(0),
            request.max_attempts,
        )
    };
    
    match authz_service.create_job(job).await {
        Ok(created_job) => Ok(Json(created_job)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_jobs_handler(
    State(authz_service): State<Arc<AuthorizationService>>,
    Query(query): Query<GetJobsQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "job:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let page = query.page.unwrap_or(1);
    let limit = query.limit.unwrap_or(10).min(100); // Maks 100 per halaman
    let status = query.status.and_then(|s| s.parse::<JobStatus>().ok());
    
    match authz_service.get_jobs(user.organization_id, status, query.queue_id, page, limit).await {
        Ok(jobs) => Ok(Json(jobs)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_job_handler(
    State(authz_service): State<Arc<AuthorizationService>>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "job:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match authz_service.get_job_by_id(job_id).await {
        Ok(Some(job)) => {
            // Pastikan job milik organisasi user
            let queue = authz_service.get_queue_by_id(job.queue_id).await
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            
            if queue.project_id != user.organization_id {
                return Err(StatusCode::FORBIDDEN);
            }
            
            Ok(Json(job))
        },
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn cancel_job_handler(
    State(authz_service): State<Arc<AuthorizationService>>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "job:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match authz_service.get_job_by_id(job_id).await {
        Ok(Some(job)) => {
            // Pastikan job milik organisasi user
            let queue = authz_service.get_queue_by_id(job.queue_id).await
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            
            if queue.project_id != user.organization_id {
                return Err(StatusCode::FORBIDDEN);
            }
            
            // Hanya job yang belum selesai yang bisa dibatalkan
            if matches!(job.status, JobStatus::Pending | JobStatus::Scheduled) {
                match authz_service.cancel_job(job_id).await {
                    Ok(_) => Ok(StatusCode::NO_CONTENT),
                    Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
                }
            } else {
                Err(StatusCode::CONFLICT) // Job sedang diproses atau sudah selesai
            }
        },
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn retry_job_handler(
    State(authz_service): State<Arc<AuthorizationService>>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "job:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match authz_service.get_job_by_id(job_id).await {
        Ok(Some(job)) => {
            // Pastikan job milik organisasi user
            let queue = authz_service.get_queue_by_id(job.queue_id).await
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
            
            if queue.project_id != user.organization_id {
                return Err(StatusCode::FORBIDDEN);
            }
            
            // Hanya job yang gagal yang bisa diretry
            if matches!(job.status, JobStatus::Failed) && job.can_be_retried() {
                match authz_service.retry_job(job_id).await {
                    Ok(_) => Ok(StatusCode::NO_CONTENT),
                    Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
                }
            } else {
                Err(StatusCode::CONFLICT) // Job tidak dalam status yang bisa diretry
            }
        },
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_job_stats_handler(
    State(authz_service): State<Arc<AuthorizationService>>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "job:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match authz_service.get_job_stats(job_id).await {
        Ok(Some(stats)) => Ok(Json(stats)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
```

## 4. Implementasi gRPC API

### 4.1. Definisi Protobuf

```protobuf
// File: proto/job_queue.proto
syntax = "proto3";

package taskforge;

// Import untuk mendukung UUID dan timestamp
import "google/protobuf/timestamp.proto";
import "google/protobuf/struct.proto";

// Servis utama untuk manajemen job queue
service JobQueueService {
  // Job Operations
  rpc SubmitJob(SubmitJobRequest) returns (SubmitJobResponse);
  rpc GetJob(GetJobRequest) returns (GetJobResponse);
  rpc CancelJob(CancelJobRequest) returns (CancelJobResponse);
  rpc RetryJob(RetryJobRequest) returns (RetryJobResponse);
  rpc ListJobs(ListJobsRequest) returns (ListJobsResponse);
  
  // Queue Operations
  rpc CreateQueue(CreateQueueRequest) returns (CreateQueueResponse);
  rpc GetQueue(GetQueueRequest) returns (GetQueueResponse);
  rpc UpdateQueue(UpdateQueueRequest) returns (UpdateQueueResponse);
  rpc DeleteQueue(DeleteQueueRequest) returns (DeleteQueueResponse);
  rpc ListQueues(ListQueuesRequest) returns (ListQueuesResponse);
  rpc GetQueueStats(GetQueueStatsRequest) returns (GetQueueStatsResponse);
  
  // Worker Operations
  rpc RegisterWorker(RegisterWorkerRequest) returns (RegisterWorkerResponse);
  rpc Heartbeat(HeartbeatRequest) returns (HeartbeatResponse);
  rpc GetWorker(GetWorkerRequest) returns (GetWorkerResponse);
  rpc ListWorkers(ListWorkersRequest) returns (ListWorkersResponse);
}

// Enumerasi status job
enum JobStatus {
  PENDING = 0;
  SCHEDULED = 1;
  PROCESSING = 2;
  SUCCEEDED = 3;
  FAILED = 4;
  CANCELLED = 5;
}

// Enumerasi tipe worker
enum WorkerType {
  GENERAL = 0;
  SPECIALIZED_CPU = 1;
  SPECIALIZED_IO = 2;
  SPECIALIZED_GPU = 3;
}

// Enumerasi status worker
enum WorkerStatus {
  ONLINE = 0;
  OFFLINE = 1;
  DRAINING = 2;
  UNHEALTHY = 3;
}

// Pesan untuk job
message Job {
  string id = 1;
  string queue_id = 2;
  string job_type = 3;
  google.protobuf.Value payload = 4;
  JobStatus status = 5;
  int32 priority = 6;
  int32 max_attempts = 7;
  int32 attempt_count = 8;
  google.protobuf.Timestamp scheduled_for = 9;
  google.protobuf.Timestamp timeout_at = 10;
  google.protobuf.Timestamp created_at = 11;
  google.protobuf.Timestamp updated_at = 12;
  google.protobuf.Timestamp completed_at = 13;
}

// Pesan untuk queue
message JobQueue {
  string id = 1;
  string project_id = 2;
  string name = 3;
  string description = 4;
  int32 priority = 5;
  google.protobuf.Value settings = 6;
  bool is_active = 7;
  google.protobuf.Timestamp created_at = 8;
  google.protobuf.Timestamp updated_at = 9;
}

// Pesan untuk worker
message Worker {
  string id = 1;
  string project_id = 2;
  string name = 3;
  WorkerType worker_type = 4;
  WorkerStatus status = 5;
  google.protobuf.Value capabilities = 6;
  uint32 max_concurrent_jobs = 7;
  google.protobuf.Timestamp last_heartbeat = 8;
  google.protobuf.Timestamp registered_at = 9;
  bool drain_mode = 10;
}

// Request dan Response untuk job
message SubmitJobRequest {
  string queue_id = 1;
  string job_type = 2;
  google.protobuf.Value payload = 3;
  int32 priority = 4;
  int32 max_attempts = 5;
  google.protobuf.Timestamp scheduled_for = 6;
}

message SubmitJobResponse {
  Job job = 1;
}

message GetJobRequest {
  string id = 1;
}

message GetJobResponse {
  Job job = 1;
}

message CancelJobRequest {
  string id = 1;
}

message CancelJobResponse {
  bool success = 1;
}

message RetryJobRequest {
  string id = 1;
}

message RetryJobResponse {
  bool success = 1;
}

message ListJobsRequest {
  string queue_id = 1;
  JobStatus status = 2;
  uint32 page = 3;
  uint32 limit = 4;
}

message ListJobsResponse {
  repeated Job jobs = 1;
  uint32 total = 2;
  uint32 page = 3;
  uint32 pages = 4;
}

// Request dan Response untuk queue
message CreateQueueRequest {
  string project_id = 1;
 string name = 2;
  string description = 3;
  int32 priority = 4;
  google.protobuf.Value settings = 5;
}

message CreateQueueResponse {
  JobQueue queue = 1;
}

message GetQueueRequest {
  string id = 1;
}

message GetQueueResponse {
  JobQueue queue = 1;
}

message UpdateQueueRequest {
  string id = 1;
  string name = 2;
  string description = 3;
  int32 priority = 4;
  google.protobuf.Value settings = 5;
  bool is_active = 6;
}

message UpdateQueueResponse {
  JobQueue queue = 1;
}

message DeleteQueueRequest {
  string id = 1;
}

message DeleteQueueResponse {
  bool success = 1;
}

message ListQueuesRequest {
  string project_id = 1;
  uint32 page = 2;
  uint32 limit = 3;
}

message ListQueuesResponse {
  repeated JobQueue queues = 1;
  uint32 total = 2;
  uint32 page = 3;
  uint32 pages = 4;
}

message GetQueueStatsRequest {
  string id = 1;
}

message GetQueueStatsResponse {
  string queue_id = 1;
  int64 pending_count = 2;
  int64 processing_count = 3;
  int64 succeeded_count = 4;
  int64 failed_count = 5;
  int64 scheduled_count = 6;
  bool is_active = 7;
  google.protobuf.Timestamp last_activity = 8;
}

// Request dan Response untuk worker
message RegisterWorkerRequest {
  string project_id = 1;
  string name = 2;
  WorkerType worker_type = 3;
  google.protobuf.Value capabilities = 4;
  uint32 max_concurrent_jobs = 5;
}

message RegisterWorkerResponse {
  Worker worker = 1;
}

message HeartbeatRequest {
  string id = 1;
  map<string, string> metadata = 2;
}

message HeartbeatResponse {
  bool success = 1;
  google.protobuf.Timestamp next_heartbeat_at = 2;
}

message GetWorkerRequest {
  string id = 1;
}

message GetWorkerResponse {
  Worker worker = 1;
}

message ListWorkersRequest {
  string project_id = 1;
  WorkerType worker_type = 2;
  uint32 page = 3;
  uint32 limit = 4;
}

message ListWorkersResponse {
  repeated Worker workers = 1;
  uint32 total = 2;
  uint32 page = 3;
  uint32 pages = 4;
}
```

### 4.2. Implementasi gRPC Server

```rust
// File: src/grpc/job_queue_service.rs
use crate::proto::job_queue_server::{JobQueue, JobQueueServer};
use crate::proto::{
    CancelJobRequest, CancelJobResponse, CreateQueueRequest, CreateQueueResponse,
    DeleteQueueRequest, DeleteQueueResponse, GetJobRequest, GetJobResponse,
    GetQueueRequest, GetQueueResponse, GetQueueStatsRequest, GetQueueStatsResponse,
    GetWorkerRequest, GetWorkerResponse, HeartbeatRequest, HeartbeatResponse,
    Job, JobQueue as ProtoJobQueue, ListJobsRequest, ListJobsResponse,
    ListQueuesRequest, ListQueuesResponse, ListWorkersRequest, ListWorkersResponse,
    RegisterWorkerRequest, RegisterWorkerResponse, RetryJobRequest, RetryJobResponse,
    SubmitJobRequest, SubmitJobResponse, UpdateQueueRequest, UpdateQueueResponse,
    Worker as ProtoWorker,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

use crate::{
    models::{Job as ModelJob, JobQueue as ModelJobQueue, Worker as ModelWorker},
    services::{
        authentication_service::AuthenticationService,
        authorization_service::AuthorizationService,
        job_service::JobService,
        queue_service::QueueService,
        worker_service::WorkerService,
    },
};

pub struct GrpcJobQueueService {
    auth_service: Arc<AuthenticationService>,
    authz_service: Arc<AuthorizationService>,
    job_service: Arc<JobService>,
    queue_service: Arc<QueueService>,
    worker_service: Arc<WorkerService>,
}

impl GrpcJobQueueService {
    pub fn new(
        auth_service: Arc<AuthenticationService>,
        authz_service: Arc<AuthorizationService>,
        job_service: Arc<JobService>,
        queue_service: Arc<QueueService>,
        worker_service: Arc<WorkerService>,
    ) -> Self {
        Self {
            auth_service,
            authz_service,
            job_service,
            queue_service,
            worker_service,
        }
    }
    
    async fn authenticate_request(&self, request: &Request<()>) -> Result<crate::models::User, Status> {
        let auth_header = request
            .metadata()
            .get("authorization")
            .ok_or(Status::unauthenticated("Authorization header missing"))?
            .to_str()
            .map_err(|_| Status::unauthenticated("Invalid authorization header"))?;
        
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or(Status::unauthenticated("Invalid authorization format"))?
            .trim();
        
        let claims = self.auth_service.validate_token(token)
            .await
            .map_err(|_| Status::unauthenticated("Invalid token"))?;
        
        // Dapatkan user dari claims
        let user_id = uuid::Uuid::parse_str(&claims.sub)
            .map_err(|_| Status::internal("Invalid user ID in token"))?;
        
        let user = self.authz_service.get_user_by_id(user_id).await
            .map_err(|_| Status::internal("Failed to get user"))?
            .ok_or(Status::not_found("User not found"))?;
        
        Ok(user)
    }
}

#[tonic::async_trait]
impl JobQueue for GrpcJobQueueService {
    async fn submit_job(
        &self,
        request: Request<SubmitJobRequest>,
    ) -> Result<Response<SubmitJobResponse>, Status> {
        let user = self.authenticate_request(&request).await?;
        
        // Periksa izin
        if !self.authz_service.has_permission(&user, "job:write").await {
            return Err(Status::permission_denied("Insufficient permissions"));
        }
        
        let req = request.into_inner();
        
        // Konversi request ke model internal
        let queue_id = uuid::Uuid::parse_str(&req.queue_id)
            .map_err(|_| Status::invalid_argument("Invalid queue ID"))?;
        
        let job = crate::models::Job::new(
            queue_id,
            req.job_type,
            req.payload.unwrap_or_else(|| serde_json::Value::Null),
            req.priority.unwrap_or(0),
            req.max_attempts,
        );
        
        // Set scheduled time jika diberikan
        if let Some(scheduled_for) = req.scheduled_for {
            let chrono_time: chrono::DateTime<chrono::Utc> = scheduled_for.into();
            job.scheduled_for = Some(chrono_time);
        }
        
        let created_job = self.job_service.create_job(job).await
            .map_err(|_| Status::internal("Failed to create job"))?;
        
        // Konversi ke protobuf response
        let proto_job = job_model_to_proto(created_job);
        
        Ok(Response::new(SubmitJobResponse {
            job: Some(proto_job),
        }))
    }
    
    async fn get_job(
        &self,
        request: Request<GetJobRequest>,
    ) -> Result<Response<GetJobResponse>, Status> {
        let user = self.authenticate_request(&request).await?;
        
        // Periksa izin
        if !self.authz_service.has_permission(&user, "job:read").await {
            return Err(Status::permission_denied("Insufficient permissions"));
        }
        
        let req = request.into_inner();
        let job_id = uuid::Uuid::parse_str(&req.id)
            .map_err(|_| Status::invalid_argument("Invalid job ID"))?;
        
        let job = self.job_service.get_job_by_id(job_id).await
            .map_err(|_| Status::internal("Failed to get job"))?
            .ok_or(Status::not_found("Job not found"))?;
        
        // Pastikan job milik organisasi user
        let queue = self.queue_service.get_queue_by_id(job.queue_id).await
            .map_err(|_| Status::internal("Failed to get queue"))?
            .ok_or(Status::internal("Queue not found"))?;
        
        if queue.project_id != user.organization_id {
            return Err(Status::permission_denied("Job not accessible"));
        }
        
        let proto_job = job_model_to_proto(job);
        
        Ok(Response::new(GetJobResponse {
            job: Some(proto_job),
        }))
    }
    
    async fn cancel_job(
        &self,
        request: Request<CancelJobRequest>,
    ) -> Result<Response<CancelJobResponse>, Status> {
        let user = self.authenticate_request(&request).await?;
        
        // Periksa izin
        if !self.authz_service.has_permission(&user, "job:write").await {
            return Err(Status::permission_denied("Insufficient permissions"));
        }
        
        let req = request.into_inner();
        let job_id = uuid::Uuid::parse_str(&req.id)
            .map_err(|_| Status::invalid_argument("Invalid job ID"))?;
        
        let job = self.job_service.get_job_by_id(job_id).await
            .map_err(|_| Status::internal("Failed to get job"))?
            .ok_or(Status::not_found("Job not found"))?;
        
        // Pastikan job milik organisasi user
        let queue = self.queue_service.get_queue_by_id(job.queue_id).await
            .map_err(|_| Status::internal("Failed to get queue"))?
            .ok_or(Status::internal("Queue not found"))?;
        
        if queue.project_id != user.organization_id {
            return Err(Status::permission_denied("Job not accessible"));
        }
        
        // Hanya job yang belum selesai yang bisa dibatalkan
        if !matches!(job.status, crate::models::JobStatus::Pending | crate::models::JobStatus::Scheduled) {
            return Err(Status::failed_precondition("Job cannot be cancelled"));
        }
        
        self.job_service.cancel_job(job_id).await
            .map_err(|_| Status::internal("Failed to cancel job"))?;
        
        Ok(Response::new(CancelJobResponse {
            success: true,
        }))
    }
    
    async fn retry_job(
        &self,
        request: Request<RetryJobRequest>,
    ) -> Result<Response<RetryJobResponse>, Status> {
        let user = self.authenticate_request(&request).await?;
        
        // Periksa izin
        if !self.authz_service.has_permission(&user, "job:write").await {
            return Err(Status::permission_denied("Insufficient permissions"));
        }
        
        let req = request.into_inner();
        let job_id = uuid::Uuid::parse_str(&req.id)
            .map_err(|_| Status::invalid_argument("Invalid job ID"))?;
        
        let job = self.job_service.get_job_by_id(job_id).await
            .map_err(|_| Status::internal("Failed to get job"))?
            .ok_or(Status::not_found("Job not found"))?;
        
        // Pastikan job milik organisasi user
        let queue = self.queue_service.get_queue_by_id(job.queue_id).await
            .map_err(|_| Status::internal("Failed to get queue"))?
            .ok_or(Status::internal("Queue not found"))?;
        
        if queue.project_id != user.organization_id {
            return Err(Status::permission_denied("Job not accessible"));
        }
        
        // Hanya job yang gagal dan bisa diretry
        if !matches!(job.status, crate::models::JobStatus::Failed) || !job.can_be_retried() {
            return Err(Status::failed_precondition("Job cannot be retried"));
        }
        
        self.job_service.retry_job(job_id).await
            .map_err(|_| Status::internal("Failed to retry job"))?;
        
        Ok(Response::new(RetryJobResponse {
            success: true,
        }))
    }
    
    async fn list_jobs(
        &self,
        request: Request<ListJobsRequest>,
    ) -> Result<Response<ListJobsResponse>, Status> {
        let user = self.authenticate_request(&request).await?;
        
        // Periksa izin
        if !self.authz_service.has_permission(&user, "job:read").await {
            return Err(Status::permission_denied("Insufficient permissions"));
        }
        
        let req = request.into_inner();
        let queue_id = req.queue_id.and_then(|id| uuid::Uuid::parse_str(&id).ok());
        let status = req.status.and_then(|s| s.try_into().ok());
        let page = req.page.unwrap_or(1) as i32;
        let limit = req.limit.unwrap_or(10).min(100) as i32;
        
        let jobs = self.job_service.get_jobs_by_criteria(
            user.organization_id,
            queue_id,
            status,
            Some(limit),
            Some((page - 1) * limit),
        ).await
            .map_err(|_| Status::internal("Failed to get jobs"))?;
        
        let proto_jobs: Vec<Job> = jobs.into_iter()
            .map(job_model_to_proto)
            .collect();
        
        Ok(Response::new(ListJobsResponse {
            jobs: proto_jobs,
            total: proto_jobs.len() as u32,
            page: page as u32,
            pages: 1, // Dalam implementasi nyata, ini akan dihitung dari total records
        }))
    }
    
    // Implementasi method lainnya untuk queue dan worker akan ditambahkan di sini
    // ...
}

// Fungsi konversi dari model internal ke protobuf
fn job_model_to_proto(job: ModelJob) -> Job {
    Job {
        id: job.id.to_string(),
        queue_id: job.queue_id.to_string(),
        job_type: job.job_type,
        payload: Some(protobuf_value_from_json(job.payload)),
        status: job_status_to_proto(job.status),
        priority: job.priority,
        max_attempts: job.max_attempts,
        attempt_count: job.attempt_count,
        scheduled_for: job.scheduled_for.map(|t| t.into()),
        timeout_at: job.timeout_at.map(|t| t.into()),
        created_at: Some(job.created_at.into()),
        updated_at: Some(job.updated_at.into()),
        completed_at: job.completed_at.map(|t| t.into()),
    }
}

fn protobuf_value_from_json(json_value: serde_json::Value) -> tonic::codec::Streaming<tonic::Bytes> {
    // Dalam implementasi nyata, ini akan mengonversi serde_json::Value ke protobuf::Value
    // Untuk sekarang, kita kembalikan null
    tonic::codec::Streaming::empty()
}

fn job_status_to_proto(status: crate::models::JobStatus) -> i32 {
    match status {
        crate::models::JobStatus::Pending => 0,
        crate::models::JobStatus::Scheduled => 1,
        crate::models::JobStatus::Processing => 2,
        crate::models::JobStatus::Succeeded => 3,
        crate::models::JobStatus::Failed => 4,
        crate::models::JobStatus::Cancelled => 5,
    }
}
```

### 4.3. Server gRPC

```rust
// File: src/grpc/server.rs
use tonic::transport::Server;
use std::sync::Arc;

use crate::{
    grpc::job_queue_service::GrpcJobQueueService,
    services::{
        authentication_service::AuthenticationService,
        authorization_service::AuthorizationService,
        job_service::JobService,
        queue_service::QueueService,
        worker_service::WorkerService,
    },
};

pub struct GrpcServer {
    addr: std::net::SocketAddr,
    service: GrpcJobQueueService,
}

impl GrpcServer {
    pub fn new(
        addr: std::net::SocketAddr,
        auth_service: Arc<AuthenticationService>,
        authz_service: Arc<AuthorizationService>,
        job_service: Arc<JobService>,
        queue_service: Arc<QueueService>,
        worker_service: Arc<WorkerService>,
    ) -> Self {
        let service = GrpcJobQueueService::new(
            auth_service,
            authz_service,
            job_service,
            queue_service,
            worker_service,
        );
        
        Self { addr, service }
    }
    
    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>> {
        Server::builder()
            .add_service(crate::proto::job_queue_server::JobQueueServer::new(self.service))
            .serve(self.addr)
            .await?;
        
        Ok(())
    }
}
```

## 5. Validasi dan Error Handling

### 5.1. Sistem Validasi API

```rust
// File: src/api/validation.rs
use serde::Deserialize;
use validator::{Validate, ValidationError};

#[derive(Debug, Deserialize, Validate)]
pub struct CreateJobRequest {
    #[validate(length(min = 1, max = 100))]
    pub queue_name: String,
    
    #[validate(length(min = 1, max = 100))]
    pub job_type: String,
    
    #[validate(range(min = 0, max = 9))]
    pub priority: Option<i32>,
    
    #[validate(range(min = 1, max = 10))]
    pub max_attempts: Option<i32>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateQueueRequest {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    
    #[validate(length(max = 500))]
    pub description: Option<String>,
    
    #[validate(range(min = 0, max = 9))]
    pub priority: Option<i32>,
}

pub fn validate_request<T: Validate>(request: &T) -> Result<(), Vec<validator::ValidationErrors>> {
    match request.validate() {
        Ok(()) => Ok(()),
        Err(e) => Err(vec![e]),
    }
}
```

### 5.2. Error Handling dan Response Format

```rust
// File: src/api/error.rs
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    
    #[error("Authentication error: {0}")]
    AuthenticationError(String),
    
    #[error("Authorization error: {0}")]
    AuthorizationError(String),
    
    #[error("Resource not found")]
    NotFound,
    
    #[error("Resource conflict")]
    Conflict,
    
    #[error("Internal server error")]
    InternalError,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            ApiError::ValidationError(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::AuthenticationError(msg) => (StatusCode::UNAUTHORIZED, msg),
            ApiError::AuthorizationError(msg) => (StatusCode::FORBIDDEN, msg),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "Resource not found".to_string()),
            ApiError::Conflict => (StatusCode::CONFLICT, "Resource conflict".to_string()),
            ApiError::DatabaseError(_) | ApiError::InternalError => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
        };

        let body = Json(json!({
            "error": error_message,
            "status_code": status.as_u16(),
        }));

        (status, body).into_response()
    }
}

// Custom Result type untuk API
pub type ApiResult<T> = Result<T, ApiError>;
```

## 6. Rate Limiting dan Security

### 6.1. Rate Limiting Middleware

```rust
// File: src/middleware/rate_limit.rs
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub struct RateLimiter {
    limits: Arc<RwLock<HashMap<String, Vec<RequestRecord>>>>,
    default_requests_per_minute: u32,
}

struct RequestRecord {
    timestamp: Instant,
}

impl RateLimiter {
    pub fn new(default_requests_per_minute: u32) -> Self {
        Self {
            limits: Arc::new(RwLock::new(HashMap::new())),
            default_requests_per_minute,
        }
    }
    
    pub async fn is_allowed(&self, key: &str, max_requests: Option<u32>) -> bool {
        let now = Instant::now();
        let max_requests = max_requests.unwrap_or(self.default_requests_per_minute);
        
        let mut limits = self.limits.write().await;
        let records = limits.entry(key.to_string()).or_insert_with(Vec::new);
        
        // Hapus request lama (lebih dari 1 menit)
        records.retain(|record| {
            now.duration_since(record.timestamp) < Duration::from_secs(60)
        });
        
        // Periksa apakah jumlah request melebihi batas
        if records.len() >= max_requests as usize {
            return false;
        }
        
        // Tambahkan request baru
        records.push(RequestRecord {
            timestamp: now,
        });
        
        true
    }
    
    pub async fn get_remaining_requests(&self, key: &str, max_requests: Option<u32>) -> u32 {
        let max_requests = max_requests.unwrap_or(self.default_requests_per_minute);
        let limits = self.limits.read().await;
        
        if let Some(records) = limits.get(key) {
            max_requests.saturating_sub(records.len() as u32)
        } else {
            max_requests
        }
    }
}

pub async fn rate_limit_middleware(
    State(rate_limiter): State<Arc<RateLimiter>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Ambil IP client
    let client_ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|header| header.to_str().ok())
        .unwrap_or("unknown")
        .split(',')
        .next()
        .unwrap_or("unknown")
        .trim()
        .to_string();
    
    // Periksa apakah request diizinkan
    if !rate_limiter.is_allowed(&client_ip, None).await {
        let remaining = rate_limiter.get_remaining_requests(&client_ip, None).await;
        
        return Ok(Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("X-RateLimit-Remaining", remaining.to_string())
            .header("Retry-After", "60") // 1 menit
            .body(axum::body::Body::from("Rate limit exceeded"))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?);
    }
    
    Ok(next.run(request).await)
}
```

## 7. Dokumentasi API (OpenAPI/Swagger)

### 7.1. Konfigurasi OpenAPI

```yaml
# openapi.yml
openapi: 3.0
info:
  title: TaskForge API
  description: High-performance background job queue API
  version: 1.0.0
  contact:
    name: TaskForge Support
    email: support@taskforge.com

servers:
  - url: https://api.taskforge.com/v1
    description: Production server
  - url: https://staging-api.taskforge.com/v1
    description: Staging server

paths:
  /auth/login:
    post:
      summary: User login
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              properties:
                email:
                  type: string
                  format: email
                password:
                  type: string
                  format: password
      responses:
        '200':
          description: Successful login
          content:
            application/json:
              schema:
                type: object
                properties:
                  access_token:
                    type: string
                  refresh_token:
                    type: string
                  token_type:
                    type: string
                    default: Bearer
        '401':
          $ref: '#/components/responses/UnauthorizedError'
  
  /queues:
    get:
      summary: Get all queues for organization
      parameters:
        - name: page
          in: query
          schema:
            type: integer
            default: 1
        - name: limit
          in: query
          schema:
            type: integer
            default: 10
            maximum: 100
      responses:
        '200':
          description: List of queues
          content:
            application/json:
              schema:
                type: object
                properties:
                  queues:
                    type: array
                    items:
                      $ref: '#/components/schemas/JobQueue'
                  pagination:
                    $ref: '#/components/schemas/Pagination'
        '401':
          $ref: '#/components/responses/UnauthorizedError'
        '403':
          $ref: '#/components/responses/ForbiddenError'
    
    post:
      summary: Create a new queue
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/CreateQueueRequest'
      responses:
        '201':
          description: Queue created successfully
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/JobQueue'
        '400':
          $ref: '#/components/responses/ValidationError'
        '401':
          $ref: '#/components/responses/UnauthorizedError'
        '403':
          $ref: '#/components/responses/ForbiddenError'

components:
  schemas:
    JobQueue:
      type: object
      properties:
        id:
          type: string
          format: uuid
        project_id:
          type: string
          format: uuid
        name:
          type: string
          maxLength: 100
        description:
          type: string
          maxLength: 500
        priority:
          type: integer
          minimum: 0
          maximum: 9
        settings:
          type: object
        is_active:
          type: boolean
        created_at:
          type: string
          format: date-time
        updated_at:
          type: string
          format: date-time
    
    Job:
      type: object
      properties:
        id:
          type: string
          format: uuid
        queue_id:
          type: string
          format: uuid
        job_type:
          type: string
          maxLength: 100
        payload:
          type: object
        status:
          type: string
          enum: [pending, scheduled, processing, succeeded, failed, cancelled]
        priority:
          type: integer
          minimum: 0
          maximum: 9
        max_attempts:
          type: integer
          minimum: 1
          maximum: 10
        attempt_count:
          type: integer
        scheduled_for:
          type: string
          format: date-time
        created_at:
          type: string
          format: date-time
        completed_at:
          type: string
          format: date-time
    
    Pagination:
      type: object
      properties:
        page:
          type: integer
        limit:
          type: integer
        total:
          type: integer
        pages:
          type: integer
  
    CreateQueueRequest:
      type: object
      required: [name]
      properties:
        name:
          type: string
          maxLength: 100
        description:
          type: string
          maxLength: 500
        priority:
          type: integer
          minimum: 0
          maximum: 9
          default: 0
        settings:
          type: object
  
  responses:
    UnauthorizedError:
      description: Unauthorized
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ErrorResponse'
    
    ForbiddenError:
      description: Forbidden
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ErrorResponse'
    
    ValidationError:
      description: Validation Error
      content:
        application/json:
          schema:
            $ref: '#/components/schemas/ErrorResponse'
    
    ErrorResponse:
      type: object
      properties:
        error:
          type: string
        status_code:
          type: integer
  
  securitySchemes:
    BearerAuth:
      type: http
      scheme: bearer
      bearerFormat: JWT

security:
  - BearerAuth: []
```

## 8. Best Practices dan Rekomendasi

### 8.1. Praktik Terbaik untuk REST API

1. **Gunakan HTTP status code yang tepat** - untuk mengindikasikan hasil operasi
2. **Gunakan konvensi penamaan yang konsisten** - untuk endpoint dan field
3. **Sediakan dokumentasi yang lengkap** - dengan contoh request dan response
4. **Terapkan rate limiting** - untuk mencegah abuse dan menjaga kinerja
5. **Gunakan pagination untuk endpoint koleksi** - untuk efisiensi dan kinerja

### 8.2. Praktik Terbaik untuk gRPC

1. **Gunakan protobuf untuk definisi schema** - untuk kompatibilitas lintas bahasa
2. **Gunakan streaming untuk data besar** - untuk efisiensi transfer
3. **Gunakan gRPC status code yang tepat** - untuk mengindikasikan hasil operasi
4. **Gunakan metadata untuk informasi tambahan** - seperti auth dan tracing
5. **Gunakan gRPC gateway untuk REST proxy** - untuk kemudahan integrasi

### 8.3. Skala dan Kinerja

1. **Gunakan connection pooling** - untuk efisiensi koneksi ke database
2. **Gunakan caching strategi** - untuk data yang sering diakses
3. **Gunakan async processing** - untuk operasi yang memakan waktu
4. **Gunakan load balancing** - untuk distribusi beban
5. **Gunakan monitoring dan observability** - untuk pemantauan kinerja