# Sistem Retry dengan Backoff untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan sistem retry dengan backoff dalam aplikasi TaskForge. Sistem ini bertanggung jawab untuk menangani kegagalan job dengan cara yang efisien dan ramah terhadap sistem, menggunakan berbagai strategi backoff untuk mencegah pengulangan yang berlebihan.

## 2. Konsep Dasar Retry dan Backoff

### 2.1. Jenis-jenis Kegagalan

Dalam sistem queue dan job processing, ada beberapa jenis kegagalan yang mungkin terjadi:

- **Kegagalan sementara (Transient failures)**: Kegagalan yang mungkin berhasil jika dicoba kembali (misalnya timeout jaringan, ketergantungan sementara tidak tersedia)
- **Kegagalan permanen (Permanent failures)**: Kegagalan yang tidak akan berhasil meskipun dicoba berkali-kali (misalnya data input yang salah)
- **Kegagalan karena beban (Load-related failures)**: Kegagalan karena sistem terlalu sibuk atau sumber daya tidak mencukupi

### 2.2. Strategi Retry

TaskForge menerapkan beberapa strategi retry:

1. **Fixed Delay**: Retry dengan jeda waktu tetap
2. **Exponential Backoff**: Retry dengan jeda waktu yang meningkat secara eksponensial
3. **Exponential Backoff with Jitter**: Exponential backoff dengan penambahan acak untuk mencegah thundering herd
4. **Linear Backoff**: Retry dengan jeda waktu yang meningkat secara linear

## 3. Model dan Konfigurasi Retry

### 3.1. Model Konfigurasi Retry

```rust
// File: src/models/retry_config.rs
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RetryConfig {
    pub max_attempts: u32,                    // Jumlah maksimum percobaan
    pub initial_delay_seconds: u32,          // Delay awal dalam detik
    pub max_delay_seconds: u32,              // Delay maksimum dalam detik
    pub backoff_multiplier: f64,             // Multiplier untuk exponential backoff
    pub retry_on_status_codes: Vec<i32>,     // Kode status HTTP yang akan diretry
    pub retry_on_errors: Vec<String>,        // Jenis error yang akan diretry
    pub enable_jitter: bool,                 // Apakah menggunakan jitter
    pub jitter_factor: f64,                  // Faktor jitter (0.0 - 1.0)
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RetryConfig {
    pub fn new(
        max_attempts: u32,
        initial_delay_seconds: u32,
        max_delay_seconds: u32,
        backoff_multiplier: f64,
    ) -> Self {
        Self {
            max_attempts,
            initial_delay_seconds,
            max_delay_seconds,
            backoff_multiplier,
            retry_on_status_codes: vec![500, 502, 503, 504], // HTTP server errors
            retry_on_errors: vec![
                "timeout".to_string(),
                "connection_error".to_string(),
                "temporary_failure".to_string(),
            ],
            enable_jitter: true,
            jitter_factor: 0.1, // 10% jitter
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
    
    pub fn calculate_delay(&self, attempt_number: u32) -> std::time::Duration {
        if attempt_number == 1 {
            return std::time::Duration::from_secs(self.initial_delay_seconds as u64);
        }
        
        // Hitung delay eksponensial
        let base_delay = self.initial_delay_seconds as f64;
        let multiplier = self.backoff_multiplier.powi((attempt_number - 1) as i32);
        let mut delay = base_delay * multiplier;
        
        // Batasi delay maksimum
        delay = delay.min(self.max_delay_seconds as f64);
        
        // Tambahkan jitter jika diaktifkan
        let delay = if self.enable_jitter {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let jitter_range = delay * self.jitter_factor;
            let jitter = rng.gen_range(-jitter_range..jitter_range);
            (delay + jitter).max(0.0) // Pastikan delay tidak negatif
        } else {
            delay
        };
        
        std::time::Duration::from_secs_f64(delay)
    }
    
    pub fn should_retry(&self, attempt_number: u32, error_type: &str) -> bool {
        attempt_number <= self.max_attempts && 
        self.retry_on_errors.contains(&error_type.to_lowercase())
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.max_attempts == 0 {
            return Err("Max attempts must be greater than 0".to_string());
        }
        
        if self.initial_delay_seconds == 0 {
            return Err("Initial delay must be greater than 0".to_string());
        }
        
        if self.backoff_multiplier < 1.0 {
            return Err("Backoff multiplier must be at least 1.0".to_string());
        }
        
        if self.jitter_factor < 0.0 || self.jitter_factor > 1.0 {
            return Err("Jitter factor must be between 0.0 and 1.0".to_string());
        }
        
        Ok(())
    }
}

// Implementasi default untuk RetryConfig
impl Default for RetryConfig {
    fn default() -> Self {
        Self::new(3, 1, 300, 2.0) // 3 kali retry, delay awal 1 detik, maks 5 menit, multiplier 2
    }
}
```

