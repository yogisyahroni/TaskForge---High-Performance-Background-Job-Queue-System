# Sistem Eksekusi Job dan Manajemen Lifecycle untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan sistem eksekusi job dan manajemen lifecycle dalam aplikasi TaskForge. Sistem ini bertanggung jawab untuk menjalankan job yang dikirim ke queue, mengelola status eksekusi, serta menyediakan mekanisme untuk retry dan penanganan error.

## 2. Arsitektur Sistem Eksekusi Job

### 2.1. Model Eksekusi Job

Model ini merepresentasikan instance eksekusi spesifik dari job:

```rust
// File: src/models/job_execution.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "execution_status", rename_all = "lowercase")]
pub enum ExecutionStatus {
    Queued,
    Started,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    RetryScheduled,
    DeadLetter,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobExecution {
    pub id: Uuid,
    pub job_id: Uuid,
    pub worker_id: Option<Uuid>,      // ID worker yang mengeksekusi (opsional)
    pub status: ExecutionStatus,
    pub attempt_number: i32,          // Nomor percobaan (untuk retry)
    pub output: Option<String>,       // Output dari eksekusi job
    pub error_message: Option<String>, // Pesan error jika gagal
    pub started_at: Option<DateTime<Utc>>,   // Waktu eksekusi dimulai
    pub finished_at: Option<DateTime<Utc>>,  // Waktu eksekusi selesai
    pub heartbeat_at: Option<DateTime<Utc>>, // Waktu terakhir heartbeat dari worker
    pub timeout_at: Option<DateTime<Utc>>,   // Waktu maksimum untuk eksekusi
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl JobExecution {
    pub fn new(job_id: Uuid, attempt_number: i32) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            job_id,
            worker_id: None,
            status: ExecutionStatus::Queued,
            attempt_number,
            output: None,
            error_message: None,
            started_at: None,
            finished_at: None,
            heartbeat_at: None,
            timeout_at: None,
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.attempt_number < 1 {
            return Err("Attempt number must be at least 1".to_string());
        }
        
        Ok(())
    }
    
    pub fn mark_as_started(&mut self, worker_id: Uuid) {
        self.status = ExecutionStatus::Started;
        self.worker_id = Some(worker_id);
        self.started_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_running(&mut self) {
        self.status = ExecutionStatus::Running;
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_succeeded(&mut self, output: Option<String>) {
        self.status = ExecutionStatus::Succeeded;
        self.output = output;
        self.finished_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_failed(&mut self, error_message: Option<String>) {
        self.status = ExecutionStatus::Failed;
        self.error_message = error_message;
        self.finished_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_cancelled(&mut self) {
        self.status = ExecutionStatus::Cancelled;
        self.finished_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_retry_scheduled(&mut self) {
        self.status = ExecutionStatus::RetryScheduled;
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_dead_letter(&mut self) {
        self.status = ExecutionStatus::DeadLetter;
        self.updated_at = Utc::now();
    }
    
    pub fn update_heartbeat(&mut self) {
        self.heartbeat_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn is_active(&self) -> bool {
        matches!(self.status, ExecutionStatus::Started | ExecutionStatus::Running)
    }
    
    pub fn has_timed_out(&self) -> bool {
        if let Some(timeout_at) = self.timeout_at {
            Utc::now() > timeout_at
        } else {
            false
        }
    }
    
    pub fn get_duration(&self) -> Option<chrono::Duration> {
        if let (Some(started), Some(finished)) = (self.started_at, self.finished_at) {
            Some(finished - started)
        } else {
            None
        }
    }
}
```

### 2.2. Model Log Eksekusi

Model ini menyimpan log detail dari eksekusi job:

