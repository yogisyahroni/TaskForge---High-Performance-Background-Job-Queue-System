# Sistem Queue dan Job Management untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan sistem queue dan job management dalam aplikasi TaskForge. Sistem ini merupakan inti dari platform, bertanggung jawab untuk menerima, mengelola, dan mengeksekusi job secara efisien dan andal.

## 2. Arsitektur Sistem Queue dan Job

### 2.1. Model Queue

Queue dalam TaskForge adalah entitas yang mengelola antrian job dengan konfigurasi tertentu:

```rust
// File: src/models/job_queue.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobQueue {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub priority: i32, // 0-9, semakin tinggi semakin prioritas
    pub settings: Value, // JSONB field untuk konfigurasi queue
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueSettings {
    pub max_concurrent_jobs: Option<u32>,      // Jumlah maksimum job yang bisa berjalan bersamaan
    pub max_retries: Option<u32>,              // Jumlah maksimum retry untuk setiap job
    pub timeout_seconds: Option<u32>,          // Timeout dalam detik untuk setiap job
    pub retry_delay_seconds: Option<u32>,      // Delay awal untuk retry (dalam detik)
    pub max_retry_delay_seconds: Option<u32>,  // Delay maksimum untuk retry (dalam detik)
    pub backoff_multiplier: Option<f64>,       // Multiplier untuk exponential backoff
    pub dead_letter_queue_id: Option<Uuid>,    // ID queue untuk job yang gagal setelah semua retry
    pub rate_limit: Option<u32>,               // Batas rate per menit
}

impl JobQueue {
    pub fn new(
        project_id: Uuid,
        name: String,
        description: Option<String>,
        priority: i32,
        settings: Value,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            name,
            description,
            priority,
            settings,
            is_active: true,
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("Queue name must be between 1 and 100 characters".to_string());
        }
        
        if self.priority < 0 || self.priority > 9 {
            return Err("Queue priority must be between 0 and 9".to_string());
        }
        
        Ok(())
    }
    
    pub fn get_settings(&self) -> Result<QueueSettings, serde_json::Error> {
        serde_json::from_value(self.settings.clone())
    }
    
    pub fn can_accept_job(&self) -> bool {
        self.is_active
    }
}
```

### 2.2. Model Job

Job adalah unit kerja individual yang dikirimkan ke queue:

```rust
// File: src/models/job.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "job_status", rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Scheduled,
    Processing,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Job {
    pub id: Uuid,
    pub queue_id: Uuid,
    pub job_type: String,     // Nama tipe job untuk routing ke worker yang tepat
    pub payload: Value,       // JSONB field untuk data job
    pub status: JobStatus,
    pub priority: i32,        // 0-9, semakin tinggi semakin prioritas
    pub max_attempts: i32,    // Jumlah maksimum percobaan
    pub attempt_count: i32,   // Jumlah percobaan yang telah dilakukan
    pub scheduled_for: Option<DateTime<Utc>>, // Waktu eksekusi dijadwalkan
    pub timeout_at: Option<DateTime<Utc>>,    // Waktu maksimum untuk menyelesaikan job
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Job {
    pub fn new(
        queue_id: Uuid,
        job_type: String,
        payload: Value,
        priority: i32,
        max_attempts: Option<i32>,
    ) -> Self {
        let now = Utc::now();
        let max_attempts = max_attempts.unwrap_or(3); // Default 3 kali percobaan
        
        Self {
            id: Uuid::new_v4(),
            queue_id,
            job_type,
            payload,
            status: JobStatus::Pending,
            priority,
            max_attempts,
            attempt_count: 0,
            scheduled_for: None,
            timeout_at: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    
    pub fn new_scheduled(
        queue_id: Uuid,
        job_type: String,
        payload: Value,
        priority: i32,
        scheduled_for: DateTime<Utc>,
        max_attempts: Option<i32>,
    ) -> Self {
        let mut job = Self::new(queue_id, job_type, payload, priority, max_attempts);
        job.status = JobStatus::Scheduled;
        job.scheduled_for = Some(scheduled_for);
        job
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.job_type.is_empty() || self.job_type.len() > 100 {
            return Err("Job type must be between 1 and 100 characters".to_string());
        }
        
        if self.priority < 0 || self.priority > 9 {
            return Err("Job priority must be between 0 and 9".to_string());
        }
        
        if self.max_attempts < 1 {
            return Err("Max attempts must be at least 1".to_string());
        }
        
        Ok(())
    }
    
    pub fn is_ready_for_execution(&self) -> bool {
        match self.status {
            JobStatus::Pending => true,
            JobStatus::Scheduled => {
                if let Some(scheduled_time) = self.scheduled_for {
                    Utc::now() >= scheduled_time
                } else {
                    false
                }
            },
            _ => false,
        }
    }
    
    pub fn can_be_retried(&self) -> bool {
        self.attempt_count < self.max_attempts && matches!(self.status, JobStatus::Failed)
    }
    
    pub fn mark_as_processing(&mut self) {
        self.status = JobStatus::Processing;
        self.attempt_count += 1;
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_succeeded(&mut self) {
        self.status = JobStatus::Succeeded;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_failed(&mut self) {
        self.status = JobStatus::Failed;
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_cancelled(&mut self) {
        self.status = JobStatus::Cancelled;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}
```