### 3.2. Model Retry History

```rust
// File: src/models/retry_history.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "retry_status", rename_all = "lowercase")]
pub enum RetryStatus {
    Scheduled,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RetryHistory {
    pub id: Uuid,
    pub job_id: Uuid,
    pub execution_id: Option<Uuid>,  // ID eksekusi yang gagal
    pub attempt_number: u32,         // Nomor percobaan (dimulai dari 2 karena 1 adalah percobaan awal)
    pub scheduled_at: DateTime<Utc>, // Waktu retry dijadwalkan
    pub executed_at: Option<DateTime<Utc>>, // Waktu retry dieksekusi
    pub status: RetryStatus,
    pub error_message: Option<String>, // Error dari percobaan sebelumnya
    pub next_retry_at: Option<DateTime<Utc>>, // Waktu retry berikutnya
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RetryHistory {
    pub fn new(
        job_id: Uuid,
        execution_id: Option<Uuid>,
        attempt_number: u32,
        scheduled_at: DateTime<Utc>,
        error_message: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            job_id,
            execution_id,
            attempt_number,
            scheduled_at,
            executed_at: None,
            status: RetryStatus::Scheduled,
            error_message,
            next_retry_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
    
    pub fn mark_as_executed(&mut self) {
        self.status = RetryStatus::Completed;
        self.executed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_failed(&mut self) {
        self.status = RetryStatus::Failed;
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_cancelled(&mut self) {
        self.status = RetryStatus::Cancelled;
        self.updated_at = Utc::now();
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.attempt_number < 2 {
            return Err("Retry attempt number must be at least 2".to_string());
        }
        
        Ok(())
    }
}
```

## 4. Layanan Retry dan Backoff

### 4.1. Layanan Retry Utama