```rust
// File: src/models/execution_log.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "log_level", rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ExecutionLog {
    pub id: Uuid,
    pub execution_id: Uuid,
    pub level: LogLevel,
    pub message: String,
    pub metadata: Value,              // JSONB field untuk metadata tambahan
    pub logged_at: DateTime<Utc>,
}

impl ExecutionLog {
    pub fn new(execution_id: Uuid, level: LogLevel, message: String, metadata: Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            execution_id,
            level,
            message,
            metadata,
            logged_at: Utc::now(),
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.message.is_empty() || self.message.len() > 10000 {
            return Err("Log message must be between 1 and 10000 characters".to_string());
        }
        
        Ok(())
    }
}
```

## 3. Sistem Manajemen Lifecycle Eksekusi

### 3.1. Layanan Eksekusi Job

```rust
// File: src/services/execution_service.rs
use crate::{
    database::Database,
    models::{JobExecution, ExecutionStatus, ExecutionLog, LogLevel},
    services::{job_service::JobService, worker_service::WorkerService},
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use serde_json::Value;

pub struct ExecutionService {
    db: Database,
    job_service: JobService,
    worker_service: WorkerService,
}

impl ExecutionService {
    pub fn new(db: Database, job_service: JobService, worker_service: WorkerService) -> Self {
        Self {
            db,
            job_service,
            worker_service,
        }
    }
    
    pub async fn create_execution(
        &self,
        job_id: Uuid,
        attempt_number: i32,
    ) -> Result<JobExecution, SqlxError> {
        let execution = JobExecution::new(job_id, attempt_number);
        execution.validate().map_err(|e| SqlxError::RowNotFound)?;
        
        self.db.create_job_execution(execution).await
    }
    
    pub async fn get_execution_by_id(
        &self,
        execution_id: Uuid,
    ) -> Result<Option<JobExecution>, SqlxError> {
        self.db.get_job_execution_by_id(execution_id).await
    }
    
    pub async fn get_executions_by_job(
        &self,
        job_id: Uuid,
    ) -> Result<Vec<JobExecution>, SqlxError> {
        self.db.get_job_executions_by_job(job_id).await
    }
    
    pub async fn get_latest_execution(
        &self,
        job_id: Uuid,
    ) -> Result<Option<JobExecution>, SqlxError> {
        self.db.get_latest_job_execution(job_id).await
    }
    
    pub async fn update_execution_status(
        &self,
        execution_id: Uuid,
        status: ExecutionStatus,
    ) -> Result<(), SqlxError> {
        self.db.update_job_execution_status(execution_id, status).await
    }
    
    pub async fn mark_execution_as_started(
        &self,
        execution_id: Uuid,
        worker_id: Uuid,
    ) -> Result<(), SqlxError> {
        self.db.mark_execution_as_started(execution_id, worker_id).await
    }
    
    pub async fn mark_execution_as_succeeded(
        &self,
        execution_id: Uuid,
        output: Option<String>,
    ) -> Result<(), SqlxError> {
        self.db.mark_execution_as_succeeded(execution_id, output).await
    }
    
    pub async fn mark_execution_as_failed(
        &self,
        execution_id: Uuid,
        error_message: Option<String>,
    ) -> Result<(), SqlxError> {
        self.db.mark_execution_as_failed(execution_id, error_message).await
    }
    
    pub async fn update_execution_heartbeat(
        &self,
        execution_id: Uuid,
    ) -> Result<(), SqlxError> {
        self.db.update_execution_heartbeat(execution_id).await
    }
    
    pub async fn log_execution_event(
        &self,
        execution_id: Uuid,
        level: LogLevel,
        message: String,
        metadata: Value,
    ) -> Result<(), SqlxError> {
        let log = ExecutionLog::new(execution_id, level, message, metadata);
        log.validate().map_err(|e| SqlxError::RowNotFound)?;
        
        self.db.create_execution_log(log).await
    }
    
    pub async fn get_execution_logs(
        &self,
        execution_id: Uuid,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<ExecutionLog>, SqlxError> {
        self.db.get_execution_logs(execution_id, limit, offset).await
    }
    
    pub async fn get_active_executions(
        &self,
    ) -> Result<Vec<JobExecution>, SqlxError> {
        self.db.get_executions_by_status(ExecutionStatus::Running).await
    }
    
    pub async fn get_timed_out_executions(
        &self,
    ) -> Result<Vec<JobExecution>, SqlxError> {
        self.db.get_timed_out_executions().await
    }
    
    pub async fn handle_execution_timeout(
        &self,
        execution_id: Uuid,
    ) -> Result<(), SqlxError> {
        let mut execution = self.get_execution_by_id(execution_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Tandai eksekusi sebagai gagal karena timeout
        execution.mark_as_failed(Some("Execution timed out".to_string()));
        self.db.update_job_execution(execution).await?;
        
        // Log event timeout
        self.log_execution_event(
            execution_id,
            LogLevel::Error,
            "Execution timed out".to_string(),
            serde_json::json!({"execution_id": execution_id}),
        ).await?;
        
        Ok(())
    }
    
    pub async fn get_execution_stats(
        &self,
        job_id: Uuid,
    ) -> Result<ExecutionStats, SqlxError> {
        let executions = self.get_executions_by_job(job_id).await?;
        
        let mut stats = ExecutionStats {
            total_executions: 0,
            succeeded_count: 0,
            failed_count: 0,
            avg_duration: chrono::Duration::zero(),
            min_duration: None,
            max_duration: None,
        };
        
        let mut total_duration = chrono::Duration::zero();
        let mut durations = Vec::new();
        
        for execution in executions {
            stats.total_executions += 1;
            
            match execution.status {
                ExecutionStatus::Succeeded => {
                    stats.succeeded_count += 1;
                    if let Some(duration) = execution.get_duration() {
                        total_duration = total_duration + duration;
                        durations.push(duration);
                    }
                },
                ExecutionStatus::Failed => {
                    stats.failed_count += 1;
                },
                _ => {}
            }
        }
        
        if !durations.is_empty() {
            stats.avg_duration = total_duration / durations.len() as i32;
            stats.min_duration = Some(*durations.iter().min().unwrap());
            stats.max_duration = Some(*durations.iter().max().unwrap());
        }
        
        Ok(stats)
    }
}

pub struct ExecutionStats {
    pub total_executions: u32,
    pub succeeded_count: u32,
    pub failed_count: u32,
    pub avg_duration: chrono::Duration,
    pub min_duration: Option<chrono::Duration>,
    pub max_duration: Option<chrono::Duration>,
}
```

