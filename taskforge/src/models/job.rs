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
    pub job_type: String,
    pub payload: Value,              // JSONB field untuk data job
    pub status: JobStatus,
    pub priority: i32,               // 0-9, semakin tinggi semakin prioritas
    pub max_attempts: i32,           // Jumlah maksimum percobaan
    pub attempt_count: i32,          // Jumlah percobaan yang telah dilakukan
    pub output: Option<String>,      // Output dari eksekusi job
    pub error_message: Option<String>, // Pesan error jika job gagal
    pub scheduled_for: Option<DateTime<Utc>>, // Waktu eksekusi dijadwalkan
    pub timeout_at: Option<DateTime<Utc>>,    // Waktu maksimum untuk menyelesaikan job
    pub completed_at: Option<DateTime<Utc>>,  // Waktu job selesai
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub organization_id: Uuid,       // ID organisasi untuk multi-tenant
    pub metadata: Value,             // JSONB field untuk metadata tambahan
}

impl Job {
    pub fn new(
        queue_id: Uuid,
        job_type: String,
        payload: Value,
        priority: i32,
        max_attempts: Option<i32>,
        organization_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            queue_id,
            job_type,
            payload,
            status: JobStatus::Pending,
            priority,
            max_attempts: max_attempts.unwrap_or(3),
            attempt_count: 0,
            output: None,
            error_message: None,
            scheduled_for: None,
            timeout_at: None,
            completed_at: None,
            created_at: now,
            updated_at: now,
            organization_id,
            metadata: serde_json::json!({}),
        }
    }
    
    pub fn new_scheduled(
        queue_id: Uuid,
        job_type: String,
        payload: Value,
        priority: i32,
        scheduled_for: DateTime<Utc>,
        max_attempts: Option<i32>,
        organization_id: Uuid,
    ) -> Self {
        let mut job = Self::new(queue_id, job_type, payload, priority, max_attempts, organization_id);
        job.status = JobStatus::Scheduled;
        job.scheduled_for = Some(scheduled_for);
        job
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.job_type.is_empty() || self.job_type.len() > 100 {
            return Err("Job type must be between 1 and 100 characters".to_string());
        }
        
        if self.priority < 0 || self.priority > 9 {
            return Err("Priority must be between 0 and 9".to_string());
        }
        
        if self.max_attempts < 1 || self.max_attempts > 10 {
            return Err("Max attempts must be between 1 and 10".to_string());
        }
        
        Ok(())
    }
    
    pub fn can_be_executed(&self) -> bool {
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
    
    pub fn is_complete(&self) -> bool {
        matches!(self.status, JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled)
    }
    
    pub fn is_successful(&self) -> bool {
        matches!(self.status, JobStatus::Succeeded)
    }
    
    pub fn is_failed(&self) -> bool {
        matches!(self.status, JobStatus::Failed)
    }
    
    pub fn can_be_retried(&self) -> bool {
        self.is_failed() && self.attempt_count < self.max_attempts
    }
    
    pub fn mark_as_processing(&mut self) {
        self.status = JobStatus::Processing;
        self.attempt_count += 1;
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_succeeded(&mut self, output: Option<String>) {
        self.status = JobStatus::Succeeded;
        self.output = output;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_failed(&mut self, error_message: Option<String>) {
        self.status = JobStatus::Failed;
        self.error_message = error_message;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_cancelled(&mut self) {
        self.status = JobStatus::Cancelled;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_scheduled(&mut self, scheduled_time: DateTime<Utc>) {
        self.status = JobStatus::Scheduled;
        self.scheduled_for = Some(scheduled_time);
        self.updated_at = Utc::now();
    }
    
    pub fn has_expired(&self) -> bool {
        if let Some(timeout_at) = self.timeout_at {
            Utc::now() > timeout_at
        } else {
            false
        }
    }
    
    pub fn get_retry_delay(&self, queue_settings: &QueueSettings) -> Option<chrono::Duration> {
        if !self.can_be_retried() {
            return None;
        }
        
        let attempt = self.attempt_count as u32;
        let base_delay = queue_settings.initial_retry_delay_seconds.unwrap_or(1);
        let max_delay = queue_settings.max_retry_delay_seconds.unwrap_or(300);
        let multiplier = queue_settings.backoff_multiplier.unwrap_or(2.0);
        
        // Calculate exponential backoff: base_delay * multiplier^(attempt-1)
        let calculated_delay = (base_delay as f64) * multiplier.powf((attempt - 1) as f64);
        let capped_delay = calculated_delay.min(max_delay as f64) as i64;
        
        // Add jitter (±10%)
        let jitter_range = (capped_delay as f64 * 0.1) as i64;
        let jitter = rand::random::<i64>() % (jitter_range * 2);
        let final_delay = if jitter <= jitter_range {
            capped_delay.saturating_sub(jitter)
        } else {
            capped_delay.saturating_add(jitter - jitter_range)
        }.max(1); // Ensure at least 1 second delay
        
        Some(chrono::Duration::seconds(final_delay))
    }
    
    pub fn get_queue_position_estimate(&self, db_connection: &PgPool) -> Result<i64, sqlx::Error> {
        use crate::schema::jobs::dsl::*;
        use diesel::prelude::*;
        
        // Dalam implementasi nyata, ini akan menghitung posisi perkiraan job dalam antrian
        // berdasarkan status dan prioritas job lain dalam queue yang sama
        let position = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM jobs WHERE queue_id = $1 AND status = 'pending' AND priority >= $2 AND created_at < $3",
            self.queue_id,
            self.priority,
            self.created_at
        )
        .fetch_one(db_connection)
        .await?;
        
        Ok(position.unwrap_or(0) + 1) // +1 karena posisi dimulai dari 1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueSettings {
    pub max_concurrent_jobs: Option<u32>,
    pub max_retries: Option<u32>,
    pub timeout_seconds: Option<u32>,
    pub initial_retry_delay_seconds: Option<u32>,
    pub max_retry_delay_seconds: Option<u32>,
    pub backoff_multiplier: Option<f64>,
    pub dead_letter_queue_id: Option<Uuid>,
    pub rate_limit: Option<u32>,
    pub enable_metrics: Option<bool>,
    pub enable_logging: Option<bool>,
    pub retention_days: Option<u32>,
}

impl Default for QueueSettings {
    fn default() -> Self {
        Self {
            max_concurrent_jobs: Some(10),
            max_retries: Some(3),
            timeout_seconds: Some(300), // 5 menit
            initial_retry_delay_seconds: Some(1),
            max_retry_delay_seconds: Some(300), // 5 menit
            backoff_multiplier: Some(2.0),
            dead_letter_queue_id: None,
            rate_limit: Some(1000), // 1000 permintaan per menit
            enable_metrics: Some(true),
            enable_logging: Some(true),
            retention_days: Some(30),
        }
    }
}