```rust
// File: src/services/retry_service.rs
use crate::{
    database::Database,
    models::{Job, JobStatus, RetryConfig, RetryHistory, RetryStatus},
    services::{job_service::JobService, execution_service::ExecutionService},
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc, Duration};

pub struct RetryService {
    db: Database,
    job_service: JobService,
    execution_service: ExecutionService,
}

impl RetryService {
    pub fn new(
        db: Database,
        job_service: JobService,
        execution_service: ExecutionService,
    ) -> Self {
        Self {
            db,
            job_service,
            execution_service,
        }
    }
    
    pub async fn schedule_retry(
        &self,
        job_id: Uuid,
        execution_id: Option<Uuid>,
        error_message: Option<String>,
        attempt_number: u32,
        queue_id: Uuid,
    ) -> Result<(), SqlxError> {
        // Dapatkan konfigurasi retry dari queue
        let queue = self.db.get_job_queue_by_id(queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let retry_config: RetryConfig = serde_json::from_value(queue.settings["retry_config"].clone())
            .unwrap_or_default();
        
        // Hitung waktu untuk retry berikutnya
        let delay = retry_config.calculate_delay(attempt_number);
        let scheduled_time = Utc::now() + Duration::from_std(delay).unwrap();
        
        // Buat entri retry history
        let retry_history = RetryHistory::new(
            job_id,
            execution_id,
            attempt_number,
            scheduled_time,
            error_message,
        );
        retry_history.validate().map_err(|e| SqlxError::RowNotFound)?;
        
        self.db.create_retry_history(retry_history).await?;
        
        // Update status job ke Scheduled untuk retry
        self.db.update_job_status(job_id, JobStatus::Scheduled).await?;
        
        Ok(())
    }
    
    pub async fn process_scheduled_retries(&self) -> Result<(), SqlxError> {
        // Ambil semua retry yang waktunya sudah tiba
        let scheduled_retries = self.db.get_scheduled_retries().await?;
        
        for retry in scheduled_retries {
            self.execute_retry(&retry).await?;
        }
        
        Ok(())
    }
    
    async fn execute_retry(&self, retry_history: &RetryHistory) -> Result<(), SqlxError> {
        // Dapatkan job yang akan diretry
        let mut job = self.job_service.get_job_by_id(retry_history.job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Dapatkan konfigurasi queue untuk retry ini
        let queue = self.db.get_job_queue_by_id(job.queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let retry_config: RetryConfig = serde_json::from_value(queue.settings["retry_config"].clone())
            .unwrap_or_default();
        
        // Periksa apakah jumlah percobaan sudah mencapai batas maksimum
        if retry_history.attempt_number > retry_config.max_attempts {
            // Pindahkan job ke dead letter queue atau tandai sebagai gagal definitif
            self.handle_max_retries_exceeded(&job, retry_history).await?;
            return Ok(());
        }
        
        // Reset status job untuk percobaan baru
        job.status = JobStatus::Pending;
        job.attempt_count = retry_history.attempt_number as i32;
        job.updated_at = Utc::now();
        
        self.db.update_job(job).await?;
        
        // Perbarui status retry history
        let mut updated_retry = retry_history.clone();
        updated_retry.mark_as_executed();
        self.db.update_retry_history(updated_retry).await?;
        
        Ok(())
    }
    
    async fn handle_max_retries_exceeded(
        &self,
        job: &Job,
        retry_history: &RetryHistory,
    ) -> Result<(), SqlxError> {
        // Dapatkan konfigurasi queue
        let queue = self.db.get_job_queue_by_id(job.queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let queue_settings: crate::models::queue_settings::QueueSettings = 
            serde_json::from_value(queue.settings).unwrap_or_default();
        
        if let Some(dlq_id) = queue_settings.dead_letter_queue_id {
            // Pindahkan job ke dead letter queue
            let mut job_in_dlq = job.clone();
            job_in_dlq.queue_id = dlq_id;
            job_in_dlq.status = JobStatus::Pending;
            job_in_dlq.updated_at = Utc::now();
            
            self.db.update_job(job_in_dlq).await?;
        } else {
            // Tandai job sebagai gagal definitif
            self.job_service
                .update_job_status(job.id, JobStatus::Failed)
                .await?;
        }
        
        // Perbarui status retry history
        let mut updated_retry = retry_history.clone();
        updated_retry.mark_as_failed();
        self.db.update_retry_history(updated_retry).await?;
        
        Ok(())
    }
    
    pub async fn get_retry_history(
        &self,
        job_id: Uuid,
    ) -> Result<Vec<RetryHistory>, SqlxError> {
        self.db.get_retry_history_by_job(job_id).await
    }
    
    pub async fn get_retry_stats(
        &self,
        job_id: Uuid,
    ) -> Result<RetryStats, SqlxError> {
        let retries = self.get_retry_history(job_id).await?;
        
        let mut stats = RetryStats {
            total_retries: 0,
            successful_retries: 0,
            failed_retries: 0,
            cancelled_retries: 0,
        };
        
        for retry in retries {
            stats.total_retries += 1;
            
            match retry.status {
                RetryStatus::Completed => stats.successful_retries += 1,
                RetryStatus::Failed => stats.failed_retries += 1,
                RetryStatus::Cancelled => stats.cancelled_retries += 1,
                _ => {}
            }
        }
        
        Ok(stats)
    }
    
    pub async fn cancel_pending_retries(
        &self,
        job_id: Uuid,
    ) -> Result<(), SqlxError> {
        let pending_retries = self.db.get_pending_retries(job_id).await?;
        
        for mut retry in pending_retries {
            retry.mark_as_cancelled();
            self.db.update_retry_history(retry).await?;
        }
        
        Ok(())
    }
}

pub struct RetryStats {
    pub total_retries: u32,
    pub successful_retries: u32,
    pub failed_retries: u32,
    pub cancelled_retries: u32,
}
```

