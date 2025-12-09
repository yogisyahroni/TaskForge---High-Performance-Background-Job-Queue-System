# Sistem Worker dan Assignment untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan sistem worker dan assignment dalam aplikasi TaskForge. Sistem ini bertanggung jawab untuk mengelola worker (proses eksekusi job), menetapkan queue ke worker, dan mengatur distribusi beban kerja secara efisien.

## 2. Arsitektur Sistem Worker

### 2.1. Model Worker

Worker dalam TaskForge adalah entitas yang mengeksekusi job dari queue:

```rust
// File: src/models/worker.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "worker_type", rename_all = "lowercase")]
pub enum WorkerType {
    General,
    SpecializedCpu,
    SpecializedIo,
    SpecializedGpu,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "worker_status", rename_all = "lowercase")]
pub enum WorkerStatus {
    Online,
    Offline,
    Draining,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Worker {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub worker_type: WorkerType,
    pub status: WorkerStatus,
    pub capabilities: serde_json::Value, // JSONB field untuk kemampuan spesifik worker
    pub max_concurrent_jobs: u32,       // Jumlah maksimum job yang bisa diproses bersamaan
    pub last_heartbeat: Option<DateTime<Utc>>, // Waktu terakhir worker mengirim heartbeat
    pub registered_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub drain_mode: bool,               // Apakah worker dalam mode drain
}

impl Worker {
    pub fn new(
        project_id: Uuid,
        name: String,
        worker_type: WorkerType,
        max_concurrent_jobs: u32,
        capabilities: serde_json::Value,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            name,
            worker_type,
            status: WorkerStatus::Offline,
            capabilities,
            max_concurrent_jobs,
            last_heartbeat: None,
            registered_at: now,
            updated_at: now,
            drain_mode: false,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("Worker name must be between 1 and 100 characters".to_string());
        }
        
        if self.max_concurrent_jobs == 0 || self.max_concurrent_jobs > 1000 {
            return Err("Max concurrent jobs must be between 1 and 1000".to_string());
        }
        
        Ok(())
    }
    
    pub fn is_online(&self) -> bool {
        matches!(self.status, WorkerStatus::Online) && !self.drain_mode
    }
    
    pub fn is_healthy(&self, heartbeat_timeout_seconds: i64) -> bool {
        if let Some(last_heartbeat) = self.last_heartbeat {
            let now = Utc::now();
            let duration_since_heartbeat = now.timestamp() - last_heartbeat.timestamp();
            duration_since_heartbeat < heartbeat_timeout_seconds
        } else {
            false
        }
    }
    
    pub fn can_accept_job(&self, heartbeat_timeout_seconds: i64) -> bool {
        self.is_online() && self.is_healthy(heartbeat_timeout_seconds)
    }
    
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Some(Utc::now());
        self.updated_at = Utc::now();
        
        // Jika statusnya offline dan worker mengirim heartbeat, perbarui status
        if matches!(self.status, WorkerStatus::Offline) {
            self.status = WorkerStatus::Online;
        }
    }
    
    pub fn enter_drain_mode(&mut self) {
        self.drain_mode = true;
        self.updated_at = Utc::now();
    }
    
    pub fn exit_drain_mode(&mut self) {
        self.drain_mode = false;
        self.updated_at = Utc::now();
    }
    
    pub fn set_status(&mut self, status: WorkerStatus) {
        self.status = status;
        self.updated_at = Utc::now();
    }
}
```

### 2.2. Model Assignment Queue-Worker

Model ini menggambarkan hubungan antara queue dan worker:

```rust
// File: src/models/queue_worker_assignment.rs
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct QueueWorkerAssignment {
    pub queue_id: Uuid,
    pub worker_id: Uuid,
    pub weight: i32,          // Bobot untuk algoritma load balancing
    pub is_active: bool,      // Apakah assignment aktif
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl QueueWorkerAssignment {
    pub fn new(queue_id: Uuid, worker_id: Uuid, weight: i32) -> Self {
        let now = Utc::now();
        Self {
            queue_id,
            worker_id,
            weight,
            is_active: true,
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.weight < 1 || self.weight > 100 {
            return Err("Assignment weight must be between 1 and 10".to_string());
        }
        
        Ok(())
    }
    
    pub fn can_process_jobs(&self) -> bool {
        self.is_active
    }
}
```