### 3.2. Proses Eksekusi Job

```rust
// File: src/services/job_processor.rs
use crate::{
    models::{Job, JobExecution, ExecutionStatus, JobStatus},
    services::{execution_service::ExecutionService, job_service::JobService},
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use std::collections::HashMap;

pub struct JobProcessor {
    execution_service: ExecutionService,
    job_service: JobService,
    job_handlers: HashMap<String, Box<dyn JobHandler>>,
}

impl JobProcessor {
    pub fn new(
        execution_service: ExecutionService,
        job_service: JobService,
    ) -> Self {
        Self {
            execution_service,
            job_service,
            job_handlers: HashMap::new(),
        }
    }
    
    pub async fn process_job(
        &self,
        job: &Job,
        worker_id: Uuid,
    ) -> Result<(), SqlxError> {
        // Buat eksekusi baru untuk job ini
        let execution = self.execution_service
            .create_execution(job.id, job.attempt_count)
            .await?;
        
        // Tandai eksekusi sebagai dimulai
        self.execution_service
            .mark_execution_as_started(execution.id, worker_id)
            .await?;
        
        // Ambil handler untuk tipe job ini
        if let Some(handler) = self.job_handlers.get(&job.job_type) {
            // Eksekusi job
            match handler.execute(&job.payload).await {
                Ok(output) => {
                    // Tandai eksekusi sebagai berhasil
                    self.execution_service
                        .mark_execution_as_succeeded(execution.id, output)
                        .await?;
                    
                    // Tandai job sebagai selesai
                    self.job_service
                        .update_job_status(job.id, JobStatus::Succeeded)
                        .await?;
                    
                    // Log keberhasilan
                    self.execution_service
                        .log_execution_event(
                            execution.id,
                            crate::models::LogLevel::Info,
                            "Job executed successfully".to_string(),
                            serde_json::json!({"job_id": job.id}),
                        )
                        .await?;
                },
                Err(error) => {
                    // Tandai eksekusi sebagai gagal
                    self.execution_service
                        .mark_execution_as_failed(execution.id, Some(error.to_string()))
                        .await?;
                    
                    // Tandai job sebagai gagal
                    self.job_service
                        .update_job_status(job.id, JobStatus::Failed)
                        .await?;
                    
                    // Log error
                    self.execution_service
                        .log_execution_event(
                            execution.id,
                            crate::models::LogLevel::Error,
                            format!("Job execution failed: {}", error),
                            serde_json::json!({"job_id": job.id, "error": error.to_string()}),
                        )
                        .await?;
                }
            }
        } else {
            // Handler tidak ditemukan
            self.execution_service
                .mark_execution_as_failed(execution.id, Some("No handler found for job type".to_string()))
                .await?;
            
            self.job_service
                .update_job_status(job.id, JobStatus::Failed)
                .await?;
        }
        
        Ok(())
    }
    
    pub fn register_handler(&mut self, job_type: String, handler: Box<dyn JobHandler>) {
        self.job_handlers.insert(job_type, handler);
    }
}

#[async_trait::async_trait]
pub trait JobHandler: Send + Sync {
    async fn execute(&self, payload: &serde_json::Value) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>>;
}
```