### 4.2. Algoritma Backoff yang Berbeda

```rust
// File: src/services/backoff_strategies.rs
use std::time::Duration;

pub trait BackoffStrategy: Send + Sync {
    fn calculate_delay(&self, attempt_number: u32) -> Duration;
}

pub struct FixedDelayBackoff {
    delay: Duration,
}

impl FixedDelayBackoff {
    pub fn new(delay_seconds: u64) -> Self {
        Self {
            delay: Duration::from_secs(delay_seconds),
        }
    }
}

impl BackoffStrategy for FixedDelayBackoff {
    fn calculate_delay(&self, _attempt_number: u32) -> Duration {
        self.delay
    }
}

pub struct ExponentialBackoff {
    initial_delay: Duration,
    max_delay: Duration,
    multiplier: f64,
    enable_jitter: bool,
}

impl ExponentialBackoff {
    pub fn new(
        initial_delay_seconds: u64,
        max_delay_seconds: u64,
        multiplier: f64,
        enable_jitter: bool,
    ) -> Self {
        Self {
            initial_delay: Duration::from_secs(initial_delay_seconds),
            max_delay: Duration::from_secs(max_delay_seconds),
            multiplier,
            enable_jitter,
        }
    }
}

impl BackoffStrategy for ExponentialBackoff {
    fn calculate_delay(&self, attempt_number: u32) -> Duration {
        if attempt_number == 1 {
            return self.initial_delay;
        }
        
        let base_delay = self.initial_delay.as_secs_f64();
        let multiplier = self.multiplier.powi((attempt_number - 1) as i32);
        let mut delay = base_delay * multiplier;
        
        // Batasi delay maksimum
        delay = delay.min(self.max_delay.as_secs_f64());
        
        // Tambahkan jitter jika diaktifkan
        let delay = if self.enable_jitter {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let jitter_range = delay * 0.1; // 10% jitter
            let jitter = rng.gen_range(-jitter_range..jitter_range);
            (delay + jitter).max(0.0) // Pastikan delay tidak negatif
        } else {
            delay
        };
        
        Duration::from_secs_f64(delay)
    }
}

pub struct LinearBackoff {
    initial_delay: Duration,
    increment: Duration,
    max_delay: Duration,
}

impl LinearBackoff {
    pub fn new(
        initial_delay_seconds: u64,
        increment_seconds: u64,
        max_delay_seconds: u64,
    ) -> Self {
        Self {
            initial_delay: Duration::from_secs(initial_delay_seconds),
            increment: Duration::from_secs(increment_seconds),
            max_delay: Duration::from_secs(max_delay_seconds),
        }
    }
}

impl BackoffStrategy for LinearBackoff {
    fn calculate_delay(&self, attempt_number: u32) -> Duration {
        if attempt_number == 1 {
            return self.initial_delay;
        }
        
        let delay = self.initial_delay.as_secs_f64() + 
                   (self.increment.as_secs_f64() * (attempt_number as f64 - 1.0));
        
        let clamped_delay = delay.min(self.max_delay.as_secs_f64()).max(0.0);
        Duration::from_secs_f64(clamped_delay)
    }
}

pub struct FibonacciBackoff {
    delays: Vec<Duration>,
    max_delay: Duration,
}

impl FibonacciBackoff {
    pub fn new(max_delay_seconds: u64) -> Self {
        let max_delay = Duration::from_secs(max_delay_seconds);
        Self {
            delays: vec![],
            max_delay,
        }
    }
    
    fn calculate_fibonacci_delay(&self, n: u32) -> Duration {
        if n <= 2 {
            return Duration::from_secs(1);
        }
        
        let mut a = 1u64;
        let mut b = 1u64;
        
        for _ in 3..=n {
            let next = a + b;
            if next > self.max_delay.as_secs() {
                return self.max_delay;
            }
            let temp = a;
            a = b;
            b = temp + b;
        }
        
        Duration::from_secs(b.min(self.max_delay.as_secs()))
    }
}

impl BackoffStrategy for FibonacciBackoff {
    fn calculate_delay(&self, attempt_number: u32) -> Duration {
        self.calculate_fibonacci_delay(attempt_number)
    }
}
```

