use crate::{
    database::Database,
    models::{job_queue::{JobQueue, QueueStatus}, job::JobStatus, worker::WorkerStatus},
    services::authorization_service::AuthorizationService,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};
use serde_json::Value;

pub struct QueueService {
    db: Database,
    authz_service: AuthorizationService,
}

impl QueueService {
    pub fn new(db: Database, authz_service: AuthorizationService) -> Self {
        Self { db, authz_service }
    }
    
    pub async fn create_queue(&self, mut queue: JobQueue) -> Result<JobQueue, SqlxError> {
        // Validasi queue
        queue.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        // Pastikan project milik organisasi yang sama dengan user
        if !self.authz_service.can_access_project(queue.project_id, queue.organization_id).await? {
            return Err(SqlxError::RowNotFound); // Atau error unauthorized
        }
        
        // Set default settings jika tidak disediakan
        if queue.settings.is_null() {
            queue.settings = serde_json::json!({
                "max_concurrent_jobs": 10,
                "max_retries": 3,
                "timeout_seconds": 300,
                "retry_delay_seconds": 1,
                "max_retry_delay_seconds": 300,
                "backoff_multiplier": 2.0,
                "enable_dead_letter_queue": true,
                "rate_limit_per_minute": 1000
            });
        }
        
        // Simpan queue ke database
        let created_queue = self.db.create_job_queue(queue).await?;
        
        Ok(created_queue)
    }
    