## 4. Sistem Heartbeat dan Timeout

### 4.1. Layanan Heartbeat

```rust
// File: src/services/heartbeat_service.rs
use crate::{
    database::Database,
    models::{JobExecution, ExecutionStatus},
    services::execution_service::ExecutionService,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc, Duration};

pub struct HeartbeatService {
    db: Database,
    execution_service: ExecutionService,
    heartbeat_timeout: Duration, // Durasi sebelum eksekusi dianggap timeout
}

impl HeartbeatService {
    pub fn new(
        db: Database,
        execution_service: ExecutionService,
        heartbeat_timeout_seconds: i64,
    ) -> Self {
        Self {
            db,
            execution_service,
            heartbeat_timeout: Duration::seconds(heartbeat_timeout_seconds),
        }
    }
    
    pub async fn update_heartbeat(
        &self,
        execution_id: Uuid,
    ) -> Result<(), SqlxError> {
        self.execution_service
            .update_execution_heartbeat(execution_id)
            .await
    }
    
    pub async fn check_for_timeout_executions(&self) -> Result<Vec<JobExecution>, SqlxError> {
        let active_executions = self.execution_service.get_active_executions().await?;
        let mut timed_out_executions = Vec::new();
        
        for execution in active_executions {
            if self.is_execution_timed_out(&execution).await? {
                timed_out_executions.push(execution);
            }
        }
        
        Ok(timed_out_executions)
    }
    
    async fn is_execution_timed_out(&self, execution: &JobExecution) -> Result<bool, SqlxError> {
        // Cek apakah ada waktu heartbeat terakhir
        if let Some(heartbeat_at) = execution.heartbeat_at {
            let now = Utc::now();
            let time_since_heartbeat = now - heartbeat_at;
            
            if time_since_heartbeat > self.heartbeat_timeout {
                return Ok(true);
            }
        } else {
            // Jika tidak ada heartbeat sama sekali, cek waktu eksekusi dimulai
            if let Some(started_at) = execution.started_at {
                let now = Utc::now();
                let time_since_start = now - started_at;
                
                if time_since_start > self.heartbeat_timeout {
                    return Ok(true);
                }
            }
        }
        
        Ok(false)
    }
    
    pub async fn handle_timed_out_executions(&self) -> Result<(), SqlxError> {
        let timed_out_executions = self.check_for_timeout_executions().await?;
        
        for execution in timed_out_executions {
            // Tangani timeout eksekusi
            self.execution_service
                .handle_execution_timeout(execution.id)
                .await?;
            
            // Dalam implementasi nyata, mungkin perlu menandai job untuk retry
            // atau memindahkannya ke dead letter queue
        }
        
        Ok(())
    }
}
```

