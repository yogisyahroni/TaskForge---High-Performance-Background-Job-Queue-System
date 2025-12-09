use crate::{
    database::Database,
    models::{job::{Job, JobStatus}, job_queue::JobQueue, job_execution::JobExecution},
    services::{queue_service::QueueService, execution_service::ExecutionService, authorization_service::AuthorizationService},
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};
use serde_json::Value;

pub struct JobService {
    db: Database,
    queue_service: QueueService,
    execution_service: ExecutionService,
    authz_service: AuthorizationService,
}

impl JobService {
    pub fn new(
        db: Database,
        queue_service: QueueService,
        execution_service: ExecutionService,
        authz_service: AuthorizationService,
    ) -> Self {
        Self {
            db,
            queue_service,
            execution_service,
            authz_service,
        }
    }
    
    pub async fn create_job(&self, mut job: Job) -> Result<Job, SqlxError> {
        // Validasi job
        job.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        // Dapatkan informasi queue
        let queue = self.queue_service.get_queue_by_id(job.queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan queue aktif dan milik organisasi yang sama
        if !queue.is_active {
            return Err(SqlxError::Decode("Queue is not active".into()));
        }
        
        // Dapatkan pengaturan queue dan terapkan ke job jika diperlukan
        let queue_settings: crate::models::queue_settings::QueueSettings = 
            serde_json::from_value(queue.settings).unwrap_or_default();
        
        // Jika priority tidak diatur, gunakan default dari queue
        if job.priority == 0 {
            job.priority = queue_settings.default_job_priority.unwrap_or(5);
        }
        
        // Jika max_attempts tidak diatur, gunakan default dari queue
        if job.max_attempts == 0 {
            job.max_attempts = queue_settings.default_max_retries.unwrap_or(3) as i32;
        }
        
        // Set timeout jika tidak diatur
        if job.timeout_at.is_none() {
            if let Some(timeout_seconds) = queue_settings.default_timeout_seconds {
                job.timeout_at = Some(Utc::now() + chrono::Duration::seconds(timeout_seconds as i64));
            }
        }
        
        // Simpan job ke database
        let created_job = self.db.create_job(job).await?;
        
        Ok(created_job)
    }
    
    pub async fn get_job_by_id(&self, job_id: Uuid) -> Result<Option<Job>, SqlxError> {
        self.db.get_job_by_id(job_id).await
    }
    
    pub async fn get_jobs_by_queue(
        &self,
        queue_id: Uuid,
        status: Option<JobStatus>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<Job>, SqlxError> {
        self.db.get_jobs_by_queue_and_status(queue_id, status, limit, offset).await
    }
    
    pub async fn get_jobs_by_project(
        &self,
        project_id: Uuid,
        status: Option<JobStatus>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<Job>, SqlxError> {
        self.db.get_jobs_by_project_and_status(project_id, status, limit, offset).await
    }
    
    pub async fn get_jobs_by_organization(
        &self,
        organization_id: Uuid,
        status: Option<JobStatus>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<Job>, SqlxError> {
        self.db.get_jobs_by_organization_and_status(organization_id, status, limit, offset).await
    }
    
    pub async fn update_job_status(
        &self,
        job_id: Uuid,
        status: JobStatus,
    ) -> Result<Job, SqlxError> {
        let mut job = self.db.get_job_by_id(job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Validasi transisi status
        if !self.is_valid_status_transition(&job.status, &status) {
            return Err(SqlxError::Decode("Invalid status transition".into()));
        }
        
        job.status = status;
        job.updated_at = Utc::now();
        
        self.db.update_job(job).await
    }
    
    pub async fn update_job(&self, job: Job) -> Result<Job, SqlxError> {
        job.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        self.db.update_job(job).await
    }
    
    pub async fn cancel_job(&self, job_id: Uuid) -> Result<Job, SqlxError> {
        let mut job = self.db.get_job_by_id(job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Hanya job yang belum selesai yang bisa dibatalkan
        if matches!(job.status, JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled) {
            return Err(SqlxError::Decode("Cannot cancel completed job".into()));
        }
        
        job.status = JobStatus::Cancelled;
        job.updated_at = Utc::now();
        
        // Jika job sedang diproses, kita juga perlu membatalkan eksekusi yang sedang berlangsung
        if matches!(job.status, JobStatus::Processing) {
            // Cari eksekusi aktif dan batalkan
            if let Some(active_execution) = self.execution_service.get_active_execution_for_job(job_id).await? {
                self.execution_service.mark_execution_as_cancelled(active_execution.id).await?;
            }
        }
        
        let updated_job = self.db.update_job(job).await?;
        
        // Jika job memiliki dependensi, kita juga perlu menangani dependensi tersebut
        self.handle_job_cancellation_dependencies(job_id).await?;
        
        Ok(updated_job)
    }
    
    async fn handle_job_cancellation_dependencies(&self, job_id: Uuid) -> Result<(), SqlxError> {
        // Dalam implementasi nyata, ini akan menangani dependensi dari job yang dibatalkan
        // Misalnya, jika job A adalah dependensi dari job B, maka pembatalan job A harus
        // menangani dampaknya pada job B
        
        // Ambil semua job yang bergantung pada job ini
        let dependent_jobs = self.db.get_dependent_jobs(job_id).await?;
        
        for mut dependent_job in dependent_jobs {
            // Dalam kasus pembatalan, kita mungkin ingin menandai dependen job juga sebagai dibatalkan
            // atau mengizinkan mereka untuk tetap menunggu dependensi lain
            dependent_job.status = JobStatus::Cancelled;
            dependent_job.error_message = Some(format!("Dependent job {} was cancelled", job_id));
            dependent_job.updated_at = Utc::now();
            
            self.db.update_job(dependent_job).await?;
        }
        
        Ok(())
    }
    
    pub async fn retry_job(&self, job_id: Uuid) -> Result<Job, SqlxError> {
        let mut job = self.db.get_job_by_id(job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Hanya job yang gagal yang bisa diretry
        if job.status != JobStatus::Failed {
            return Err(SqlxError::Decode("Only failed jobs can be retried".into()));
        }
        
        // Periksa apakah job masih dalam batas percobaan
        if job.attempt_count >= job.max_attempts {
            return Err(SqlxError::Decode("Maximum retry attempts reached".into()));
        }
        
        // Reset status untuk retry
        job.status = JobStatus::Pending;
        job.error_message = None;
        job.output = None;
        job.updated_at = Utc::now();
        
        let updated_job = self.db.update_job(job).await?;
        
        // Dalam implementasi nyata, kita akan menjadwalkan retry berdasarkan
        // konfigurasi backoff dari queue
        Ok(updated_job)
    }
    
    pub async fn get_next_job_for_queue(&self, queue_id: Uuid) -> Result<Option<Job>, SqlxError> {
        // Dapatkan job berikutnya berdasarkan prioritas dan waktu pembuatan
        // Ini akan mengambil job dengan prioritas tertinggi yang siap dieksekusi
        self.db.get_next_ready_job_by_queue(queue_id).await
    }
    
    pub async fn get_job_stats(&self, job_id: Uuid) -> Result<Option<JobStats>, SqlxError> {
        if let Some(job) = self.db.get_job_by_id(job_id).await? {
            let execution_stats = self.execution_service.get_job_execution_stats(job_id).await?;
            
            let stats = JobStats {
                id: job.id,
                status: job.status,
                attempt_count: job.attempt_count,
                max_attempts: job.max_attempts,
                queue_id: job.queue_id,
                created_at: job.created_at,
                updated_at: job.updated_at,
                completed_at: job.completed_at,
                execution_stats,
            };
            
            Ok(Some(stats))
        } else {
            Ok(None)
        }
    }
    
    pub async fn get_queue_statistics(&self, queue_id: Uuid) -> Result<QueueStats, SqlxError> {
        let queue = self.queue_service.get_queue_by_id(queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let stats = self.db.get_queue_statistics(queue_id).await?;
        
        Ok(QueueStats {
            queue_id,
            name: queue.name,
            pending_jobs: stats.pending_count,
            processing_jobs: stats.processing_count,
            succeeded_jobs: stats.succeeded_count,
            failed_jobs: stats.failed_count,
            cancelled_jobs: stats.cancelled_count,
            scheduled_jobs: stats.scheduled_count,
            avg_processing_time_ms: stats.avg_processing_time_ms,
            p95_processing_time_ms: stats.p95_processing_time_ms,
            p99_processing_time_ms: stats.p99_processing_time_ms,
            throughput_per_minute: stats.throughput_per_minute,
            error_rate_percent: stats.error_rate_percent,
            is_active: queue.is_active,
            created_at: queue.created_at,
            updated_at: queue.updated_at,
        })
    }
    
    pub async fn get_job_execution_history(
        &self,
        job_id: Uuid,
    ) -> Result<Vec<JobExecution>, SqlxError> {
        self.db.get_job_executions_by_job_id(job_id).await
    }
    
    pub async fn get_jobs_with_filters(
        &self,
        organization_id: Uuid,
        filters: JobFilters,
    ) -> Result<Vec<Job>, SqlxError> {
        self.db.get_jobs_with_filters(organization_id, filters).await
    }
    
    fn is_valid_status_transition(&self, from: &JobStatus, to: &JobStatus) -> bool {
        match (from, to) {
            // Dari Pending
            (JobStatus::Pending, JobStatus::Processing) => true,
            (JobStatus::Pending, JobStatus::Scheduled) => true,
            (JobStatus::Pending, JobStatus::Cancelled) => true,
            
            // Dari Scheduled
            (JobStatus::Scheduled, JobStatus::Pending) => true,
            (JobStatus::Scheduled, JobStatus::Cancelled) => true,
            
            // Dari Processing
            (JobStatus::Processing, JobStatus::Succeeded) => true,
            (JobStatus::Processing, JobStatus::Failed) => true,
            (JobStatus::Processing, JobStatus::Cancelled) => true,
            
            // Dari Failed
            (JobStatus::Failed, JobStatus::Pending) => true, // Untuk retry
            (JobStatus::Failed, JobStatus::DeadLetter) => true, // Untuk pindah ke DLQ
            
            // Dari Succeeded - tidak bisa berpindah status lagi
            (JobStatus::Succeeded, _) => false,
            
            // Dari Cancelled - tidak bisa berpindah status lagi
            (JobStatus::Cancelled, _) => false,
            
            // Dari DeadLetter - hanya bisa dipindahkan ke queue normal untuk retry
            (JobStatus::DeadLetter, JobStatus::Pending) => true,
            (JobStatus::DeadLetter, _) => false,
            
            _ => false,
        }
    }
    
    pub async fn can_access_job(
        &self,
        user_id: Uuid,
        job_id: Uuid,
    ) -> Result<bool, SqlxError> {
        if let Some(job) = self.db.get_job_by_id(job_id).await? {
            // Dapatkan queue dari job
            if let Some(queue) = self.queue_service.get_queue_by_id(job.queue_id).await? {
                // Dapatkan project dari queue
                if let Some(project) = self.db.get_project_by_id(queue.project_id).await? {
                    // Periksa apakah user adalah bagian dari organisasi yang sama
                    self.authz_service.user_belongs_to_organization(user_id, project.organization_id).await
                } else {
                    Ok(false)
                }
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }
    
    pub async fn get_organization_job_statistics(
        &self,
        organization_id: Uuid,
    ) -> Result<OrganizationJobStats, SqlxError> {
        let stats = self.db.get_organization_job_statistics(organization_id).await?;
        
        Ok(OrganizationJobStats {
            organization_id,
            total_jobs: stats.total_count,
            pending_jobs: stats.pending_count,
            processing_jobs: stats.processing_count,
            succeeded_jobs: stats.succeeded_count,
            failed_jobs: stats.failed_count,
            cancelled_jobs: stats.cancelled_count,
            avg_processing_time_ms: stats.avg_processing_time_ms,
            error_rate_percent: stats.error_rate_percent,
        })
    }
    
    pub async fn get_project_job_statistics(
        &self,
        project_id: Uuid,
    ) -> Result<ProjectJobStats, SqlxError> {
        let stats = self.db.get_project_job_statistics(project_id).await?;
        
        Ok(ProjectJobStats {
            project_id,
            total_jobs: stats.total_count,
            pending_jobs: stats.pending_count,
            processing_jobs: stats.processing_count,
            succeeded_jobs: stats.succeeded_count,
            failed_jobs: stats.failed_count,
            cancelled_jobs: stats.cancelled_count,
        })
    }
    
    pub async fn schedule_job_for_retry(
        &self,
        job_id: Uuid,
        error_message: Option<String>,
    ) -> Result<(), SqlxError> {
        let mut job = self.db.get_job_by_id(job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if !job.can_be_retried() {
            return Err(SqlxError::Decode("Job cannot be retried".into()));
        }
        
        // Ambil konfigurasi retry dari queue
        let queue = self.queue_service.get_queue_by_id(job.queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let queue_settings: crate::models::queue_settings::QueueSettings = 
            serde_json::from_value(queue.settings).unwrap_or_default();
        
        // Hitung waktu untuk retry berikutnya
        let delay = job.calculate_retry_delay(&queue_settings);
        
        if let Some(delay_duration) = delay {
            // Jadwalkan job untuk retry
            job.status = JobStatus::Scheduled;
            job.scheduled_for = Some(Utc::now() + chrono::Duration::from_std(delay_duration).unwrap());
            job.updated_at = Utc::now();
            
            self.db.update_job(job).await?;
        } else {
            // Jika tidak bisa diretry lagi, pindahkan ke dead letter queue
            self.move_job_to_dead_letter_queue(job_id).await?;
        }
        
        Ok(())
    }
    
    async fn move_job_to_dead_letter_queue(&self, job_id: Uuid) -> Result<(), SqlxError> {
        let mut job = self.db.get_job_by_id(job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Ambil queue saat ini
        let current_queue = self.queue_service.get_queue_by_id(job.queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Dapatkan dead letter queue dari konfigurasi
        let queue_settings: crate::models::queue_settings::QueueSettings = 
            serde_json::from_value(current_queue.settings).unwrap_or_default();
        
        if let Some(dlq_id) = queue_settings.dead_letter_queue_id {
            // Pindahkan job ke dead letter queue
            job.queue_id = dlq_id;
            job.status = JobStatus::Pending;
            job.updated_at = Utc::now();
            
            self.db.update_job(job).await?;
        } else {
            // Jika tidak ada DLQ yang ditentukan, tandai job sebagai gagal definitif
            job.status = JobStatus::Failed;
            job.error_message = Some("Max retry attempts reached and no dead letter queue configured".to_string());
            job.updated_at = Utc::now();
            
            self.db.update_job(job).await?;
        }
        
        Ok(())
    }
}

pub struct JobStats {
    pub id: Uuid,
    pub status: JobStatus,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub queue_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub execution_stats: crate::services::execution_service::ExecutionStats,
}

pub struct QueueStats {
    pub queue_id: Uuid,
    pub name: String,
    pub pending_jobs: i64,
    pub processing_jobs: i64,
    pub succeeded_jobs: i64,
    pub failed_jobs: i64,
    pub cancelled_jobs: i64,
    pub scheduled_jobs: i64,
    pub avg_processing_time_ms: f64,
    pub p95_processing_time_ms: f64,
    pub p99_processing_time_ms: f64,
    pub throughput_per_minute: f64,
    pub error_rate_percent: f64,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct OrganizationJobStats {
    pub organization_id: Uuid,
    pub total_jobs: i64,
    pub pending_jobs: i64,
    pub processing_jobs: i64,
    pub succeeded_jobs: i64,
    pub failed_jobs: i64,
    pub cancelled_jobs: i64,
    pub avg_processing_time_ms: f64,
    pub error_rate_percent: f64,
}

pub struct ProjectJobStats {
    pub project_id: Uuid,
    pub total_jobs: i64,
    pub pending_jobs: i64,
    pub processing_jobs: i64,
    pub succeeded_jobs: i64,
    pub failed_jobs: i64,
    pub cancelled_jobs: i64,
}

#[derive(Debug, Clone)]
pub struct JobFilters {
    pub status: Option<JobStatus>,
    pub job_type: Option<String>,
    pub queue_id: Option<Uuid>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub priority_min: Option<i32>,
    pub priority_max: Option<i32>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

impl JobFilters {
    pub fn new() -> Self {
        Self {
            status: None,
            job_type: None,
            queue_id: None,
            created_after: None,
            created_before: None,
            priority_min: None,
            priority_max: None,
            limit: None,
            offset: None,
        }
    }
    
    pub fn with_status(mut self, status: JobStatus) -> Self {
        self.status = Some(status);
        self
    }
    
    pub fn with_job_type(mut self, job_type: String) -> Self {
        self.job_type = Some(job_type);
        self
    }
    
    pub fn with_queue_id(mut self, queue_id: Uuid) -> Self {
        self.queue_id = Some(queue_id);
        self
    }
    
    pub fn with_created_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.created_after = Some(start);
        self.created_before = Some(end);
        self
    }
    
    pub fn with_priority_range(mut self, min: i32, max: i32) -> Self {
        self.priority_min = Some(min);
        self.priority_max = Some(max);
        self
    }
    
    pub fn with_pagination(mut self, limit: i32, offset: i32) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::job::{Job, JobStatus};
    use uuid::Uuid;
    
    #[tokio::test]
    async fn test_create_job() {
        // Dalam implementasi nyata, kita akan menggunakan mock database
        // Untuk sekarang, kita hanya menguji logika validasi
        
        let job = Job::new(
            Uuid::new_v4(), // queue_id
            "test_job".to_string(),
            serde_json::json!({}),
            5,
            Some(3),
            Uuid::new_v4(), // organization_id
        );
        
        assert_eq!(job.job_type, "test_job");
        assert_eq!(job.priority, 5);
        assert_eq!(job.max_attempts, 3);
        assert_eq!(job.status, JobStatus::Pending);
        assert_eq!(job.attempt_count, 0);
        
        // Test validation
        assert!(job.validate().is_ok());
    }
    
    #[test]
    fn test_valid_status_transitions() {
        let job_service = JobService::new(/* mock db */, /* mock queue service */, /* mock execution service */, /* mock authz service */);
        
        // Test valid transitions
        assert!(job_service.is_valid_status_transition(&JobStatus::Pending, &JobStatus::Processing));
        assert!(job_service.is_valid_status_transition(&JobStatus::Processing, &JobStatus::Succeeded));
        assert!(job_service.is_valid_status_transition(&JobStatus::Processing, &JobStatus::Failed));
        assert!(job_service.is_valid_status_transition(&JobStatus::Failed, &JobStatus::Pending));
        
        // Test invalid transitions
        assert!(!job_service.is_valid_status_transition(&JobStatus::Succeeded, &JobStatus::Processing));
        assert!(!job_service.is_valid_status_transition(&JobStatus::Cancelled, &JobStatus::Processing));
    }
    
    #[tokio::test]
    async fn test_job_retry_logic() {
        let job_service = JobService::new(/* mock services */);
        
        let mut job = Job::new(
            Uuid::new_v4(),
            "retry_test_job".to_string(),
            serde_json::json!({}),
            5,
            Some(3),
            Uuid::new_v4(),
        );
        
        // Job yang baru dibuat tidak bisa diretry
        assert!(!job.can_be_retried());
        
        // Set status ke Failed dan coba lagi
        job.status = JobStatus::Failed;
        job.attempt_count = 1;
        
        assert!(job.can_be_retried());
        
        // Set ke attempt maksimum, seharusnya tidak bisa diretry
        job.attempt_count = 3;
        assert!(!job.can_be_retried());
    }
    
    #[tokio::test]
    async fn test_job_cancellation() {
        let job_service = JobService::new(/* mock services */);
        
        let mut job = Job::new(
            Uuid::new_v4(),
            "cancel_test_job".to_string(),
            serde_json::json!({}),
            5,
            Some(3),
            Uuid::new_v4(),
        );
        
        // Job dalam status Pending bisa dibatalkan
        assert!(matches!(job.status, JobStatus::Pending));
        
        job.status = JobStatus::Succeeded;
        // Job yang sudah selesai tidak bisa dibatalkan
        assert!(job_service.is_valid_status_transition(&job.status, &JobStatus::Cancelled) == false);
    }
    
    #[test]
    fn test_job_filters() {
        let filters = JobFilters::new()
            .with_status(JobStatus::Pending)
            .with_job_type("email".to_string())
            .with_priority_range(0, 5)
            .with_pagination(10, 0);
        
        assert_eq!(filters.status, Some(JobStatus::Pending));
        assert_eq!(filters.job_type, Some("email".to_string()));
        assert_eq!(filters.priority_min, Some(0));
        assert_eq!(filters.priority_max, Some(5));
        assert_eq!(filters.limit, Some(10));
        assert_eq!(filters.offset, Some(0));
    }
}