### 4.3. Integrasi dengan Sistem Eksekusi

```rust
// File: src/services/intelligent_retry_service.rs
use crate::{
    database::Database,
    models::{Job, JobStatus, RetryConfig, RetryHistory},
    services::{
        job_service::JobService,
        execution_service::ExecutionService,
        backoff_strategies::{BackoffStrategy, ExponentialBackoff},
    },
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use std::sync::Arc;

pub struct IntelligentRetryService {
    db: Database,
    job_service: JobService,
    execution_service: ExecutionService,
    backoff_strategy: Arc<dyn BackoffStrategy>,
}

impl IntelligentRetryService {
    pub fn new(
        db: Database,
        job_service: JobService,
        execution_service: ExecutionService,
        backoff_strategy: Arc<dyn BackoffStrategy>,
    ) -> Self {
        Self {
            db,
            job_service,
            execution_service,
            backoff_strategy,
        }
    }
    
    pub async fn handle_job_failure(
        &self,
        job_id: Uuid,
        execution_id: Uuid,
        error_type: &str,
        error_message: &str,
    ) -> Result<(), SqlxError> {
        let job = self.job_service.get_job_by_id(job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Dapatkan konfigurasi retry dari queue
        let queue = self.db.get_job_queue_by_id(job.queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let retry_config: RetryConfig = serde_json::from_value(queue.settings["retry_config"].clone())
            .unwrap_or_default();
        
        // Periksa apakah error ini bisa diretry
        if !retry_config.retry_on_errors.contains(&error_type.to_lowercase()) {
            // Jika error tidak bisa diretry, tandai job sebagai gagal definitif
            self.job_service
                .update_job_status(job_id, JobStatus::Failed)
                .await?;
            
            return Ok(());
        }
        
        // Periksa apakah jumlah percobaan sudah mencapai batas maksimum
        if job.attempt_count >= retry_config.max_attempts as i32 {
            // Pindahkan ke dead letter queue atau tandai gagal definitif
            self.handle_max_retries_exceeded(&job).await?;
            return Ok(());
        }
        
        // Jadwalkan retry
        let next_attempt = (job.attempt_count + 1) as u32;
        let delay = self.backoff_strategy.calculate_delay(next_attempt);
        
        // Hitung waktu untuk retry berikutnya
        let scheduled_time = chrono::Utc::now() + 
            chrono::Duration::from_std(delay).unwrap();
        
        // Buat entri retry history
        let retry_history = RetryHistory::new(
            job_id,
            Some(execution_id),
            next_attempt,
            scheduled_time,
            Some(error_message.to_string()),
        );
        
        self.db.create_retry_history(retry_history).await?;
        
        // Update status job ke Scheduled untuk retry
        self.db.update_job_status(job_id, JobStatus::Scheduled).await?;
        
        Ok(())
    }
    
    async fn handle_max_retries_exceeded(&self, job: &Job) -> Result<(), SqlxError> {
        // Dapatkan konfigurasi queue
        let queue = self.db.get_job_queue_by_id(job.queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let queue_settings: crate::models::queue_settings::QueueSettings = 
            serde_json::from_value(queue.settings).unwrap_or_default();
        
        if let Some(dlq_id) = queue_settings.dead_letter_queue_id {
            // Pindahkan job ke dead letter queue
            let mut job_in_dlq = job.clone();
            job_in_dlq.queue_id = dlq_id;
            job_in_dlq.status = JobStatus::Pending;
            job_in_dlq.updated_at = chrono::Utc::now();
            
            self.db.update_job(job_in_dlq).await?;
        } else {
            // Tandai job sebagai gagal definitif
            self.job_service
                .update_job_status(job.id, JobStatus::Failed)
                .await?;
        }
        
        Ok(())
    }
    
    pub async fn get_retry_recommendation(
        &self,
        job_id: Uuid,
        error_type: &str,
    ) -> Result<RetryRecommendation, SqlxError> {
        let job = self.job_service.get_job_by_id(job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let queue = self.db.get_job_queue_by_id(job.queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let retry_config: RetryConfig = serde_json::from_value(queue.settings["retry_config"].clone())
            .unwrap_or_default();
        
        let should_retry = retry_config.should_retry(job.attempt_count as u32, error_type);
        let max_attempts_reached = job.attempt_count >= retry_config.max_attempts as i32;
        
        let next_delay = if should_retry && !max_attempts_reached {
            let next_attempt = (job.attempt_count + 1) as u32;
            Some(self.backoff_strategy.calculate_delay(next_attempt))
        } else {
            None
        };
        
        Ok(RetryRecommendation {
            should_retry,
            max_attempts_reached,
            next_attempt: (job.attempt_count + 1) as u32,
            next_delay,
        })
    }
}

pub struct RetryRecommendation {
    pub should_retry: bool,
    pub max_attempts_reached: bool,
    pub next_attempt: u32,
    pub next_delay: Option<std::time::Duration>,
}
```