## 3. Sistem Manajemen Worker

### 3.1. Layanan Worker

```rust
// File: src/services/worker_service.rs
use crate::{
    database::Database,
    models::{Worker, WorkerStatus, WorkerType},
    utils::validation,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use serde_json::Value;

pub struct WorkerService {
    db: Database,
}

impl WorkerService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn register_worker(
        &self,
        project_id: Uuid,
        name: String,
        worker_type: WorkerType,
        max_concurrent_jobs: u32,
        capabilities: Value,
    ) -> Result<Worker, SqlxError> {
        let worker = Worker::new(project_id, name, worker_type, max_concurrent_jobs, capabilities);
        worker.validate().map_err(|e| SqlxError::RowNotFound)?;
        
        self.db.create_worker(worker).await
    }
    
    pub async fn get_worker_by_id(
        &self,
        worker_id: Uuid,
    ) -> Result<Option<Worker>, SqlxError> {
        self.db.get_worker_by_id(worker_id).await
    }
    
    pub async fn get_workers_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<Worker>, SqlxError> {
        self.db.get_workers_by_project(project_id).await
    }
    
    pub async fn update_worker(
        &self,
        worker_id: Uuid,
        name: Option<String>,
        max_concurrent_jobs: Option<u32>,
        capabilities: Option<Value>,
    ) -> Result<(), SqlxError> {
        self.db.update_worker(worker_id, name, max_concurrent_jobs, capabilities).await
    }
    
    pub async fn update_worker_status(
        &self,
        worker_id: Uuid,
        status: WorkerStatus,
    ) -> Result<(), SqlxError> {
        self.db.update_worker_status(worker_id, status).await
    }
    
    pub async fn update_worker_heartbeat(
        &self,
        worker_id: Uuid,
    ) -> Result<(), SqlxError> {
        self.db.update_worker_heartbeat(worker_id).await
    }
    
    pub async fn enter_drain_mode(
        &self,
        worker_id: Uuid,
    ) -> Result<(), SqlxError> {
        self.db.set_worker_drain_mode(worker_id, true).await
    }
    
    pub async fn exit_drain_mode(
        &self,
        worker_id: Uuid,
    ) -> Result<(), SqlxError> {
        self.db.set_worker_drain_mode(worker_id, false).await
    }
    
    pub async fn deregister_worker(
        &self,
        worker_id: Uuid,
    ) -> Result<(), SqlxError> {
        // Pastikan worker tidak sedang memproses job
        let active_jobs = self.db.get_active_jobs_for_worker(worker_id).await?;
        if active_jobs > 0 {
            return Err(SqlxError::RowNotFound); // Tidak bisa deregister worker yang sedang aktif
        }
        
        self.db.delete_worker(worker_id).await
    }
    
    pub async fn get_worker_stats(
        &self,
        worker_id: Uuid,
    ) -> Result<WorkerStats, SqlxError> {
        let worker = self.get_worker_by_id(worker_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let active_jobs = self.db.get_active_jobs_for_worker(worker_id).await?;
        let completed_jobs = self.db.get_completed_jobs_for_worker(worker_id).await?;
        let failed_jobs = self.db.get_failed_jobs_for_worker(worker_id).await?;
        
        Ok(WorkerStats {
            worker_id,
            active_jobs,
            completed_jobs,
            failed_jobs,
            status: worker.status,
            last_heartbeat: worker.last_heartbeat,
        })
    }
    
    pub async fn get_workers_by_type(
        &self,
        project_id: Uuid,
        worker_type: WorkerType,
    ) -> Result<Vec<Worker>, SqlxError> {
        self.db.get_workers_by_project_and_type(project_id, worker_type).await
    }
    
    pub async fn get_healthy_workers(
        &self,
        project_id: Uuid,
        heartbeat_timeout_seconds: i64,
    ) -> Result<Vec<Worker>, SqlxError> {
        let workers = self.get_workers_by_project(project_id).await?;
        
        Ok(workers.into_iter()
            .filter(|worker| worker.is_healthy(heartbeat_timeout_seconds))
            .collect())
    }
}

pub struct WorkerStats {
    pub worker_id: Uuid,
    pub active_jobs: i64,
    pub completed_jobs: i64,
    pub failed_jobs: i64,
    pub status: WorkerStatus,
    pub last_heartbeat: Option<DateTime<Utc>>,
}
```

