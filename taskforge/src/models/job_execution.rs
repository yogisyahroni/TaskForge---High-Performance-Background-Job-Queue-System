use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

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
    pub worker_id: Option<Uuid>,        // ID worker yang mengeksekusi (opsional)
    pub status: ExecutionStatus,
    pub attempt_number: i32,            // Nomor percobaan (dimulai dari 1)
    pub output: Option<String>,         // Output dari eksekusi
    pub error_message: Option<String>,  // Pesan error jika gagal
    pub started_at: Option<DateTime<Utc>>,   // Waktu eksekusi dimulai
    pub finished_at: Option<DateTime<Utc>>,  // Waktu eksekusi selesai
    pub heartbeat_at: Option<DateTime<Utc>>, // Waktu heartbeat terakhir dari worker
    pub timeout_at: Option<DateTime<Utc>>,   // Waktu maksimum eksekusi
    pub execution_time_ms: Option<i64>,      // Durasi eksekusi dalam milidetik
    pub worker_metadata: Option<Value>,      // Metadata tambahan dari worker
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
            execution_time_ms: None,
            worker_metadata: None,
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.attempt_number < 1 {
            return Err("Attempt number must be at least 1".to_string());
        }
        
        if let Some(output) = &self.output {
            if output.len() > 10 * 1024 * 1024 { // 10MB
                return Err("Output size exceeds 10MB limit".to_string());
            }
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
        self.execution_time_ms = self.calculate_execution_time();
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_failed(&mut self, error_message: Option<String>) {
        self.status = ExecutionStatus::Failed;
        self.error_message = error_message;
        self.finished_at = Some(Utc::now());
        self.execution_time_ms = self.calculate_execution_time();
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_cancelled(&mut self) {
        self.status = ExecutionStatus::Cancelled;
        self.finished_at = Some(Utc::now());
        self.execution_time_ms = self.calculate_execution_time();
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
    
    pub fn is_complete(&self) -> bool {
        matches!(
            self.status,
            ExecutionStatus::Succeeded | ExecutionStatus::Failed | ExecutionStatus::Cancelled
        )
    }
    
    pub fn is_successful(&self) -> bool {
        matches!(self.status, ExecutionStatus::Succeeded)
    }
    
    pub fn has_timed_out(&self) -> bool {
        if let Some(timeout_at) = self.timeout_at {
            Utc::now() > timeout_at
        } else {
            false
        }
    }
    
    fn calculate_execution_time(&self) -> Option<i64> {
        if let (Some(started), Some(finished)) = (self.started_at, self.finished_at) {
            let duration = finished - started;
            Some(duration.num_milliseconds())
        } else {
            None
        }
    }
    
    pub fn get_duration(&self) -> Option<chrono::Duration> {
        if let (Some(started), Some(finished)) = (self.started_at, self.finished_at) {
            Some(finished - started)
        } else {
            None
        }
    }
    
    pub fn get_worker_load_percentage(&self, max_concurrent_jobs: u32) -> Option<f64> {
        if let Some(worker_id) = self.worker_id {
            // Dalam implementasi nyata, ini akan menghitung beban worker
            // berdasarkan jumlah job aktif di worker tersebut
            Some(0.0) // Placeholder
        } else {
            None
        }
    }
    
    pub fn can_be_cancelled(&self) -> bool {
        matches!(self.status, ExecutionStatus::Queued | ExecutionStatus::Started)
    }
    
    pub fn can_be_retried(&self) -> bool {
        matches!(self.status, ExecutionStatus::Failed) && self.attempt_number < 3 // Misalnya max 3 percobaan
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ExecutionLog {
    pub id: Uuid,
    pub execution_id: Uuid,
    pub level: LogLevel,
    pub message: String,
    pub metadata: Value,           // JSONB field untuk metadata tambahan
    pub logged_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "log_level", rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
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
    
    pub fn is_error(&self) -> bool {
        matches!(self.level, LogLevel::Error | LogLevel::Warn)
    }
    
    pub fn is_info(&self) -> bool {
        matches!(self.level, LogLevel::Info | LogLevel::Debug | LogLevel::Trace)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub execution_id: Uuid,
    pub job_id: Uuid,
    pub worker_id: Option<Uuid>,
    pub execution_time_ms: Option<i64>,
    pub memory_used_mb: Option<f64>,
    pub cpu_usage_percent: Option<f64>,
    pub network_io_bytes: Option<u64>,
    pub recorded_at: DateTime<Utc>,
}

impl ExecutionMetrics {
    pub fn new(execution_id: Uuid, job_id: Uuid) -> Self {
        Self {
            execution_id,
            job_id,
            worker_id: None,
            execution_time_ms: None,
            memory_used_mb: None,
            cpu_usage_percent: None,
            network_io_bytes: None,
            recorded_at: Utc::now(),
        }
    }
    
    pub fn calculate_performance_score(&self) -> Option<f64> {
        if let Some(time_ms) = self.execution_time_ms {
            // Skor kinerja berdasarkan waktu eksekusi
            // Semakin cepat eksekusi, semakin tinggi skornya
            let max_acceptable_time = 10000.0; // 10 detik sebagai baseline
            let score = (1.0 - (time_ms as f64 / max_acceptable_time)).max(0.0).min(100.0);
            Some(score)
        } else {
            None
        }
    }
    
    pub fn is_performant(&self) -> bool {
        if let Some(time_ms) = self.execution_time_ms {
            time_ms < 5000 // Kurang dari 5 detik dianggap performant
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub execution_id: Uuid,
    pub job_id: Uuid,
    pub status: ExecutionStatus,
    pub output: Option<String>,
    pub error_message: Option<String>,
    pub execution_time_ms: Option<i64>,
    pub worker_id: Option<Uuid>,
    pub attempt_number: i32,
    pub completed_at: Option<DateTime<Utc>>,
}

impl ExecutionResult {
    pub fn from_execution(execution: &JobExecution) -> Self {
        Self {
            execution_id: execution.id,
            job_id: execution.job_id,
            status: execution.status.clone(),
            output: execution.output.clone(),
            error_message: execution.error_message.clone(),
            execution_time_ms: execution.execution_time_ms,
            worker_id: execution.worker_id,
            attempt_number: execution.attempt_number,
            completed_at: execution.finished_at,
        }
    }
    
    pub fn is_successful(&self) -> bool {
        matches!(self.status, ExecutionStatus::Succeeded)
    }
    
    pub fn get_success_rate(&self) -> f64 {
        if self.is_successful() { 100.0 } else { 0.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionFilter {
    pub job_id: Option<Uuid>,
    pub worker_id: Option<Uuid>,
    pub status: Option<ExecutionStatus>,
    pub attempt_number: Option<i32>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

impl ExecutionFilter {
    pub fn new() -> Self {
        Self {
            job_id: None,
            worker_id: None,
            status: None,
            attempt_number: None,
            start_time: None,
            end_time: None,
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
    
    pub fn with_status(mut self, status: ExecutionStatus) -> Self {
        self.status = Some(status);
        self
    }
    
    pub fn with_attempt_number(mut self, attempt_number: i32) -> Self {
        self.attempt_number = Some(attempt_number);
        self
    }
    
    pub fn with_time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }
    
    pub fn with_pagination(mut self, limit: i32, offset: i32) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionStats {
    pub total_executions: u64,
    pub successful_executions: u64,
    pub failed_executions: u64,
    pub avg_execution_time_ms: f64,
    pub p95_execution_time_ms: f64,
    pub p99_execution_time_ms: f64,
    pub success_rate_percent: f64,
    pub error_rate_percent: f64,
    pub throughput_per_minute: f64,
}

impl ExecutionStats {
    pub fn new() -> Self {
        Self {
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            avg_execution_time_ms: 0.0,
            p95_execution_time_ms: 0.0,
            p99_execution_time_ms: 0.0,
            success_rate_percent: 0.0,
            error_rate_percent: 0.0,
            throughput_per_minute: 0.0,
        }
    }
    
    pub fn calculate_rates(&mut self) {
        if self.total_executions > 0 {
            self.success_rate_percent = (self.successful_executions as f64 / self.total_executions as f64) * 100.0;
            self.error_rate_percent = (self.failed_executions as f64 / self.total_executions as f64) * 100.0;
        } else {
            self.success_rate_percent = 100.0;
            self.error_rate_percent = 0.0;
        }
    }
    
    pub fn add_execution(&mut self, execution: &JobExecution) {
        self.total_executions += 1;
        
        match execution.status {
            ExecutionStatus::Succeeded => self.successful_executions += 1,
            ExecutionStatus::Failed => self.failed_executions += 1,
            _ => {} // Status lain tidak dihitung dalam statistik keberhasilan
        }
        
        if let Some(time_ms) = execution.execution_time_ms {
            // Untuk menghitung avg_execution_time, kita perlu menyimpan semua nilai
            // atau menggunakan teknik streaming average
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    use chrono::Utc;
    
    #[test]
    fn test_job_execution_creation() {
        let job_id = Uuid::new_v4();
        let execution = JobExecution::new(job_id, 1);
        
        assert_eq!(execution.job_id, job_id);
        assert_eq!(execution.attempt_number, 1);
        assert_eq!(execution.status, ExecutionStatus::Queued);
        assert!(execution.created_at <= Utc::now());
        assert!(execution.validate().is_ok());
    }
    
    #[test]
    fn test_job_execution_status_transitions() {
        let mut execution = JobExecution::new(Uuid::new_v4(), 1);
        let worker_id = Uuid::new_v4();
        
        // Test transition from Queued to Started
        execution.mark_as_started(worker_id);
        assert_eq!(execution.status, ExecutionStatus::Started);
        assert!(execution.started_at.is_some());
        assert_eq!(execution.worker_id, Some(worker_id));
        
        // Test transition from Started to Running
        execution.mark_as_running();
        assert_eq!(execution.status, ExecutionStatus::Running);
        
        // Test transition from Running to Succeeded
        execution.mark_as_succeeded(Some("Success output".to_string()));
        assert_eq!(execution.status, ExecutionStatus::Succeeded);
        assert!(execution.finished_at.is_some());
        assert!(execution.execution_time_ms.is_some());
        
        // Test transition from Running to Failed
        let mut execution2 = JobExecution::new(Uuid::new_v4(), 1);
        execution2.mark_as_started(Uuid::new_v4());
        execution2.mark_as_running();
        execution2.mark_as_failed(Some("Error occurred".to_string()));
        assert_eq!(execution2.status, ExecutionStatus::Failed);
        assert!(execution2.finished_at.is_some());
        assert!(execution2.error_message.is_some());
        assert!(execution2.execution_time_ms.is_some());
    }
    
    #[test]
    fn test_job_execution_completion_status() {
        let mut execution = JobExecution::new(Uuid::new_v4(), 1);
        
        assert!(!execution.is_complete());
        
        execution.mark_as_succeeded(None);
        assert!(execution.is_complete());
        
        let mut execution2 = JobExecution::new(Uuid::new_v4(), 1);
        execution2.mark_as_failed(None);
        assert!(execution2.is_complete());
        
        let mut execution3 = JobExecution::new(Uuid::new_v4(), 1);
        execution3.mark_as_cancelled();
        assert!(execution3.is_complete());
    }
    
    #[test]
    fn test_job_execution_success_status() {
        let mut execution = JobExecution::new(Uuid::new_v4(), 1);
        
        assert!(!execution.is_successful());
        
        execution.mark_as_succeeded(None);
        assert!(execution.is_successful());
        
        let mut execution2 = JobExecution::new(Uuid::new_v4(), 1);
        execution2.mark_as_failed(None);
        assert!(!execution2.is_successful());
    }
    
    #[test]
    fn test_job_execution_time_calculation() {
        let mut execution = JobExecution::new(Uuid::new_v4(), 1);
        let start_time = Utc::now();
        execution.started_at = Some(start_time);
        
        // Simulasikan selesai 1 detik kemudian
        let finish_time = start_time + chrono::Duration::seconds(1);
        execution.finished_at = Some(finish_time);
        
        let duration = execution.get_duration();
        assert!(duration.is_some());
        assert_eq!(duration.unwrap().num_milliseconds(), 1000);
    }
    
    #[test]
    fn test_execution_log_creation() {
        let execution_id = Uuid::new_v4();
        let log = ExecutionLog::new(
            execution_id,
            LogLevel::Info,
            "Test log message".to_string(),
            serde_json::json!({"test": "data"}),
        );
        
        assert_eq!(log.execution_id, execution_id);
        assert_eq!(log.level, LogLevel::Info);
        assert_eq!(log.message, "Test log message");
        assert!(log.logged_at <= Utc::now());
        assert!(log.validate().is_ok());
    }
    
    #[test]
    fn test_execution_log_validation() {
        let log = ExecutionLog::new(
            Uuid::new_v4(),
            LogLevel::Error,
            "A".repeat(10001), // Terlalu panjang
            serde_json::json!({}),
        );
        
        assert!(log.validate().is_err());
    }
    
    #[test]
    fn test_execution_log_error_check() {
        let error_log = ExecutionLog::new(
            Uuid::new_v4(),
            LogLevel::Error,
            "Error occurred".to_string(),
            serde_json::json!({}),
        );
        
        let warn_log = ExecutionLog::new(
            Uuid::new_v4(),
            LogLevel::Warn,
            "Warning occurred".to_string(),
            serde_json::json!({}),
        );
        
        let info_log = ExecutionLog::new(
            Uuid::new_v4(),
            LogLevel::Info,
            "Info message".to_string(),
            serde_json::json!({}),
        );
        
        assert!(error_log.is_error());
        assert!(warn_log.is_error());
        assert!(!info_log.is_error());
    }
    
    #[test]
    fn test_execution_metrics() {
        let execution_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        let metrics = ExecutionMetrics::new(execution_id, job_id);
        
        assert_eq!(metrics.execution_id, execution_id);
        assert_eq!(metrics.job_id, job_id);
        assert!(metrics.recorded_at <= Utc::now());
    }
    
    #[test]
    fn test_execution_filter() {
        let worker_id = Uuid::new_v4();
        let filter = ExecutionFilter::new()
            .with_worker_id(worker_id)
            .with_status(ExecutionStatus::Succeeded)
            .with_pagination(10, 0);
        
        assert_eq!(filter.worker_id, Some(worker_id));
        assert_eq!(filter.status, Some(ExecutionStatus::Succeeded));
        assert_eq!(filter.limit, Some(10));
        assert_eq!(filter.offset, Some(0));
    }
    
    #[test]
    fn test_execution_stats_calculation() {
        let mut stats = ExecutionStats::new();
        
        // Simulasi 10 eksekusi, 8 berhasil, 2 gagal
        stats.total_executions = 10;
        stats.successful_executions = 8;
        stats.failed_executions = 2;
        
        stats.calculate_rates();
        
        assert_eq!(stats.success_rate_percent, 80.0);
        assert_eq!(stats.error_rate_percent, 20.0);
    }
}