    pub async fn get_queue_by_id(&self, queue_id: Uuid) -> Result<Option<JobQueue>, SqlxError> {
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
    
    pub async fn get_queues_by_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<JobQueue>, SqlxError> {
        self.db.get_job_queues_by_organization(organization_id).await
    }
    
    pub async fn update_queue(&self, queue: JobQueue) -> Result<JobQueue, SqlxError> {
        queue.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        // Pastikan user dapat mengakses queue ini
        if !self.authz_service.can_access_queue(queue.id, queue.organization_id).await? {
            return Err(SqlxError::RowNotFound); // Atau error unauthorized
        }
        
        self.db.update_job_queue(queue).await
    }
    
    pub async fn delete_queue(&self, queue_id: Uuid) -> Result<(), SqlxError> {
        // Periksa apakah user dapat mengakses queue ini
        let queue = self.db.get_job_queue_by_id(queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if !self.authz_service.can_access_queue(queue_id, queue.organization_id).await? {
            return Err(SqlxError::RowNotFound); // Atau error unauthorized
        }
        
        // Pastikan tidak ada job aktif di queue ini
        let active_jobs = self.db.get_active_jobs_count_by_queue(queue_id).await?;
        if active_jobs > 0 {
            return Err(SqlxError::Decode("Cannot delete queue with active jobs".into()));
        }
        
        self.db.delete_job_queue(queue_id).await?;
        Ok(())
    }
    
    pub async fn pause_queue(&self, queue_id: Uuid) -> Result<JobQueue, SqlxError> {
        let mut queue = self.db.get_job_queue_by_id(queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan user dapat mengakses queue ini
        if !self.authz_service.can_access_queue(queue_id, queue.organization_id).await? {
            return Err(SqlxError::RowNotFound); // Atau error unauthorized
        }
        
        queue.is_active = false;
        queue.updated_at = Utc::now();
        
        self.db.update_job_queue(queue).await
    }
    
    pub async fn resume_queue(&self, queue_id: Uuid) -> Result<JobQueue, SqlxError> {
        let mut queue = self.db.get_job_queue_by_id(queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan user dapat mengakses queue ini
        if !self.authz_service.can_access_queue(queue_id, queue.organization_id).await? {
            return Err(SqlxError::RowNotFound); // Atau error unauthorized
        }
        
        queue.is_active = true;
        queue.updated_at = Utc::now();
        
        self.db.update_job_queue(queue).await
    }
    
    pub async fn get_queue_stats(&self, queue_id: Uuid) -> Result<QueueStats, SqlxError> {
        let queue = self.db.get_job_queue_by_id(queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let stats = self.db.get_queue_statistics(queue_id).await?;
        
        Ok(QueueStats {
            queue_id: queue.id,
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
    
    pub async fn get_queue_health(&self, queue_id: Uuid) -> Result<QueueHealth, SqlxError> {
        let queue = self.db.get_job_queue_by_id(queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let stats = self.get_queue_stats(queue_id).await?;
        
        // Dapatkan informasi worker yang ditugaskan ke queue ini
        let assigned_workers = self.db.get_workers_assigned_to_queue(queue_id).await?;
        let online_workers = assigned_workers.iter()
            .filter(|w| w.status == WorkerStatus::Online)
            .count() as i64;
        
        let health_score = self.calculate_queue_health_score(&stats, online_workers as u32).await;
        
        Ok(QueueHealth {
            queue_id: queue.id,
            name: queue.name,
            status: if queue.is_active { 
                QueueHealthStatus::Healthy 
            } else { 
                QueueHealthStatus::Paused 
            },
            health_score,
            stats,
            online_workers: online_workers as u32,
            total_workers: assigned_workers.len() as u32,
            last_activity: self.db.get_last_queue_activity(queue_id).await?,
        })
    }
    
    async fn calculate_queue_health_score(&self, stats: &QueueStats, online_workers: u32) -> f64 {
        let mut score = 100.0;
        
        // Kurangi skor jika error rate tinggi
        if stats.error_rate_percent > 5.0 {
            score -= (stats.error_rate_percent - 5.0) * 2.0;
        }
        
        // Kurangi skor jika tidak ada worker online
        if online_workers == 0 {
            score -= 50.0;
        }
        
        // Kurangi skor jika backlog terlalu besar
        let total_jobs = stats.pending_jobs + stats.processing_jobs;
        if total_jobs > 1000 { // Ambang batas
            let excess_ratio = ((total_jobs - 1000) as f64 / 1000.0).min(1.0);
            score -= excess_ratio * 30.0;
        }
        
        // Kurangi skor jika throughput rendah
        if stats.throughput_per_minute < 1.0 {
            score -= 20.0;
        }
        
        // Batasi skor antara 0 dan 100
        score.clamp(0.0, 100.0)
    }
    
    pub async fn get_queue_performance_metrics(
        &self,
        queue_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<QueuePerformanceMetrics, SqlxError> {
        let metrics = self.db.get_queue_performance_metrics_in_period(queue_id, start_time, end_time).await?;
        
        Ok(QueuePerformanceMetrics {
            queue_id,
            period_start: start_time,
            period_end: end_time,
            total_jobs: metrics.total_jobs,
            succeeded_jobs: metrics.succeeded_jobs,
            failed_jobs: metrics.failed_jobs,
            avg_processing_time_ms: metrics.avg_processing_time_ms,
            p95_processing_time_ms: metrics.p95_processing_time_ms,
            p99_processing_time_ms: metrics.p99_processing_time_ms,
            throughput_per_minute: metrics.throughput_per_minute,
            error_rate_percent: metrics.error_rate_percent,
            queue_depth: metrics.queue_depth,
            worker_utilization: metrics.worker_utilization,
        })
    }
    
    pub async fn get_organization_queues_health(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<QueueHealth>, SqlxError> {
        let queues = self.get_queues_by_organization(organization_id).await?;
        let mut health_reports = Vec::new();
        
        for queue in queues {
            if let Ok(health) = self.get_queue_health(queue.id).await {
                health_reports.push(health);
            }
        }
        
        Ok(health_reports)
    }
    
    pub async fn can_access_queue(
        &self,
        user_id: Uuid,
        queue_id: Uuid,
    ) -> Result<bool, SqlxError> {
        if let Some(queue) = self.db.get_job_queue_by_id(queue_id).await? {
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
    }
    
    pub async fn get_queue_backlog_prediction(
        &self,
        queue_id: Uuid,
    ) -> Result<QueueBacklogPrediction, SqlxError> {
        let stats = self.get_queue_stats(queue_id).await?;
        
        // Ambil data historis untuk menghitung throughput rata-rata
        let historical_throughput = self.db.get_average_throughput_last_hour(queue_id).await?;
        
        let remaining_jobs = stats.pending_jobs + stats.processing_jobs;
        
        let estimated_completion_minutes = if historical_throughput > 0.0 {
            (remaining_jobs as f64 / historical_throughput).ceil() as i64
        } else {
            i64::MAX // Tidak bisa memprediksi jika throughput 0
        };
        
        Ok(QueueBacklogPrediction {
            queue_id,
            pending_jobs: stats.pending_jobs,
            processing_jobs: stats.processing_jobs,
            estimated_completion_minutes,
            predicted_completion_time: if estimated_completion_minutes != i64::MAX {
                Some(Utc::now() + chrono::Duration::minutes(estimated_completion_minutes))
            } else {
                None
            },
            current_throughput_per_minute: historical_throughput,
        })
    }
    
    pub async fn get_queue_assignment_status(
        &self,
        queue_id: Uuid,
    ) -> Result<QueueAssignmentStatus, SqlxError> {
        let workers = self.db.get_workers_for_queue(queue_id).await?;
        let assignments = self.db.get_queue_worker_assignments(queue_id).await?;
        
        let mut online_workers = 0;
        let mut processing_workers = 0;
        let mut draining_workers = 0;
        let mut offline_workers = 0;
        
        for worker in &workers {
            match worker.status {
                crate::models::worker::WorkerStatus::Online => online_workers += 1,
                crate::models::worker::WorkerStatus::Processing => processing_workers += 1,
                crate::models::worker::WorkerStatus::Draining => draining_workers += 1,
                crate::models::worker::WorkerStatus::Offline => offline_workers += 1,
            }
        }
        
        Ok(QueueAssignmentStatus {
            queue_id,
            total_workers: workers.len() as u32,
            online_workers,
            processing_workers,
            draining_workers,
            offline_workers,
            assignments: assignments.len() as u32,
            active_assignments: assignments.iter().filter(|a| a.is_active).count() as u32,
        })
    }
    
    pub async fn update_queue_settings(
        &self,
        queue_id: Uuid,
        settings: Value,
    ) -> Result<JobQueue, SqlxError> {
        let mut queue = self.db.get_job_queue_by_id(queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan user dapat mengakses queue ini
        if !self.authz_service.can_access_queue(queue_id, queue.organization_id).await? {
            return Err(SqlxError::RowNotFound); // Atau error unauthorized
        }
        
        // Validasi settings baru
        if let Err(e) = self.validate_queue_settings(&settings) {
            return Err(SqlxError::Decode(e.into()));
        }
        
        queue.settings = settings;
        queue.updated_at = Utc::now();
        
        self.db.update_job_queue(queue).await
    }
    
    fn validate_queue_settings(&self, settings: &Value) -> Result<(), String> {
        // Validasi struktur settings
        if !settings.is_object() {
            return Err("Queue settings must be an object".to_string());
        }
        
        // Validasi nilai-nilai individual jika ada
        if let Some(max_concurrent) = settings.get("max_concurrent_jobs").and_then(|v| v.as_i64()) {
            if max_concurrent < 1 || max_concurrent > 1000 {
                return Err("Max concurrent jobs must be between 1 and 1000".to_string());
            }
        }
        
        if let Some(max_retries) = settings.get("max_retries").and_then(|v| v.as_i64()) {
            if max_retries < 1 || max_retries > 10 {
                return Err("Max retries must be between 1 and 10".to_string());
            }
        }
        
        if let Some(timeout) = settings.get("timeout_seconds").and_then(|v| v.as_i64()) {
            if timeout < 1 || timeout > 86400 { // 1 hari
                return Err("Timeout must be between 1 second and 1 day".to_string());
            }
        }
        
        Ok(())
    }
    
    pub async fn get_queues_with_filters(
        &self,
        organization_id: Uuid,
        filters: QueueFilters,
    ) -> Result<Vec<JobQueue>, SqlxError> {
        self.db.get_queues_with_filters(organization_id, filters).await
    }
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

pub struct QueueHealth {
    pub queue_id: Uuid,
    pub name: String,
    pub status: QueueHealthStatus,
    pub health_score: f64,  // 0-100
    pub stats: QueueStats,
    pub online_workers: u32,
    pub total_workers: u32,
    pub last_activity: Option<DateTime<Utc>>,
}

pub enum QueueHealthStatus {
    Healthy,   // 80-100
    Warning,   // 60-79
    Critical,  // 0-59
    Paused,    // Queue dijeda
}

pub struct QueuePerformanceMetrics {
    pub queue_id: Uuid,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_jobs: u64,
    pub succeeded_jobs: u64,
    pub failed_jobs: u64,
    pub avg_processing_time_ms: f64,
    pub p95_processing_time_ms: f64,
    pub p99_processing_time_ms: f64,
    pub throughput_per_minute: f64,
    pub error_rate_percent: f64,
    pub queue_depth: u64,
    pub worker_utilization: f64,  // 0-100%
}

pub struct QueueBacklogPrediction {
    pub queue_id: Uuid,
    pub pending_jobs: i64,
    pub processing_jobs: i64,
    pub estimated_completion_minutes: i64,
    pub predicted_completion_time: Option<DateTime<Utc>>,
    pub current_throughput_per_minute: f64,
}

pub struct QueueAssignmentStatus {
    pub queue_id: Uuid,
    pub total_workers: u32,
    pub online_workers: u32,
    pub processing_workers: u32,
    pub draining_workers: u32,
    pub offline_workers: u32,
    pub assignments: u32,
    pub active_assignments: u32,
}

#[derive(Debug, Clone)]
pub struct QueueFilters {
    pub name_pattern: Option<String>,
    pub is_active: Option<bool>,
    pub project_id: Option<Uuid>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub priority_min: Option<i32>,
    pub priority_max: Option<i32>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

impl QueueFilters {
    pub fn new() -> Self {
        Self {
            name_pattern: None,
            is_active: None,
            project_id: None,
            created_after: None,
            created_before: None,
            priority_min: None,
            priority_max: None,
            limit: None,
            offset: None,
        }
    }
    
    pub fn with_name_pattern(mut self, pattern: &str) -> Self {
        self.name_pattern = Some(pattern.to_string());
        self
    }
    
    pub fn with_active_status(mut self, is_active: bool) -> Self {
        self.is_active = Some(is_active);
        self
    }
    
    pub fn with_project_id(mut self, project_id: Uuid) -> Self {
        self.project_id = Some(project_id);
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
    use crate::models::job_queue::{JobQueue, QueueStatus};
    use uuid::Uuid;
    
    #[tokio::test]
    async fn test_create_queue() {
        // Dalam implementasi nyata, kita akan menggunakan mock database
        // Untuk sekarang, kita hanya menguji logika validasi
        
        let queue = JobQueue::new(
            Uuid::new_v4(), // project_id
            "test_queue".to_string(),
            Some("Test queue description".to_string()),
            5,
            serde_json::json!({
                "max_concurrent_jobs": 10,
                "max_retries": 3
            }),
            Uuid::new_v4(), // organization_id
        );
        
        assert_eq!(queue.name, "test_queue");
        assert_eq!(queue.priority, 5);
        assert!(queue.is_active);
        
        // Test validation
        assert!(queue.validate().is_ok());
    }
    
    #[test]
    fn test_queue_validation() {
        let valid_queue = JobQueue::new(
            Uuid::new_v4(),
            "valid_queue_name".to_string(),
            None,
            5,
            serde_json::json!({}),
            Uuid::new_v4(),
        );
        
        assert!(valid_queue.validate().is_ok());
        
        let invalid_queue = JobQueue::new(
            Uuid::new_v4(),
            "a".repeat(101), // Nama terlalu panjang
            None,
            5,
            serde_json::json!({}),
            Uuid::new_v4(),
        );
        
        assert!(invalid_queue.validate().is_err());
    }
    
    #[tokio::test]
    async fn test_queue_health_calculation() {
        let queue_service = QueueService::new(/* mock db */, /* mock authz */);
        
        let stats = QueueStats {
            queue_id: Uuid::new_v4(),
            name: "test_queue".to_string(),
            pending_jobs: 100,
            processing_jobs: 5,
            succeeded_jobs: 1000,
            failed_jobs: 10,
            cancelled_jobs: 2,
            scheduled_jobs: 5,
            avg_processing_time_ms: 2500.0,
            p95_processing_time_ms: 4000.0,
            p99_processing_time_ms: 6000.0,
            throughput_per_minute: 50.0,
            error_rate_percent: 1.0,
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        let health_score = queue_service.calculate_queue_health_score(&stats, 3).await;
        
        // Karena error rate rendah, ada worker online, dan throughput bagus,
        // skor kesehatan seharusnya tinggi
        assert!(health_score > 80.0);
        
        // Test dengan error rate tinggi
        let mut bad_stats = stats;
        bad_stats.error_rate_percent = 10.0;
        let bad_health_score = queue_service.calculate_queue_health_score(&bad_stats, 3).await;
        
        assert!(bad_health_score < health_score); // Skor seharusnya lebih rendah karena error rate tinggi
    }
    
    #[test]
    fn test_queue_filters() {
        let filters = QueueFilters::new()
            .with_name_pattern("email")
            .with_active_status(true)
            .with_priority_range(3, 7)
            .with_pagination(10, 0);
        
        assert_eq!(filters.name_pattern, Some("email".to_string()));
        assert_eq!(filters.is_active, Some(true));
        assert_eq!(filters.priority_min, Some(3));
        assert_eq!(filters.priority_max, Some(7));
        assert_eq!(filters.limit, Some(10));
        assert_eq!(filters.offset, Some(0));
    }
    
    #[test]
    fn test_queue_settings_validation() {
        let queue_service = QueueService::new(/* mock db */, /* mock authz */);
        
        // Valid settings
        let valid_settings = serde_json::json!({
            "max_concurrent_jobs": 5,
            "max_retries": 3,
            "timeout_seconds": 300
        });
        
        assert!(queue_service.validate_queue_settings(&valid_settings).is_ok());
        
        // Invalid settings - max concurrent jobs too high
        let invalid_settings = serde_json::json!({
            "max_concurrent_jobs": 2000, // Too high
            "max_retries": 3,
            "timeout_seconds": 300
        });
        
        assert!(queue_service.validate_queue_settings(&invalid_settings).is_err());
        
        // Invalid settings - max retries too low
        let invalid_settings2 = serde_json::json!({
            "max_concurrent_jobs": 5,
            "max_retries": 0, // Too low
            "timeout_seconds": 300
        });
        
        assert!(queue_service.validate_queue_settings(&invalid_settings2).is_err());
    }
}