## 3. Sistem Queue Management

### 3.1. Layanan Queue

```rust
// File: src/services/queue_service.rs
use crate::{
    database::Database,
    models::{JobQueue, Job, JobStatus},
    utils::validation,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use serde_json::Value;

pub struct QueueService {
    db: Database,
}

impl QueueService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn create_queue(
        &self,
        project_id: Uuid,
        name: String,
        description: Option<String>,
        priority: i32,
        settings: Value,
    ) -> Result<JobQueue, SqlxError> {
        let queue = JobQueue::new(project_id, name, description, priority, settings);
        queue.validate().map_err(|e| SqlxError::RowNotFound)?;
        
        self.db.create_job_queue(queue).await
    }
    
    pub async fn get_queue_by_id(
        &self,
        queue_id: Uuid,
    ) -> Result<Option<JobQueue>, SqlxError> {
        self.db.get_job_queue_by_id(queue_id).await
    }
    
    pub async fn get_queue_by_name_and_project(
        &self,
        name: &str,
        project_id: Uuid,
    ) -> Result<Option<JobQueue>, SqlxError> {
        self.db.get_job_queue_by_name_and_project(name, project_id).await
    }
    
    pub async fn get_queues_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<JobQueue>, SqlxError> {
        self.db.get_job_queues_by_project(project_id).await
    }
    
    pub async fn update_queue(
        &self,
        queue_id: Uuid,
        name: Option<String>,
        description: Option<String>,
        priority: Option<i32>,
        settings: Option<Value>,
        is_active: Option<bool>,
    ) -> Result<(), SqlxError> {
        self.db.update_job_queue(queue_id, name, description, priority, settings, is_active).await
    }
    
    pub async fn delete_queue(
        &self,
        queue_id: Uuid,
    ) -> Result<(), SqlxError> {
        // Periksa apakah queue memiliki job yang sedang berjalan
        let active_jobs = self.db.get_active_jobs_count(queue_id).await?;
        if active_jobs > 0 {
            return Err(SqlxError::RowNotFound); // Tidak bisa menghapus queue dengan job aktif
        }
        
        self.db.delete_job_queue(queue_id).await
    }
    
    pub async fn pause_queue(
        &self,
        queue_id: Uuid,
    ) -> Result<(), SqlxError> {
        self.db.update_queue_active_status(queue_id, false).await
    }
    
    pub async fn resume_queue(
        &self,
        queue_id: Uuid,
    ) -> Result<(), SqlxError> {
        self.db.update_queue_active_status(queue_id, true).await
    }
    
    pub async fn get_queue_stats(
        &self,
        queue_id: Uuid,
    ) -> Result<QueueStats, SqlxError> {
        let pending_count = self.db.get_job_count_by_status(queue_id, JobStatus::Pending).await?;
        let processing_count = self.db.get_job_count_by_status(queue_id, JobStatus::Processing).await?;
        let succeeded_count = self.db.get_job_count_by_status(queue_id, JobStatus::Succeeded).await?;
        let failed_count = self.db.get_job_count_by_status(queue_id, JobStatus::Failed).await?;
        let scheduled_count = self.db.get_job_count_by_status(queue_id, JobStatus::Scheduled).await?;
        
        let queue = self.get_queue_by_id(queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        Ok(QueueStats {
            queue_id,
            pending_count,
            processing_count,
            succeeded_count,
            failed_count,
            scheduled_count,
            is_active: queue.is_active,
        })
    }
}

pub struct QueueStats {
    pub queue_id: Uuid,
    pub pending_count: i64,
    pub processing_count: i64,
    pub succeeded_count: i64,
    pub failed_count: i64,
    pub scheduled_count: i64,
    pub is_active: bool,
}
```