### 4.2. Sistem Timeout Global

```rust
// File: src/services/timeout_manager.rs
use crate::{
    database::Database,
    models::{Job, JobStatus, JobQueue},
    services::{job_service::JobService, queue_service::QueueService},
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc, Duration};

pub struct TimeoutManager {
    db: Database,
    job_service: JobService,
    queue_service: QueueService,
}

impl TimeoutManager {
    pub fn new(
        db: Database,
        job_service: JobService,
        queue_service: QueueService,
    ) -> Self {
        Self {
            db,
            job_service,
            queue_service,
        }
    }
    
    pub async fn check_for_job_timeouts(&self) -> Result<(), SqlxError> {
        let jobs = self.db.get_jobs_with_timeout().await?;
        
        for job in jobs {
            if self.is_job_timed_out(&job).await? {
                self.handle_job_timeout(job).await?;
            }
        }
        
        Ok(())
    }
    
    async fn is_job_timed_out(&self, job: &Job) -> Result<bool, SqlxError> {
        if let Some(timeout_at) = job.timeout_at {
            if Utc::now() > timeout_at {
                return Ok(true);
            }
        }
        
        // Cek apakah job telah melewati batas waktu berdasarkan konfigurasi queue
        let queue = self.queue_service.get_queue_by_id(job.queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Dapatkan konfigurasi timeout dari settings queue
        let queue_settings: crate::models::queue_settings::QueueSettings = 
            serde_json::from_value(queue.settings).unwrap_or_default();
        
        if let Some(timeout_seconds) = queue_settings.timeout_seconds {
            if let Some(started_at) = job.created_at {
                let timeout_duration = Duration::seconds(timeout_seconds as i64);
                let now = Utc::now();
                
                if now - started_at > timeout_duration {
                    return Ok(true);
                }
            }
        }
        
        Ok(false)
    }
    
    async fn handle_job_timeout(&self, job: Job) -> Result<(), SqlxError> {
        // Tandai job sebagai gagal
        self.job_service
            .update_job_status(job.id, JobStatus::Failed)
            .await?;
        
        // Log event timeout
        // Dalam implementasi nyata, ini akan mencatat ke log eksekusi
        
        Ok(())
    }
}
```

## 5. Sistem Retry dan Dead Letter Queue

### 5.1. Layanan Retry