### 3.2. Endpoint API Worker

```rust
// File: src/handlers/worker_handler.rs
use axum::{
    extract::{State, Path, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use crate::{
    services::worker_service::{WorkerService, WorkerStats},
    auth::middleware::{AuthenticatedUser, check_permission},
    models::{Worker, WorkerStatus, WorkerType},
};

#[derive(Deserialize)]
pub struct RegisterWorkerRequest {
    pub name: String,
    pub worker_type: WorkerType,
    pub max_concurrent_jobs: Option<u32>,
    pub capabilities: serde_json::Value,
}

#[derive(Deserialize)]
pub struct UpdateWorkerRequest {
    pub name: Option<String>,
    pub max_concurrent_jobs: Option<u32>,
    pub capabilities: Option<serde_json::Value>,
}

pub async fn register_worker(
    State(worker_service): State<WorkerService>,
    authenticated_user: AuthenticatedUser,
    Json(payload): Json<RegisterWorkerRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "worker:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let worker = worker_service
        .register_worker(
            authenticated_user.organization_id, // Ini seharusnya project_id
            payload.name,
            payload.worker_type,
            payload.max_concurrent_jobs.unwrap_or(10),
            payload.capabilities,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(worker))
}

pub async fn get_worker(
    State(worker_service): State<WorkerService>,
    authenticated_user: AuthenticatedUser,
    Path(worker_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "worker:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let worker = worker_service
        .get_worker_by_id(worker_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan worker milik proyek dalam organisasi yang sama
    let project = /* implementasikan untuk mendapatkan proyek worker */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    Ok(Json(worker))
}

pub async fn get_workers(
    State(worker_service): State<WorkerService>,
    authenticated_user: AuthenticatedUser,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "worker:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Dapatkan semua proyek dalam organisasi
    let projects = /* implementasikan untuk mendapatkan proyek dalam organisasi */;
    
    let mut all_workers = Vec::new();
    for project in projects {
        let project_workers = worker_service
            .get_workers_by_project(project.id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        
        all_workers.extend(project_workers);
    }
    
    Ok(Json(all_workers))
}

pub async fn update_worker(
    State(worker_service): State<WorkerService>,
    authenticated_user: AuthenticatedUser,
    Path(worker_id): Path<uuid::Uuid>,
    Json(payload): Json<UpdateWorkerRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "worker:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let worker = worker_service
        .get_worker_by_id(worker_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan worker milik proyek dalam organisasi yang sama
    let project = /* implementasikan untuk mendapatkan proyek worker */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    worker_service
        .update_worker(
            worker_id,
            payload.name,
            payload.max_concurrent_jobs,
            payload.capabilities,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn deregister_worker(
    State(worker_service): State<WorkerService>,
    authenticated_user: AuthenticatedUser,
    Path(worker_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "worker:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let worker = worker_service
        .get_worker_by_id(worker_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan worker milik proyek dalam organisasi yang sama
    let project = /* implementasikan untuk mendapatkan proyek worker */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    worker_service
        .deregister_worker(worker_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn send_heartbeat(
    State(worker_service): State<WorkerService>,
    authenticated_user: AuthenticatedUser,
    Path(worker_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    // Dalam konteks worker, biasanya ini diotentikasi dengan API key khusus worker
    // atau dengan JWT yang spesifik untuk worker
    
    worker_service
        .update_worker_heartbeat(worker_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn enter_drain_mode(
    State(worker_service): State<WorkerService>,
    authenticated_user: AuthenticatedUser,
    Path(worker_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "worker:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let worker = worker_service
        .get_worker_by_id(worker_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan worker milik proyek dalam organisasi yang sama
    let project = /* implementasikan untuk mendapatkan proyek worker */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    worker_service
        .enter_drain_mode(worker_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn exit_drain_mode(
    State(worker_service): State<WorkerService>,
    authenticated_user: AuthenticatedUser,
    Path(worker_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "worker:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let worker = worker_service
        .get_worker_by_id(worker_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan worker milik proyek dalam organisasi yang sama
    let project = /* implementasikan untuk mendapatkan proyek worker */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    worker_service
        .exit_drain_mode(worker_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_worker_stats(
    State(worker_service): State<WorkerService>,
    authenticated_user: AuthenticatedUser,
    Path(worker_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "worker:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let worker = worker_service
        .get_worker_by_id(worker_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan worker milik proyek dalam organisasi yang sama
    let project = /* implementasikan untuk mendapatkan proyek worker */;
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let stats = worker_service
        .get_worker_stats(worker_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(stats))
}
```