### 3.2. Endpoint API Queue

```rust
// File: src/handlers/queue_handler.rs
use axum::{
    extract::{State, Path, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use crate::{
    services::queue_service::{QueueService, QueueStats},
    auth::middleware::{AuthenticatedUser, check_permission},
    models::{JobQueue, JobStatus},
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

pub async fn create_queue(
    State(queue_service): State<QueueService>,
    authenticated_user: AuthenticatedUser,
    Json(payload): Json<CreateQueueRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "queue:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Dapatkan project ID dari request (dalam implementasi nyata ini akan dari path atau body)
    let project_id = authenticated_user.organization_id; // Ini hanya contoh
    
    let queue = queue_service
        .create_queue(
            project_id,
            payload.name,
            payload.description,
            payload.priority.unwrap_or(0),
            payload.settings,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(queue))
}

pub async fn get_queue(
    State(queue_service): State<QueueService>,
    authenticated_user: AuthenticatedUser,
    Path(queue_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "queue:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let queue = queue_service
        .get_queue_by_id(queue_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan queue milik proyek dalam organisasi yang sama
    let project = /* implementasikan untuk mendapatkan proyek queue */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    Ok(Json(queue))
}

pub async fn get_queues(
    State(queue_service): State<QueueService>,
    authenticated_user: AuthenticatedUser,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "queue:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Dapatkan semua proyek dalam organisasi
    let projects = /* implementasikan untuk mendapatkan proyek dalam organisasi */;
    
    let mut all_queues = Vec::new();
    for project in projects {
        let project_queues = queue_service
            .get_queues_by_project(project.id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        
        all_queues.extend(project_queues);
    }
    
    Ok(Json(all_queues))
}

pub async fn update_queue(
    State(queue_service): State<QueueService>,
    authenticated_user: AuthenticatedUser,
    Path(queue_id): Path<uuid::Uuid>,
    Json(payload): Json<UpdateQueueRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "queue:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let queue = queue_service
        .get_queue_by_id(queue_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan queue milik proyek dalam organisasi yang sama
    let project = /* implementasikan untuk mendapatkan proyek queue */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    queue_service
        .update_queue(
            queue_id,
            payload.name,
            payload.description,
            payload.priority,
            payload.settings,
            payload.is_active,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_queue(
    State(queue_service): State<QueueService>,
    authenticated_user: AuthenticatedUser,
    Path(queue_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "queue:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let queue = queue_service
        .get_queue_by_id(queue_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan queue milik proyek dalam organisasi yang sama
    let project = /* implementasikan untuk mendapatkan proyek queue */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    queue_service
        .delete_queue(queue_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_queue_stats(
    State(queue_service): State<QueueService>,
    authenticated_user: AuthenticatedUser,
    Path(queue_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "queue:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let queue = queue_service
        .get_queue_by_id(queue_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan queue milik proyek dalam organisasi yang sama
    let project = /* implementasikan untuk mendapatkan proyek queue */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let stats = queue_service
        .get_queue_stats(queue_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(stats))
}

pub async fn pause_queue(
    State(queue_service): State<QueueService>,
    authenticated_user: AuthenticatedUser,
    Path(queue_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "queue:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let queue = queue_service
        .get_queue_by_id(queue_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan queue milik proyek dalam organisasi yang sama
    let project = /* implementasikan untuk mendapatkan proyek queue */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    queue_service
        .pause_queue(queue_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn resume_queue(
    State(queue_service): State<QueueService>,
    authenticated_user: AuthenticatedUser,
    Path(queue_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "queue:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let queue = queue_service
        .get_queue_by_id(queue_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan queue milik proyek dalam organisasi yang sama
    let project = /* implementasikan untuk mendapatkan proyek queue */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    queue_service
        .resume_queue(queue_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}
```

