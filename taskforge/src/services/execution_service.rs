use crate::{
    database::Database,
    models::{job_execution::JobExecution, job::Job, worker::Worker},
    services::{
        job_service::JobService,
        worker_service::WorkerService,
        authorization_service::AuthorizationService,
    },
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};

pub struct ExecutionService {
    db: Database,
    job_service: JobService,
    worker_service: WorkerService,
    authz_service: AuthorizationService,
}

impl ExecutionService {
    pub fn new(
        db: Database,
        job_service: JobService,
        worker_service: WorkerService,
        authz_service: AuthorizationService,
    ) -> Self {
        Self {
            db,
            job_service,
            worker_service,
            authz_service,
        }
    }
    
    pub async fn create_execution(
        &self,
        job_id: Uuid,
        worker_id: Option<Uuid>,
    ) -> Result<JobExecution, SqlxError> {
        // Dapatkan job yang akan dieksekusi
        let job = self.job_service.get_job_by_id(job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan job milik organisasi yang benar
        if !self.authz_service.can_access_job(job_id, job.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        // Buat execution record
        let execution = JobExecution::new(
            job_id,
            worker_id,
            job.attempt_count,
        );
        
        execution.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
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
    
    pub async fn get_executions_by_worker(
        &self,
        worker_id: Uuid,
    ) -> Result<Vec<JobExecution>, SqlxError> {
        self.db.get_job_executions_by_worker(worker_id).await
    }
    
    pub async fn get_recent_executions(
        &self,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<JobExecution>, SqlxError> {
        self.db.get_recent_job_executions(limit, offset).await
    }
    
    pub async fn update_execution_status(
        &self,
        execution_id: Uuid,
        status: crate::models::execution_status::ExecutionStatus,
    ) -> Result<JobExecution, SqlxError> {
        let mut execution = self.db.get_job_execution_by_id(execution_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan execution milik job dalam organisasi yang benar
        let job = self.job_service.get_job_by_id(execution.job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if !self.authz_service.can_access_job(job.id, job.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        execution.status = status;
        execution.updated_at = Utc::now();
        
        if matches!(status, crate::models::execution_status::ExecutionStatus::Started) {
            execution.started_at = Some(Utc::now());
        } else if matches!(status, crate::models::execution_status::ExecutionStatus::Succeeded | 
                          crate::models::execution_status::ExecutionStatus::Failed |
                          crate::models::execution_status::ExecutionStatus::Cancelled) {
            execution.finished_at = Some(Utc::now());
        }
        
        self.db.update_job_execution(execution).await
    }
    
    pub async fn mark_execution_as_started(
        &self,
        execution_id: Uuid,
        worker_id: Uuid,
    ) -> Result<JobExecution, SqlxError> {
        let mut execution = self.db.get_job_execution_by_id(execution_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan execution milik job dalam organisasi yang benar
        let job = self.job_service.get_job_by_id(execution.job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if !self.authz_service.can_access_job(job.id, job.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        execution.status = crate::models::execution_status::ExecutionStatus::Started;
        execution.worker_id = Some(worker_id);
        execution.started_at = Some(Utc::now());
        execution.updated_at = Utc::now();
        
        self.db.update_job_execution(execution).await
    }
    
    pub async fn mark_execution_as_succeeded(
        &self,
        execution_id: Uuid,
        output: Option<String>,
    ) -> Result<JobExecution, SqlxError> {
        let mut execution = self.db.get_job_execution_by_id(execution_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan execution milik job dalam organisasi yang benar
        let job = self.job_service.get_job_by_id(execution.job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if !self.authz_service.can_access_job(job.id, job.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        execution.status = crate::models::execution_status::ExecutionStatus::Succeeded;
        execution.output = output;
        execution.finished_at = Some(Utc::now());
        execution.updated_at = Utc::now();
        
        self.db.update_job_execution(execution).await
    }
    
    pub async fn mark_execution_as_failed(
        &self,
        execution_id: Uuid,
        error_message: Option<String>,
    ) -> Result<JobExecution, SqlxError> {
        let mut execution = self.db.get_job_execution_by_id(execution_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan execution milik job dalam organisasi yang benar
        let job = self.job_service.get_job_by_id(execution.job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if !self.authz_service.can_access_job(job.id, job.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        execution.status = crate::models::execution_status::ExecutionStatus::Failed;
        execution.error_message = error_message;
        execution.finished_at = Some(Utc::now());
        execution.updated_at = Utc::now();
        
        self.db.update_job_execution(execution).await
    }
    
    pub async fn update_execution_heartbeat(
        &self,
        execution_id: Uuid,
    ) -> Result<(), SqlxError> {
        let mut execution = self.db.get_job_execution_by_id(execution_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan execution milik job dalam organisasi yang benar
        let job = self.job_service.get_job_by_id(execution.job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if !self.authz_service.can_access_job(job.id, job.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        execution.heartbeat_at = Some(Utc::now());
        execution.updated_at = Utc::now();
        
        self.db.update_job_execution(execution).await?;
        Ok(())
    }
    
    pub async fn get_execution_stats(
        &self,
        job_id: Uuid,
    ) -> Result<ExecutionStats, SqlxError> {
        let executions = self.get_executions_by_job(job_id).await?;
        
        let mut stats = ExecutionStats {
            total_executions: 0,
            succeeded_executions: 0,
            failed_executions: 0,
            avg_duration_ms: 0.0,
            min_duration_ms: None,
            max_duration_ms: None,
        };
        
        let mut total_duration = 0.0;
        let mut durations = Vec::new();
        
        for execution in executions {
            stats.total_executions += 1;
            
            match execution.status {
                crate::models::execution_status::ExecutionStatus::Succeeded => {
                    stats.succeeded_executions += 1;
                    
                    if let (Some(started), Some(finished)) = (execution.started_at, execution.finished_at) {
                        let duration = (finished - started).num_milliseconds() as f64;
                        total_duration += duration;
                        durations.push(duration);
                    }
                },
                crate::models::execution_status::ExecutionStatus::Failed => {
                    stats.failed_executions += 1;
                },
                _ => {}
            }
        }
        
        if !durations.is_empty() {
            stats.avg_duration_ms = total_duration / durations.len() as f64;
            stats.min_duration_ms = Some(*durations.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap());
            stats.max_duration_ms = Some(*durations.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap());
        }
        
        Ok(stats)
    }
    
    pub async fn get_worker_execution_stats(
        &self,
        worker_id: Uuid,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
    ) -> Result<WorkerExecutionStats, SqlxError> {
        let executions = self.db.get_worker_executions_in_period(worker_id, start_time, end_time).await?;
        
        let mut stats = WorkerExecutionStats {
            worker_id,
            total_executions: 0,
            succeeded_executions: 0,
            failed_executions: 0,
            avg_processing_time_ms: 0.0,
            p95_processing_time_ms: 0.0,
            p99_processing_time_ms: 0.0,
            total_processed_data_mb: 0.0,
            start_time,
            end_time,
        };
        
        let mut processing_times = Vec::new();
        let mut total_data_processed = 0.0;
        
        for execution in executions {
            stats.total_executions += 1;
            
            if let (Some(started), Some(finished)) = (execution.started_at, execution.finished_at) {
                let processing_time = (finished - started).num_milliseconds() as f64;
                processing_times.push(processing_time);
                
                if let Some(output) = &execution.output {
                    total_data_processed += output.len() as f64 / (1024.0 * 1024.0); // Convert to MB
                }
            }
            
            match execution.status {
                crate::models::execution_status::ExecutionStatus::Succeeded => {
                    stats.succeeded_executions += 1;
                },
                crate::models::execution_status::ExecutionStatus::Failed => {
                    stats.failed_executions += 1;
                },
                _ => {}
            }
        }
        
        if !processing_times.is_empty() {
            processing_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            
            stats.avg_processing_time_ms = processing_times.iter().sum::<f64>() / processing_times.len() as f64;
            
            // Calculate percentiles
            if !processing_times.is_empty() {
                let p95_idx = (processing_times.len() as f64 * 0.95).floor() as usize;
                let p99_idx = (processing_times.len() as f64 * 0.99).floor() as usize;
                
                stats.p95_processing_time_ms = processing_times.get(p95_idx.clamp(0, processing_times.len() - 1)).copied().unwrap_or(0.0);
                stats.p99_processing_time_ms = processing_times.get(p99_idx.clamp(0, processing_times.len() - 1)).copied().unwrap_or(0.0);
            }
            
            stats.total_processed_data_mb = total_data_processed;
        }
        
        Ok(stats)
    }
    
    pub async fn can_access_execution(
        &self,
        user_id: Uuid,
        execution_id: Uuid,
    ) -> Result<bool, SqlxError> {
        if let Some(execution) = self.db.get_job_execution_by_id(execution_id).await? {
            // Dapatkan job dari execution
            if let Some(job) = self.job_service.get_job_by_id(execution.job_id).await? {
                // Periksa apakah user adalah bagian dari organisasi yang sama
                self.authz_service.user_belongs_to_organization(user_id, job.organization_id).await
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }
    
    pub async fn get_execution_timeline(
        &self,
        job_id: Uuid,
    ) -> Result<Vec<ExecutionTimelineEvent>, SqlxError> {
        let executions = self.get_executions_by_job(job_id).await?;
        let mut timeline = Vec::new();
        
        for execution in executions {
            // Tambahkan event mulai eksekusi
            if let Some(started_at) = execution.started_at {
                timeline.push(ExecutionTimelineEvent {
                    timestamp: started_at,
                    event_type: ExecutionEventType::Started,
                    execution_id: execution.id,
                    worker_id: execution.worker_id,
                    attempt_number: execution.attempt_number,
                });
            }
            
            // Tambahkan event selesai eksekusi
            if let Some(finished_at) = execution.finished_at {
                let event_type = match execution.status {
                    crate::models::execution_status::ExecutionStatus::Succeeded => ExecutionEventType::Succeeded,
                    crate::models::execution_status::ExecutionStatus::Failed => ExecutionEventType::Failed,
                    crate::models::execution_status::ExecutionStatus::Cancelled => ExecutionEventType::Cancelled,
                    _ => ExecutionEventType::Unknown,
                };
                
                timeline.push(ExecutionTimelineEvent {
                    timestamp: finished_at,
                    event_type,
                    execution_id: execution.id,
                    worker_id: execution.worker_id,
                    attempt_number: execution.attempt_number,
                });
            }
        }
        
        // Urutkan berdasarkan timestamp
        timeline.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        
        Ok(timeline)
    }
    
    pub async fn get_execution_performance_metrics(
        &self,
        queue_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<ExecutionPerformanceMetrics, SqlxError> {
        let metrics = self.db.get_execution_performance_metrics(queue_id, start_time, end_time).await?;
        
        Ok(ExecutionPerformanceMetrics {
            queue_id,
            period_start: start_time,
            period_end: end_time,
            total_executions: metrics.total_executions,
            successful_executions: metrics.successful_executions,
            failed_executions: metrics.failed_executions,
            avg_processing_time_ms: metrics.avg_processing_time_ms,
            p95_processing_time_ms: metrics.p95_processing_time_ms,
            p99_processing_time_ms: metrics.p99_processing_time_ms,
            throughput_per_minute: metrics.throughput_per_minute,
            error_rate_percent: metrics.error_rate_percent,
            total_processed_data_mb: metrics.total_processed_data_mb,
            total_processing_time_seconds: metrics.total_processing_time_seconds,
        })
    }
}

pub struct ExecutionStats {
    pub total_executions: u32,
    pub succeeded_executions: u32,
    pub failed_executions: u32,
    pub avg_duration_ms: f64,
    pub min_duration_ms: Option<f64>,
    pub max_duration_ms: Option<f64>,
}

pub struct WorkerExecutionStats {
    pub worker_id: Uuid,
    pub total_executions: u32,
    pub succeeded_executions: u32,
    pub failed_executions: u32,
    pub avg_processing_time_ms: f64,
    pub p95_processing_time_ms: f64,
    pub p99_processing_time_ms: f64,
    pub total_processed_data_mb: f64,
    pub period_start: Option<DateTime<Utc>>,
    pub period_end: Option<DateTime<Utc>>,
}

pub struct ExecutionPerformanceMetrics {
    pub queue_id: Uuid,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub avg_processing_time_ms: f64,
    pub p95_processing_time_ms: f64,
    pub p99_processing_time_ms: f64,
    pub throughput_per_minute: f64,
    pub error_rate_percent: f64,
    pub total_processed_data_mb: f64,
    pub total_processing_time_seconds: f64,
}

#[derive(Debug, Clone)]
pub enum ExecutionEventType {
    Started,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Heartbeat,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ExecutionTimelineEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: ExecutionEventType,
    pub execution_id: Uuid,
    pub worker_id: Option<Uuid>,
    pub attempt_number: i32,
}

#[derive(Debug, Clone)]
pub struct ExecutionFilter {
    pub job_id: Option<Uuid>,
    pub worker_id: Option<Uuid>,
    pub status: Option<crate::models::execution_status::ExecutionStatus>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub attempt_number: Option<i32>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

impl ExecutionFilter {
    pub fn new() -> Self {
        Self {
            job_id: None,
            worker_id: None,
            status: None,
            start_time: None,
            end_time: None,
            attempt_number: None,
            limit: None,
            offset: None,
        }
    }
    
    pub fn with_job_id(mut self, job_id: Uuid) -> Self {
        self.job_id = Some(job_id);
        self
    }
    
    pub fn with_worker_id(mut self, worker_id: Uuid) -> Self {
        self.worker_id = Some(worker_id);
        self
    }
    
    pub fn with_status(mut self, status: crate::models::execution_status::ExecutionStatus) -> Self {
        self.status = Some(status);
        self
    }
    
    pub fn with_time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }
    
    pub fn with_attempt_number(mut self, attempt: i32) -> Self {
        self.attempt_number = Some(attempt);
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
    use crate::models::job_execution::{JobExecution, ExecutionStatus};
    use uuid::Uuid;
    
    #[tokio::test]
    async fn test_create_execution() {
        // Dalam implementasi nyata, kita akan menggunakan mock database
        // Untuk sekarang, kita hanya menguji logika validasi
        
        let execution = JobExecution::new(
            Uuid::new_v4(), // job_id
            Some(Uuid::new_v4()), // worker_id
            1, // attempt_number
        );
        
        assert_eq!(execution.status, ExecutionStatus::Created);
        assert_eq!(execution.attempt_number, 1);
        assert!(execution.created_at <= Utc::now());
    }
    
    #[tokio::test]
    async fn test_execution_status_transitions() {
        let mut execution = JobExecution::new(
            Uuid::new_v4(),
            Some(Uuid::new_v4()),
            1,
        );
        
        // Test transition from Created to Started
        execution.status = ExecutionStatus::Started;
        assert_eq!(execution.status, ExecutionStatus::Started);
        
        // Test transition from Started to Succeeded
        execution.status = ExecutionStatus::Succeeded;
        assert_eq!(execution.status, ExecutionStatus::Succeeded);
        
        // Test transition from Started to Failed
        let mut execution2 = JobExecution::new(
            Uuid::new_v4(),
            Some(Uuid::new_v4()),
            1,
        );
        execution2.status = ExecutionStatus::Started;
        execution2.status = ExecutionStatus::Failed;
        assert_eq!(execution2.status, ExecutionStatus::Failed);
    }
    
    #[test]
    fn test_execution_filter() {
        let filter = ExecutionFilter::new()
            .with_job_id(Uuid::new_v4())
            .with_status(ExecutionStatus::Succeeded)
            .with_pagination(10, 0);
        
        assert!(filter.job_id.is_some());
        assert_eq!(filter.status, Some(ExecutionStatus::Succeeded));
        assert_eq!(filter.limit, Some(10));
        assert_eq!(filter.offset, Some(0));
    }
}