## 4. Sistem Assignment Queue-Worker

### 4.1. Layanan Assignment

```rust
// File: src/services/assignment_service.rs
use crate::{
    database::Database,
    models::{QueueWorkerAssignment, JobQueue, Worker},
};
use uuid::Uuid;
use sqlx::Error as SqlxError;

pub struct AssignmentService {
    db: Database,
}

impl AssignmentService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn create_assignment(
        &self,
        queue_id: Uuid,
        worker_id: Uuid,
        weight: i32,
    ) -> Result<QueueWorkerAssignment, SqlxError> {
        let assignment = QueueWorkerAssignment::new(queue_id, worker_id, weight);
        assignment.validate().map_err(|e| SqlxError::RowNotFound)?;
        
        self.db.create_queue_worker_assignment(assignment).await
    }
    
    pub async fn get_assignment(
        &self,
        queue_id: Uuid,
        worker_id: Uuid,
    ) -> Result<Option<QueueWorkerAssignment>, SqlxError> {
        self.db.get_queue_worker_assignment(queue_id, worker_id).await
    }
    
    pub async fn get_assignments_by_queue(
        &self,
        queue_id: Uuid,
    ) -> Result<Vec<QueueWorkerAssignment>, SqlxError> {
        self.db.get_assignments_by_queue(queue_id).await
    }
    
    pub async fn get_assignments_by_worker(
        &self,
        worker_id: Uuid,
    ) -> Result<Vec<QueueWorkerAssignment>, SqlxError> {
        self.db.get_assignments_by_worker(worker_id).await
    }
    
    pub async fn update_assignment_weight(
        &self,
        queue_id: Uuid,
        worker_id: Uuid,
        new_weight: i32,
    ) -> Result<(), SqlxError> {
        self.db.update_assignment_weight(queue_id, worker_id, new_weight).await
    }
    
    pub async fn activate_assignment(
        &self,
        queue_id: Uuid,
        worker_id: Uuid,
    ) -> Result<(), SqlxError> {
        self.db.update_assignment_active_status(queue_id, worker_id, true).await
    }
    
    pub async fn deactivate_assignment(
        &self,
        queue_id: Uuid,
        worker_id: Uuid,
    ) -> Result<(), SqlxError> {
        self.db.update_assignment_active_status(queue_id, worker_id, false).await
    }
    
    pub async fn remove_assignment(
        &self,
        queue_id: Uuid,
        worker_id: Uuid,
    ) -> Result<(), SqlxError> {
        self.db.delete_queue_worker_assignment(queue_id, worker_id).await
    }
    
    pub async fn get_eligible_workers_for_queue(
        &self,
        queue_id: Uuid,
    ) -> Result<Vec<Worker>, SqlxError> {
        // Dapatkan semua assignment aktif untuk queue ini
        let assignments = self.get_assignments_by_queue(queue_id).await?;
        
        // Dapatkan ID worker dari assignment
        let worker_ids: Vec<Uuid> = assignments
            .iter()
            .filter(|assignment| assignment.is_active)
            .map(|assignment| assignment.worker_id)
            .collect();
        
        // Dapatkan informasi worker
        let mut workers = Vec::new();
        for worker_id in worker_ids {
            if let Some(worker) = self.db.get_worker_by_id(worker_id).await? {
                workers.push(worker);
            }
        }
        
        Ok(workers)
    }
    
    pub async fn get_eligible_assignments_for_worker(
        &self,
        worker_id: Uuid,
    ) -> Result<Vec<QueueWorkerAssignment>, SqlxError> {
        let assignments = self.get_assignments_by_worker(worker_id).await?;
        
        Ok(assignments
            .into_iter()
            .filter(|assignment| assignment.is_active)
            .collect())
    }
    
    pub async fn get_assignment_matrix(
        &self,
        project_id: Uuid,
    ) -> Result<AssignmentMatrix, SqlxError> {
        let queues = self.db.get_job_queues_by_project(project_id).await?;
        let workers = self.db.get_workers_by_project(project_id).await?;
        
        let mut queue_assignments = std::collections::HashMap::new();
        for queue in &queues {
            let assignments = self.get_assignments_by_queue(queue.id).await?;
            queue_assignments.insert(queue.id, assignments);
        }
        
        Ok(AssignmentMatrix {
            queues,
            workers,
            assignments: queue_assignments,
        })
    }
}

pub struct AssignmentMatrix {
    pub queues: Vec<JobQueue>,
    pub workers: Vec<Worker>,
    pub assignments: std::collections::HashMap<Uuid, Vec<QueueWorkerAssignment>>,
}
```