## 4. Sistem Job Management

### 4.1. Layanan Job

```rust
// File: src/services/job_service.rs
use crate::{
    database::Database,
    models::{Job, JobStatus, JobQueue},
    utils::validation,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use serde_json::Value;
use chrono::{DateTime, Utc};

pub struct JobService {
    db: Database,
}

impl JobService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn create_job(
        &self,
        queue_id: Uuid,
        job_type: String,
        payload: Value,
        priority: i32,
        max_attempts: Option<i32>,
    ) -> Result<Job, SqlxError> {
        // Dapatkan informasi queue
        let queue = self.db.get_job_queue_by_id(queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if !queue.can_accept_job() {
            return Err(SqlxError::RowNotFound); // Queue tidak aktif
        }
        
        let mut job = Job::new(queue_id, job_type, payload, priority, max_attempts);
        job.validate().map_err(|e| SqlxError::RowNotFound)?;
        
        self.db.create_job(job).await
    }
    
    pub async fn create_scheduled_job(
        &self,
        queue_id: Uuid,
        job_type: String,
        payload: Value,
        priority: i32,
        scheduled_for: DateTime<Utc>,
        max_attempts: Option<i32>,
    ) -> Result<Job, SqlxError> {
        // Dapatkan informasi queue
        let queue = self.db.get_job_queue_by_id(queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if !queue.can_accept_job() {
            return Err(SqlxError::RowNotFound); // Queue tidak aktif
        }
        
        let mut job = Job::new_scheduled(queue_id, job_type, payload, priority, scheduled_for, max_attempts);
        job.validate().map_err(|e| SqlxError::RowNotFound)?;
        
        self.db.create_job(job).await
    }
    
    pub async fn get_job_by_id(
        &self,
        job_id: Uuid,
    ) -> Result<Option<Job>, SqlxError> {
        self.db.get_job_by_id(job_id).await
    }
    
    pub async fn get_jobs_by_queue(
        &self,
        queue_id: Uuid,
        status: Option<JobStatus>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<Job>, SqlxError> {
        self.db.get_jobs_by_queue(queue_id, status, limit, offset).await
    }
    
    pub async fn get_next_job_for_processing(
        &self,
        queue_id: Uuid,
    ) -> Result<Option<Job>, SqlxError> {
        self.db.get_next_ready_job(queue_id).await
    }
    
    pub async fn update_job_status(
        &self,
        job_id: Uuid,
        status: JobStatus,
    ) -> Result<(), SqlxError> {
        self.db.update_job_status(job_id, status).await
    }
    
    pub async fn cancel_job(
        &self,
        job_id: Uuid,
    ) -> Result<(), SqlxError> {
        let mut job = self.db.get_job_by_id(job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Hanya job yang belum selesai yang bisa dibatalkan
        match job.status {
            JobStatus::Pending | JobStatus::Scheduled => {
                job.mark_as_cancelled();
                self.db.update_job(job).await?;
                Ok(())
            },
            _ => Err(SqlxError::RowNotFound), // Tidak bisa membatalkan job yang sedang berjalan atau sudah selesai
        }
    }
    
    pub async fn retry_job(
        &self,
        job_id: Uuid,
    ) -> Result<(), SqlxError> {
        let mut job = self.db.get_job_by_id(job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if job.can_be_retried() {
            // Reset status ke pending untuk retry
            job.status = JobStatus::Pending;
            job.updated_at = Utc::now();
            self.db.update_job(job).await?;
            Ok(())
        } else {
            Err(SqlxError::RowNotFound) // Tidak bisa retry
        }
    }
    
    pub async fn get_job_stats_by_queue(
        &self,
        queue_id: Uuid,
    ) -> Result<JobStats, SqlxError> {
        let total_count = self.db.get_job_count_by_queue(queue_id).await?;
        let pending_count = self.db.get_job_count_by_status(queue_id, JobStatus::Pending).await?;
        let processing_count = self.db.get_job_count_by_status(queue_id, JobStatus::Processing).await?;
        let succeeded_count = self.db.get_job_count_by_status(queue_id, JobStatus::Succeeded).await?;
        let failed_count = self.db.get_job_count_by_status(queue_id, JobStatus::Failed).await?;
        let scheduled_count = self.db.get_job_count_by_status(queue_id, JobStatus::Scheduled).await?;
        let cancelled_count = self.db.get_job_count_by_status(queue_id, JobStatus::Cancelled).await?;
        
        Ok(JobStats {
            queue_id,
            total_count,
            pending_count,
            processing_count,
            succeeded_count,
            failed_count,
            scheduled_count,
            cancelled_count,
        })
    }
    
    pub async fn get_jobs_by_status(
        &self,
        queue_id: Uuid,
        status: JobStatus,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<Job>, SqlxError> {
        self.db.get_jobs_by_status(queue_id, status, limit, offset).await
    }
    
    pub async fn get_jobs_by_type(
        &self,
        queue_id: Uuid,
        job_type: &str,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<Job>, SqlxError> {
        self.db.get_jobs_by_type(queue_id, job_type, limit, offset).await
    }
}

pub struct JobStats {
    pub queue_id: Uuid,
    pub total_count: i64,
    pub pending_count: i64,
    pub processing_count: i64,
    pub succeeded_count: i64,
    pub failed_count: i64,
    pub scheduled_count: i64,
    pub cancelled_count: i64,
}
```