```rust
// File: src/services/retry_service.rs
use crate::{
    database::Database,
    models::{Job, JobStatus, JobQueue},
    services::{job_service::JobService, queue_service::QueueService},
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc, Duration};

pub struct RetryService {
    db: Database,
    job_service: JobService,
    queue_service: QueueService,
}

impl RetryService {
    pub fn new(
        db: Database,
        job_service: JobService,
        queue_service: QueueService,
    ) -> Self {
        Self {
            db,
            job_service,
            queue_service,
        }
    }
    
    pub async fn process_retry_queue(&self) -> Result<(), SqlxError> {
        let jobs_to_retry = self.db.get_jobs_for_retry().await?;
        
        for job in jobs_to_retry {
            self.execute_retry(&job).await?;
        }
        
        Ok(())
    }
    
    pub async fn schedule_retry(
        &self,
        job: &Job,
        failure_reason: &str,
    ) -> Result<(), SqlxError> {
        if job.can_be_retried() {
            // Dapatkan konfigurasi retry dari queue
            let queue = self.queue_service.get_queue_by_id(job.queue_id).await?
                .ok_or(SqlxError::RowNotFound)?;
            
            let queue_settings: crate::models::queue_settings::QueueSettings = 
                serde_json::from_value(queue.settings).unwrap_or_default();
            
            // Hitung delay retry (exponential backoff)
            let delay_seconds = self.calculate_retry_delay(
                job.attempt_count,
                queue_settings.backoff_multiplier.unwrap_or(2.0),
                queue_settings.retry_delay_seconds.unwrap_or(1),
                queue_settings.max_retry_delay_seconds.unwrap_or(300),
            );
            
            // Jadwalkan job untuk retry
            let retry_time = Utc::now() + Duration::seconds(delay_seconds as i64);
            
            // Buat job baru dengan status Scheduled untuk retry
            let mut new_job = job.clone();
            new_job.status = JobStatus::Scheduled;
            new_job.scheduled_for = Some(retry_time);
            new_job.updated_at = Utc::now();
            
            self.db.update_job(new_job).await?;
            
            // Log event retry
            // Dalam implementasi nyata, ini akan mencatat ke log eksekusi
        } else {
            // Job tidak bisa diretry, pindahkan ke dead letter queue
            self.move_to_dead_letter_queue(job).await?;
        }
        
        Ok(())
    }
    
    fn calculate_retry_delay(
        &self,
        attempt_count: i32,
        backoff_multiplier: f64,
        initial_delay: u32,
        max_delay: u32,
    ) -> u32 {
        let delay = initial_delay as f64 * backoff_multiplier.powi(attempt_count - 1);
        delay.min(max_delay as f64) as u32
    }
    
    async fn execute_retry(&self, job: &Job) -> Result<(), SqlxError> {
        // Reset status job untuk retry
        let mut job_to_retry = job.clone();
        job_to_retry.status = JobStatus::Pending;
        job_to_retry.updated_at = Utc::now();
        
        self.db.update_job(job_to_retry).await?;
        
        Ok(())
    }
    
    async fn move_to_dead_letter_queue(&self, job: &Job) -> Result<(), SqlxError> {
        // Dapatkan konfigurasi queue
        let queue = self.queue_service.get_queue_by_id(job.queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let queue_settings: crate::models::queue_settings::QueueSettings = 
            serde_json::from_value(queue.settings).unwrap_or_default();
        
        if let Some(dlq_id) = queue_settings.dead_letter_queue_id {
            // Pindahkan job ke dead letter queue
            let mut job_in_dlq = job.clone();
            job_in_dlq.queue_id = dlq_id;
            job_in_dlq.status = JobStatus::Pending; // Reset status
            job_in_dlq.updated_at = Utc::now();
            
            self.db.update_job(job_in_dlq).await?;
        } else {
            // Jika tidak ada DLQ, tandai job sebagai failed definitif
            self.job_service
                .update_job_status(job.id, JobStatus::Failed)
                .await?;
        }
        
        Ok(())
    }
}
```

## 6. Sistem Monitoring dan Observasi Eksekusi

### 6.1. Layanan Monitoring Eksekusi