### 4.2. Algoritma Load Balancing

```rust
// File: src/services/load_balancer.rs
use crate::{
    models::{Worker, QueueWorkerAssignment},
    services::assignment_service::AssignmentService,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;

pub struct LoadBalancer {
    assignment_service: AssignmentService,
}

impl LoadBalancer {
    pub fn new(assignment_service: AssignmentService) -> Self {
        Self { assignment_service }
    }
    
    pub async fn select_worker_for_job(
        &self,
        queue_id: Uuid,
        available_workers: Vec<Worker>,
    ) -> Result<Option<Worker>, SqlxError> {
        if available_workers.is_empty() {
            return Ok(None);
        }
        
        // Dapatkan assignment untuk queue ini
        let assignments = self.assignment_service
            .get_assignments_by_queue(queue_id)
            .await?;
        
        // Filter assignment aktif dan cocokkan dengan worker yang tersedia
        let active_assignments: Vec<&QueueWorkerAssignment> = assignments
            .iter()
            .filter(|assignment| assignment.is_active)
            .collect();
        
        if active_assignments.is_empty() {
            return Ok(None);
        }
        
        // Implementasi weighted round-robin berdasarkan bobot assignment
        let mut weighted_workers = Vec::new();
        for worker in available_workers {
            if let Some(assignment) = active_assignments
                .iter()
                .find(|assignment| assignment.worker_id == worker.id) {
                weighted_workers.push((worker, assignment.weight));
            }
        }
        
        if weighted_workers.is_empty() {
            return Ok(None);
        }
        
        // Pilih worker berdasarkan algoritma weighted round-robin
        let selected_worker = self.weighted_round_robin(&weighted_workers);
        Ok(Some(selected_worker))
    }
    
    fn weighted_round_robin(&self, weighted_workers: &[(Worker, i32)]) -> Worker {
        // Implementasi sederhana dari weighted round-robin
        // Dalam implementasi nyata, ini akan melacak posisi terakhir untuk setiap worker
        let total_weight: i32 = weighted_workers.iter().map(|(_, weight)| weight).sum();
        
        if total_weight == 0 {
            return weighted_workers[0].0.clone();
        }
        
        // Untuk sederhananya, kita pilih berdasarkan bobot dan indeks
        let mut current_weight = 0;
        let selection_point = rand::random::<i32>() % total_weight;
        
        for (worker, weight) in weighted_workers {
            current_weight += weight;
            if selection_point < current_weight {
                return worker.clone();
            }
        }
        
        // Fallback ke worker pertama
        weighted_workers[0].0.clone()
    }
    
    pub async fn get_workers_by_load(
        &self,
        queue_id: Uuid,
        project_id: Uuid,
    ) -> Result<Vec<WorkerLoadInfo>, SqlxError> {
        let eligible_workers = self.assignment_service
            .get_eligible_workers_for_queue(queue_id)
            .await?;
        
        let mut worker_loads = Vec::new();
        for worker in eligible_workers {
            let active_jobs = /* implementasikan untuk mendapatkan jumlah job aktif per worker */;
            let max_jobs = worker.max_concurrent_jobs as i64;
            let load_percentage = if max_jobs > 0 {
                (active_jobs * 100) / max_jobs
            } else {
                0
            };
            
            worker_loads.push(WorkerLoadInfo {
                worker,
                active_jobs,
                max_concurrent_jobs: max_jobs as u32,
                load_percentage: load_percentage as u8,
            });
        }
        
        // Urutkan berdasarkan beban (dari terendah ke tertinggi)
        worker_loads.sort_by(|a, b| a.load_percentage.cmp(&b.load_percentage));
        
        Ok(worker_loads)
    }
}

pub struct WorkerLoadInfo {
    pub worker: Worker,
    pub active_jobs: i64,
    pub max_concurrent_jobs: u32,
    pub load_percentage: u8,
}
```