### 4.2. Endpoint API Job

```rust
// File: src/handlers/job_handler.rs
use axum::{
    extract::{State, Path, Query, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use crate::{
    services::job_service::{JobService, JobStats},
    auth::middleware::{AuthenticatedUser, check_permission},
    models::{Job, JobStatus},
};

#[derive(Deserialize)]
pub struct CreateJobRequest {
    pub queue_name: String,  // Nama queue tempat job akan dikirim
    pub job_type: String,
    pub payload: serde_json::Value,
    pub priority: Option<i32>,
    pub max_attempts: Option<i32>,
    pub scheduled_for: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Deserialize)]
pub struct GetJobsQuery {
    pub status: Option<String>,
    pub job_type: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

pub async fn create_job(
    State(job_service): State<JobService>,
    authenticated_user: AuthenticatedUser,
    Json(payload): Json<CreateJobRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "job:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Dapatkan queue berdasarkan nama dan organisasi
    let queue = /* implementasikan untuk mendapatkan queue berdasarkan nama dan organisasi */;
    
    let job = if let Some(scheduled_for) = payload.scheduled_for {
        job_service
            .create_scheduled_job(
                queue.id,
                payload.job_type,
                payload.payload,
                payload.priority.unwrap_or(0),
                scheduled_for,
                payload.max_attempts,
            )
            .await
    } else {
        job_service
            .create_job(
                queue.id,
                payload.job_type,
                payload.payload,
                payload.priority.unwrap_or(0),
                payload.max_attempts,
            )
            .await
    }
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(job))
}

pub async fn get_job(
    State(job_service): State<JobService>,
    authenticated_user: AuthenticatedUser,
    Path(job_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "job:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let job = job_service
        .get_job_by_id(job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan job milik queue dalam organisasi yang sama
    let queue = /* implementasikan untuk mendapatkan queue job */;
    let project = /* implementasikan untuk mendapatkan proyek queue */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    Ok(Json(job))
}

pub async fn get_jobs(
    State(job_service): State<JobService>,
    authenticated_user: AuthenticatedUser,
    query: Query<GetJobsQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "job:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Dapatkan semua queue dalam organisasi
    let queues = /* implementasikan untuk mendapatkan semua queue dalam organisasi */;
    
    let mut all_jobs = Vec::new();
    for queue in queues {
        let jobs = job_service
            .get_jobs_by_queue(
                queue.id,
                query.status.as_deref().and_then(|s| s.parse::<JobStatus>().ok()),
                query.limit,
                query.offset,
            )
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        
        all_jobs.extend(jobs);
    }
    
    Ok(Json(all_jobs))
}

pub async fn cancel_job(
    State(job_service): State<JobService>,
    authenticated_user: AuthenticatedUser,
    Path(job_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "job:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let job = job_service
        .get_job_by_id(job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan job milik queue dalam organisasi yang sama
    let queue = /* implementasikan untuk mendapatkan queue job */;
    let project = /* implementasikan untuk mendapatkan proyek queue */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    job_service
        .cancel_job(job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn retry_job(
    State(job_service): State<JobService>,
    authenticated_user: AuthenticatedUser,
    Path(job_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "job:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let job = job_service
        .get_job_by_id(job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan job milik queue dalam organisasi yang sama
    let queue = /* implementasikan untuk mendapatkan queue job */;
    let project = /* implementasikan untuk mendapatkan proyek queue */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    job_service
        .retry_job(job_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_job_stats(
    State(job_service): State<JobService>,
    authenticated_user: AuthenticatedUser,
    Path(queue_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "job:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let queue = /* implementasikan untuk mendapatkan queue */;
    if queue.project_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let stats = job_service
        .get_job_stats_by_queue(queue_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(stats))
}
```