## 5. Sistem Dead Letter Queue

### 5.1. Layanan Dead Letter Queue

```rust
// File: src/services/dead_letter_service.rs
use crate::{
    database::Database,
    models::{Job, JobStatus, JobQueue},
    services::job_service::JobService,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;

pub struct DeadLetterService {
    db: Database,
    job_service: JobService,
}

impl DeadLetterService {
    pub fn new(db: Database, job_service: JobService) -> Self {
        Self { db, job_service }
    }
    
    pub async fn move_to_dead_letter_queue(
        &self,
        job_id: Uuid,
        reason: &str,
    ) -> Result<(), SqlxError> {
        let mut job = self.job_service.get_job_by_id(job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Dapatkan queue asal job
        let source_queue = self.db.get_job_queue_by_id(job.queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Dapatkan dead letter queue dari konfigurasi
        let queue_settings: crate::models::queue_settings::QueueSettings = 
            serde_json::from_value(source_queue.settings).unwrap_or_default();
        
        if let Some(dlq_id) = queue_settings.dead_letter_queue_id {
            // Pindahkan job ke dead letter queue
            job.queue_id = dlq_id;
            job.status = JobStatus::Pending; // Reset status
            job.updated_at = chrono::Utc::now();
            
            // Tambahkan metadata tentang alasan pemindahan
            let mut payload = job.payload.clone();
            if let serde_json::Value::Object(ref mut obj) = payload {
                obj.insert("dlq_reason".to_string(), serde_json::Value::String(reason.to_string()));
                obj.insert("moved_from_queue".to_string(), serde_json::Value::String(source_queue.id.to_string()));
                obj.insert("moved_at".to_string(), serde_json::Value::String(chrono::Utc::now().to_rfc3339()));
            }
            
            job.payload = payload;
            
            self.db.update_job(job).await?;
        } else {
            // Jika tidak ada DLQ yang ditentukan, hanya tandai sebagai gagal definitif
            self.job_service
                .update_job_status(job_id, JobStatus::Failed)
                .await?;
        }
        
        Ok(())
    }
    
    pub async fn get_dead_letter_jobs(
        &self,
        dlq_id: Uuid,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<Job>, SqlxError> {
        self.db.get_jobs_by_queue_and_status(
            dlq_id,
            JobStatus::Pending,
            limit,
            offset,
        ).await
    }
    
    pub async fn retry_from_dead_letter(
        &self,
        job_id: Uuid,
        target_queue_id: Uuid,
    ) -> Result<(), SqlxError> {
        let mut job = self.job_service.get_job_by_id(job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pindahkan job kembali ke queue target
        job.queue_id = target_queue_id;
        job.status = JobStatus::Pending;
        job.attempt_count = 0; // Reset jumlah percobaan
        job.updated_at = chrono::Utc::now();
        
        // Hapus metadata DLQ dari payload
        let mut payload = job.payload.clone();
        if let serde_json::Value::Object(ref mut obj) = payload {
            obj.remove("dlq_reason");
            obj.remove("moved_from_queue");
            obj.remove("moved_at");
        }
        
        job.payload = payload;
        
        self.db.update_job(job).await?;
        
        Ok(())
    }
    
    pub async fn delete_from_dead_letter(
        &self,
        job_id: Uuid,
    ) -> Result<(), SqlxError> {
        // Dalam implementasi nyata, mungkin lebih aman untuk menandai job sebagai dihapus
        // daripada benar-benar menghapusnya
        self.job_service
            .update_job_status(job_id, JobStatus::Cancelled)
            .await?;
        
        Ok(())
    }
}
```