## 5. Sistem Auto-Scaling

### 5.1. Layanan Auto-Scaling

```rust
// File: src/services/auto_scaling_service.rs
use crate::{
    database::Database,
    models::{JobQueue, Worker},
    services::{queue_service::QueueService, worker_service::WorkerService},
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use serde_json::Value;

pub struct AutoScalingService {
    db: Database,
    queue_service: QueueService,
    worker_service: WorkerService,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingPolicy {
    pub queue_id: Uuid,
    pub min_workers: u32,
    pub max_workers: u32,
    pub target_queue_length: u32,
    pub target_worker_utilization: u8, // dalam persen
    pub scale_up_threshold: u32,       // ambang untuk menaikkan worker
    pub scale_down_threshold: u32,     // ambang untuk menurunkan worker
    pub scale_up_cooldown: i64,        // waktu cooldown setelah menaikkan (dalam detik)
    pub scale_down_cooldown: i64,      // waktu cooldown setelah menurunkan (dalam detik)
}

impl AutoScalingService {
    pub fn new(
        db: Database,
        queue_service: QueueService,
        worker_service: WorkerService,
    ) -> Self {
        Self {
            db,
            queue_service,
            worker_service,
        }
    }
    
    pub async fn evaluate_scaling_requirements(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ScalingRecommendation>, SqlxError> {
        let queues = self.queue_service.get_queues_by_project(project_id).await?;
        let mut recommendations = Vec::new();
        
        for queue in queues {
            if let Some(policy) = self.get_scaling_policy(queue.id).await? {
                let queue_stats = self.queue_service.get_queue_stats(queue.id).await?;
                let current_workers = self.get_active_workers_for_queue(queue.id).await?;
                
                let recommendation = self.make_scaling_decision(
                    &policy,
                    &queue_stats,
                    current_workers.len() as u32,
                ).await?;
                
                if let Some(rec) = recommendation {
                    recommendations.push(rec);
                }
            }
        }
        
        Ok(recommendations)
    }
    
    async fn make_scaling_decision(
        &self,
        policy: &ScalingPolicy,
        queue_stats: &crate::services::queue_service::QueueStats,
        current_worker_count: u32,
    ) -> Result<Option<ScalingRecommendation>, SqlxError> {
        let queue_length = queue_stats.pending_count as u32 + queue_stats.processing_count as u32;
        
        // Cek apakah perlu menaikkan jumlah worker
        if queue_length > policy.scale_up_threshold && 
           current_worker_count < policy.max_workers {
            return Ok(Some(ScalingRecommendation {
                queue_id: policy.queue_id,
                action: ScalingAction::ScaleUp,
                count: 1, // Untuk sederhananya, naikkan 1 per iterasi
            }));
        }
        
        // Cek apakah perlu menurunkan jumlah worker
        if queue_length < policy.scale_down_threshold &&
           current_worker_count > policy.min_workers {
            return Ok(Some(ScalingRecommendation {
                queue_id: policy.queue_id,
                action: ScalingAction::ScaleDown,
                count: 1, // Untuk sederhananya, turunkan 1 per iterasi
            }));
        }
        
        Ok(None) // Tidak perlu scaling
    }
    
    pub async fn execute_scaling_action(
        &self,
        recommendation: &ScalingRecommendation,
    ) -> Result<(), SqlxError> {
        match recommendation.action {
            ScalingAction::ScaleUp => {
                self.scale_up(recommendation.queue_id, recommendation.count).await?;
            },
            ScalingAction::ScaleDown => {
                self.scale_down(recommendation.queue_id, recommendation.count).await?;
            },
        }
        
        Ok(())
    }
    
    async fn scale_up(&self, queue_id: Uuid, count: u32) -> Result<(), SqlxError> {
        // Dalam implementasi nyata, ini akan memicu pembuatan worker baru
        // melalui integrasi dengan platform cloud (AWS ASG, K8s HPA, dll)
        
        // Contoh: buat worker baru dengan konfigurasi default
        for _ in 0..count {
            let queue = self.queue_service.get_queue_by_id(queue_id).await?
                .ok_or(SqlxError::RowNotFound)?;
            
            // Dalam implementasi nyata, ini akan memicu pembuatan worker di infrastruktur
            // atau mengirim sinyal ke sistem orkestrasi untuk menaikkan jumlah worker
            println!("Scaling up: Need to add worker for queue {}", queue.name);
        }
        
        Ok(())
    }
    
    async fn scale_down(&self, queue_id: Uuid, count: u32) -> Result<(), SqlxError> {
        // Dapatkan worker yang ditugaskan ke queue ini
        let workers = self.get_active_workers_for_queue(queue_id).await?;
        
        // Masukkan worker ke mode drain (untuk menyelesaikan job yang sedang berjalan)
        let workers_to_drain = workers.into_iter()
            .take(count as usize)
            .collect::<Vec<_>>();
        
        for worker in workers_to_drain {
            self.worker_service.enter_drain_mode(worker.id).await?;
        }
        
        Ok(())
    }
    
    async fn get_scaling_policy(&self, queue_id: Uuid) -> Result<Option<ScalingPolicy>, SqlxError> {
        // Ambil kebijakan scaling dari database
        self.db.get_scaling_policy(queue_id).await
    }
    
    async fn get_active_workers_for_queue(&self, queue_id: Uuid) -> Result<Vec<Worker>, SqlxError> {
        // Dapatkan worker aktif yang ditugaskan ke queue ini
        let assignments = self.db.get_assignments_by_queue(queue_id).await?;
        
        let mut active_workers = Vec::new();
        for assignment in assignments {
            if assignment.is_active {
                if let Some(worker) = self.worker_service.get_worker_by_id(assignment.worker_id).await? {
                    if worker.is_online() {
                        active_workers.push(worker);
                    }
                }
            }
        }
        
        Ok(active_workers)
    }
}

pub struct ScalingRecommendation {
    pub queue_id: Uuid,
    pub action: ScalingAction,
    pub count: u32,
}

#[derive(Debug)]
pub enum ScalingAction {
    ScaleUp,
    ScaleDown,
}
```