```rust
// File: src/services/execution_monitoring_service.rs
use crate::database::Database;
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};

pub struct ExecutionMonitoringService {
    db: Database,
}

impl ExecutionMonitoringService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn get_execution_metrics(
        &self,
        queue_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<ExecutionMetrics, SqlxError> {
        let total_executions = self.db.get_execution_count_in_period(queue_id, start_time, end_time).await?;
        let successful_executions = self.db.get_successful_execution_count_in_period(queue_id, start_time, end_time).await?;
        let failed_executions = self.db.get_failed_execution_count_in_period(queue_id, start_time, end_time).await?;
        
        let avg_duration = self.db.get_avg_execution_duration_in_period(queue_id, start_time, end_time).await?;
        let p95_duration = self.db.get_p95_execution_duration_in_period(queue_id, start_time, end_time).await?;
        let p99_duration = self.db.get_p99_execution_duration_in_period(queue_id, start_time, end_time).await?;
        
        let success_rate = if total_executions > 0 {
            (successful_executions as f64 / total_executions as f64) * 100.0
        } else {
            100.0 // Jika tidak ada eksekusi, tingkat keberhasilan adalah 100%
        };
        
        Ok(ExecutionMetrics {
            queue_id,
            total_executions: total_executions as u64,
            successful_executions: successful_executions as u64,
            failed_executions: failed_executions as u64,
            success_rate,
            avg_duration,
            p95_duration,
            p99_duration,
            period_start: start_time,
            period_end: end_time,
        })
    }
    
    pub async fn get_worker_execution_metrics(
        &self,
        worker_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<WorkerExecutionMetrics, SqlxError> {
        let executions = self.db.get_worker_executions_in_period(worker_id, start_time, end_time).await?;
        
        let mut successful = 0;
        let mut failed = 0;
        let mut total_duration = chrono::Duration::zero();
        
        for execution in executions {
            match execution.status {
                crate::models::ExecutionStatus::Succeeded => successful += 1,
                crate::models::ExecutionStatus::Failed => failed += 1,
                _ => {}
            }
            
            if let Some(duration) = execution.get_duration() {
                total_duration = total_duration + duration;
            }
        }
        
        let avg_duration = if successful + failed > 0 {
            total_duration / (successful + failed) as i32
        } else {
            chrono::Duration::zero()
        };
        
        let success_rate = if successful + failed > 0 {
            (successful as f64 / (successful + failed) as f64) * 100.0
        } else {
            100.0
        };
        
        Ok(WorkerExecutionMetrics {
            worker_id,
            total_executions: (successful + failed) as u64,
            successful_executions: successful as u64,
            failed_executions: failed as u64,
            success_rate,
            avg_duration,
            period_start: start_time,
            period_end: end_time,
        })
    }
}

pub struct ExecutionMetrics {
    pub queue_id: Uuid,
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub success_rate: f64,
    pub avg_duration: f64, // dalam milidetik
    pub p95_duration: f64, // dalam milidetik
    pub p99_duration: f64, // dalam milidetik
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

pub struct WorkerExecutionMetrics {
    pub worker_id: Uuid,
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub success_rate: f64,
    pub avg_duration: chrono::Duration,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}
```

## 7. Best Practices dan Rekomendasi

### 7.1. Praktik Terbaik untuk Eksekusi Job

1. **Gunakan heartbeat secara teratur** - untuk memastikan eksekusi berjalan dengan baik
2. **Terapkan timeout yang sesuai** - untuk mencegah job yang berjalan terlalu lama
3. **Gunakan retry dengan exponential backoff** - untuk menangani kegagalan sementara
4. **Gunakan dead letter queue** - untuk menangani job yang tidak bisa diproses
5. **Gunakan logging yang komprehensif** - untuk debugging dan troubleshooting

### 7.2. Praktik Terbaik untuk Manajemen Lifecycle

1. **Gunakan transaksi database** - untuk memastikan konsistensi status
2. **Terapkan penanganan error yang baik** - untuk menjaga keandalan sistem
3. **Gunakan monitoring dan alerting** - untuk mendeteksi masalah dengan cepat
4. **Gunakan audit trail** - untuk keperluan troubleshooting dan keamanan
5. **Gunakan manajemen sumber daya yang efisien** - untuk mengoptimalkan kinerja

### 7.3. Skala dan Kinerja

1. **Gunakan indeks yang tepat** - pada tabel eksekusi untuk query yang cepat
2. **Optimalkan query database** - terutama untuk operasi histori eksekusi
3. **Gunakan caching strategi** - untuk informasi eksekusi yang sering diakses
4. **Gunakan partisi tabel** - untuk mengelola data historis dengan efisien
5. **Gunakan connection pooling** - untuk efisiensi koneksi database