## 6. Monitoring dan Observasi Retry

### 6.1. Metrik Retry

```rust
// File: src/services/retry_monitoring_service.rs
use crate::database::Database;
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};

pub struct RetryMonitoringService {
    db: Database,
}

impl RetryMonitoringService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn get_retry_metrics(
        &self,
        queue_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<RetryMetrics, SqlxError> {
        let total_retries = self.db.get_retry_count_in_period(queue_id, start_time, end_time).await?;
        let successful_retries = self.db.get_successful_retry_count_in_period(queue_id, start_time, end_time).await?;
        let failed_retries = self.db.get_failed_retry_count_in_period(queue_id, start_time, end_time).await?;
        
        let retry_rate = if total_retries > 0 {
            (successful_retries as f64 / total_retries as f64) * 100.0
        } else {
            100.0
        };
        
        let avg_retry_delay = self.db.get_avg_retry_delay_in_period(queue_id, start_time, end_time).await?;
        
        Ok(RetryMetrics {
            queue_id,
            total_retries: total_retries as u64,
            successful_retries: successful_retries as u64,
            failed_retries: failed_retries as u64,
            retry_rate,
            avg_retry_delay,
            period_start: start_time,
            period_end: end_time,
        })
    }
    
    pub async fn get_retry_patterns(
        &self,
        queue_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<RetryPatterns, SqlxError> {
        let retry_counts_by_attempt = self.db.get_retry_counts_by_attempt_in_period(
            queue_id, start_time, end_time
        ).await?;
        
        let retry_outcomes_by_attempt = self.db.get_retry_outcomes_by_attempt_in_period(
            queue_id, start_time, end_time
        ).await?;
        
        Ok(RetryPatterns {
            queue_id,
            retry_counts_by_attempt,
            retry_outcomes_by_attempt,
            period_start: start_time,
            period_end: end_time,
        })
    }
}

pub struct RetryMetrics {
    pub queue_id: Uuid,
    pub total_retries: u64,
    pub successful_retries: u64,
    pub failed_retries: u64,
    pub retry_rate: f64,  // dalam persen
    pub avg_retry_delay: f64,  // dalam detik
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

pub struct RetryPatterns {
    pub queue_id: Uuid,
    pub retry_counts_by_attempt: std::collections::HashMap<u32, i64>,
    pub retry_outcomes_by_attempt: std::collections::HashMap<u32, RetryOutcomeCount>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

pub struct RetryOutcomeCount {
    pub succeeded: i64,
    pub failed: i64,
    pub cancelled: i64,
}
```

## 7. Best Practices dan Rekomendasi

### 7.1. Praktik Terbaik untuk Retry

1. **Gunakan exponential backoff** - untuk mencegah pengulangan yang berlebihan
2. **Terapkan jitter** - untuk mencegah thundering herd effect
3. **Batasi jumlah retry** - untuk mencegah sumber daya terbuang pada job yang tidak bisa diselesaikan
4. **Gunakan dead letter queue** - untuk menangani job yang tidak bisa diproses
5. **Log semua retry** - untuk keperluan debugging dan analisis

### 7.2. Konfigurasi Retry yang Disarankan

- **Max Attempts**: 3-5 kali untuk kebanyakan job
- **Initial Delay**: 1-5 detik tergantung jenis job
- **Max Delay**: 5-10 menit untuk mencegah penundaan yang terlalu lama
- **Backoff Multiplier**: 2.0 untuk exponential backoff
- **Jitter**: 10-25% untuk mencegah pola yang terlalu terduga

### 7.3. Skala dan Kinerja

1. **Gunakan indeks yang tepat** - pada tabel retry history untuk query yang cepat
2. **Optimalkan query database** - terutama untuk operasi penjadwalan retry
3. **Gunakan caching strategi** - untuk konfigurasi retry yang sering diakses
4. **Gunakan partisi tabel** - untuk mengelola data retry history dengan efisien
5. **Gunakan connection pooling** - untuk efisiensi koneksi database