## 6. Sistem Monitoring dan Observasi Worker

### 6.1. Metrik Worker

```rust
// File: src/services/metrics_service.rs
use crate::database::Database;
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};

pub struct MetricsService {
    db: Database,
}

impl MetricsService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn get_worker_metrics(
        &self,
        worker_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<WorkerMetrics, SqlxError> {
        let jobs_processed = self.db.get_jobs_processed_by_worker_in_period(
            worker_id, start_time, end_time
        ).await?;
        
        let avg_processing_time = self.db.get_avg_processing_time_for_worker(
            worker_id, start_time, end_time
        ).await?;
        
        let success_rate = self.db.get_success_rate_for_worker(
            worker_id, start_time, end_time
        ).await?;
        
        let cpu_usage = self.db.get_cpu_usage_for_worker(
            worker_id, start_time, end_time
        ).await?;
        
        let memory_usage = self.db.get_memory_usage_for_worker(
            worker_id, start_time, end_time
        ).await?;
        
        Ok(WorkerMetrics {
            worker_id,
            jobs_processed,
            avg_processing_time,
            success_rate,
            cpu_usage,
            memory_usage,
            period_start: start_time,
            period_end: end_time,
        })
    }
    
    pub async fn get_queue_worker_metrics(
        &self,
        queue_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<QueueWorkerMetrics>, SqlxError> {
        let workers = self.db.get_workers_for_queue(queue_id).await?;
        
        let mut metrics = Vec::new();
        for worker in workers {
            let worker_metrics = self.get_worker_metrics(worker.id, start_time, end_time).await?;
            
            metrics.push(QueueWorkerMetrics {
                worker_id: worker.id,
                worker_name: worker.name,
                metrics: worker_metrics,
            });
        }
        
        Ok(metrics)
    }
}

pub struct WorkerMetrics {
    pub worker_id: Uuid,
    pub jobs_processed: u64,
    pub avg_processing_time: f64, // dalam milidetik
    pub success_rate: f64,        // dalam persen
    pub cpu_usage: f64,           // dalam persen
    pub memory_usage: f64,        // dalam persen
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

pub struct QueueWorkerMetrics {
    pub worker_id: Uuid,
    pub worker_name: String,
    pub metrics: WorkerMetrics,
}
```