## 5. Sistem Prioritas dan Penjadwalan

### 5.1. Algoritma Prioritas

TaskForge menggunakan sistem prioritas multi-level untuk menentukan urutan eksekusi job:

```rust
// File: src/services/priority_service.rs
use crate::models::{Job, JobQueue};
use chrono::{DateTime, Utc};

pub struct PriorityService;

impl PriorityService {
    pub fn get_execution_order(jobs: &mut Vec<Job>) {
        // Urutkan berdasarkan prioritas queue, kemudian prioritas job, kemudian waktu pembuatan
        jobs.sort_by(|a, b| {
            // Dapatkan informasi queue untuk masing-masing job
            // Dalam implementasi nyata, ini akan melibatkan lookup database
            let queue_a_priority = 0; // Nilai default
            let queue_b_priority = 0; // Nilai default
            
            // Bandingkan prioritas queue terlebih dahulu
            queue_b_priority.cmp(&queue_a_priority)
                .then_with(|| {
                    // Jika prioritas queue sama, bandingkan prioritas job
                    b.priority.cmp(&a.priority)
                })
                .then_with(|| {
                    // Jika prioritas job juga sama, urutkan berdasarkan waktu pembuatan (FIFO)
                    a.created_at.cmp(&b.created_at)
                })
        });
    }
    
    pub fn should_execute_immediately(job: &Job) -> bool {
        // Job dengan prioritas tinggi mungkin harus dieksekusi segera
        job.priority >= 8
    }
    
    pub fn calculate_execution_delay(job: &Job, queue: &JobQueue) -> Option<DateTime<Utc>> {
        // Dalam implementasi nyata, ini akan mempertimbangkan beban sistem,
        // kebijakan rate limiting, dan prioritas lainnya
        if Self::should_execute_immediately(job) {
            None // Eksekusi segera
        } else {
            // Tunda eksekusi berdasarkan prioritas
            let delay_seconds = match job.priority {
                0..=2 => 30,   // Rendah: delay 30 detik
                3..=5 => 10,   // Sedang: delay 10 detik
                6..=7 => 5,    // Tinggi: delay 5 detik
                _ => 0,        // Sangat tinggi: tidak ada delay
            };
            
            Some(Utc::now() + chrono::Duration::seconds(delay_seconds))
        }
    }
}
```