## 7. Best Practices dan Rekomendasi

### 7.1. Praktik Terbaik untuk Worker

1. **Gunakan heartbeat yang teratur** - untuk memastikan kesehatan worker
2. **Implementasi graceful shutdown** - untuk menyelesaikan job yang sedang berjalan saat dimatikan
3. **Gunakan mode drain dengan benar** - untuk menyelesaikan job sebelum offline
4. **Monitor beban kerja** - untuk menghindari overloading
5. **Gunakan logging yang komprehensif** - untuk debugging dan troubleshooting

### 7.2. Praktik Terbaik untuk Assignment

1. **Gunakan bobot yang sesuai** - untuk distribusi beban yang seimbang
2. **Aktifkan hanya assignment yang diperlukan** - untuk efisiensi
3. **Gunakan kebijakan scaling yang bijak** - untuk mengoptimalkan biaya dan kinerja
4. **Monitor kesehatan worker** - untuk memastikan ketersediaan
5. **Gunakan strategi load balancing yang efektif** - untuk kinerja optimal

### 7.3. Skala dan Kinerja

1. **Gunakan indeks yang tepat** - pada tabel assignment untuk query yang cepat
2. **Optimalkan query database** - terutama untuk operasi assignment
3. **Gunakan caching strategi** - untuk informasi assignment yang sering diakses
4. **Gunakan connection pooling** - untuk efisiensi koneksi database
5. **Gunakan partisi tabel** - untuk mengelola data historis dengan efisien