### 5.2. Sistem Penjadwalan

```rust
// File: src/services/scheduler_service.rs
use crate::{
    database::Database,
    models::{Job, JobStatus},
    services::job_service::JobService,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};

pub struct SchedulerService {
    db: Database,
    job_service: JobService,
}

impl SchedulerService {
    pub fn new(db: Database, job_service: JobService) -> Self {
        Self { db, job_service }
    }
    
    pub async fn process_scheduled_jobs(&self) -> Result<(), SqlxError> {
        // Ambil semua job yang dijadwalkan dan waktunya sudah tiba
        let scheduled_jobs = self.db.get_scheduled_jobs_for_execution().await?;
        
        for mut job in scheduled_jobs {
            // Ubah status dari Scheduled ke Pending
            job.status = JobStatus::Pending;
            job.updated_at = Utc::now();
            
            self.db.update_job(job).await?;
        }
        
        Ok(())
    }
    
    pub async fn schedule_job(
        &self,
        queue_id: Uuid,
        job_type: String,
        payload: serde_json::Value,
        priority: i32,
        scheduled_for: DateTime<Utc>,
        max_attempts: Option<i32>,
    ) -> Result<Job, SqlxError> {
        self.job_service
            .create_scheduled_job(
                queue_id,
                job_type,
                payload,
                priority,
                scheduled_for,
                max_attempts,
            )
            .await
    }
    
    pub async fn get_scheduled_jobs(
        &self,
        queue_id: Uuid,
        from_time: Option<DateTime<Utc>>,
        to_time: Option<DateTime<Utc>>,
    ) -> Result<Vec<Job>, SqlxError> {
        self.db.get_scheduled_jobs(queue_id, from_time, to_time).await
    }
}
```

## 6. Best Practices dan Rekomendasi

### 6.1. Praktik Terbaik untuk Queue

1. **Gunakan nama queue yang deskriptif** - untuk memudahkan identifikasi tujuan job
2. **Tetapkan batas ukuran payload** - untuk mencegah penggunaan resource yang berlebihan
3. **Gunakan konfigurasi queue yang sesuai** - untuk setiap jenis beban kerja
4. **Terapkan monitoring dan alerting** - untuk mendeteksi queue yang macet atau bermasalah
5. **Gunakan dead letter queue** - untuk menangani job yang tidak bisa diproses

### 6.2. Praktik Terbaik untuk Job

1. **Desain job untuk idempotensi** - sehingga aman untuk dijalankan ulang
2. **Gunakan payload yang ringan** - untuk mengurangi overhead jaringan dan penyimpanan
3. **Terapkan timeout yang sesuai** - untuk mencegah job yang berjalan terlalu lama
4. **Gunakan retry dengan exponential backoff** - untuk menangani kegagalan sementara
5. **Gunakan logging yang komprehensif** - untuk debugging dan troubleshooting

### 6.3. Skala dan Kinerja

1. **Gunakan indeks yang tepat** - pada kolom-kolom yang sering digunakan untuk query
2. **Optimalkan query database** - terutama untuk operasi pengambilan job
3. **Gunakan connection pooling** - untuk efisiensi koneksi database
4. **Gunakan caching strategi** - untuk informasi queue yang sering diakses
5. **Gunakan partisi tabel** - untuk mengelola data historis dengan efisien