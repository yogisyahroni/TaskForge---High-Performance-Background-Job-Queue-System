# Strategi Testing (Unit, Integration, dan E2E) untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan strategi dan implementasi testing komprehensif untuk aplikasi TaskForge. Sistem testing mencakup tiga tingkatan utama: unit testing, integration testing, dan end-to-end (E2E) testing, untuk memastikan kualitas, keandalan, dan kinerja aplikasi.

## 2. Arsitektur Testing

### 2.1. Pendekatan Testing Pyramid

TaskForge menerapkan pendekatan testing pyramid:

```
┌─────────────────────────┐
│     E2E Testing         │  ← Jumlah sedikit, cakupan luas
├─────────────────────────┤
│   Integration Testing   │  ← Jumlah sedang, cakupan medium
├─────────────────────────┤
│      Unit Testing       │  ← Jumlah banyak, cakupan sempit
└─────────────────────────┘
```

### 2.2. Tools dan Framework Testing

- **Unit Testing**: `tokio::test`, `serial_test` untuk testing async
- **Integration Testing**: `sqlx` dengan database test, `axum` test utilities
- **E2E Testing**: `reqwest` untuk HTTP testing, `testcontainers` untuk dependencies
- **Mocking**: `mockall`, `wiremock` untuk mocking external dependencies
- **Property Testing**: `proptest` untuk testing edge cases
- **Benchmarking**: `criterion` untuk testing kinerja

## 3. Unit Testing

### 3.1. Struktur Unit Testing

```rust
// File: tests/unit/job_service_tests.rs
use taskforge::{
    models::job::{Job, JobStatus},
    services::job_service::JobService,
};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serial_test::serial;

#[tokio::test]
#[serial]
async fn test_job_creation() {
    let job_type = "test_job".to_string();
    let payload = serde_json::json!({"test": "data"});
    
    let job = Job::new(
        Uuid::new_v4(), // queue_id
        job_type.clone(),
        payload.clone(),
        5, // priority
        Some(3), // max_attempts
    );
    
    assert_eq!(job.job_type, "test_job");
    assert_eq!(job.priority, 5);
    assert_eq!(job.max_attempts, 3);
    assert_eq!(job.status, JobStatus::Pending);
    assert!(job.created_at <= Utc::now());
    assert_eq!(job.attempt_count, 0);
}

#[tokio::test]
#[serial]
async fn test_job_validation_success() {
    let job = Job::new(
        Uuid::new_v4(),
        "valid_job_type".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    assert!(job.validate().is_ok());
}

#[tokio::test]
#[serial]
async fn test_job_validation_invalid_priority() {
    let job = Job::new(
        Uuid::new_v4(),
        "valid_job_type".to_string(),
        serde_json::json!({}),
        15, // Invalid priority (> 9)
        Some(3),
    );
    
    assert!(job.validate().is_err());
}

#[tokio::test]
#[serial]
async fn test_job_validation_empty_type() {
    let job = Job::new(
        Uuid::new_v4(),
        "".to_string(), // Empty job type
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    assert!(job.validate().is_err());
}

#[tokio::test]
#[serial]
async fn test_job_ready_for_execution_pending() {
    let job = Job::new(
        Uuid::new_v4(),
        "test_job".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    assert!(job.can_be_executed());
}

#[tokio::test]
#[serial]
async fn test_job_ready_for_execution_scheduled_future() {
    let mut job = Job::new(
        Uuid::new_v4(),
        "test_job".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    job.status = JobStatus::Scheduled;
    job.scheduled_for = Some(Utc::now() + chrono::Duration::hours(1)); // Scheduled in the future
    
    assert!(!job.can_be_executed());
}

#[tokio::test]
#[serial]
async fn test_job_ready_for_execution_scheduled_past() {
    let mut job = Job::new(
        Uuid::new_v4(),
        "test_job".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    job.status = JobStatus::Scheduled;
    job.scheduled_for = Some(Utc::now() - chrono::Duration::hours(1)); // Scheduled in the past
    
    assert!(job.can_be_executed());
}

#[tokio::test]
#[serial]
async fn test_job_can_be_retried_success() {
    let mut job = Job::new(
        Uuid::new_v4(),
        "test_job".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    job.status = JobStatus::Failed;
    job.attempt_count = 1; // Still within max attempts
    
    assert!(job.can_be_retried());
}

#[tokio::test]
#[serial]
async fn test_job_can_be_retried_max_attempts_reached() {
    let mut job = Job::new(
        Uuid::new_v4(),
        "test_job".to_string(),
        serde_json::json!({}),
        5,
        Some(2), // Max attempts = 2
    );
    
    job.status = JobStatus::Failed;
    job.attempt_count = 2; // Reached max attempts
    
    assert!(!job.can_be_retried());
}

#[tokio::test]
#[serial]
async fn test_job_mark_as_processing() {
    let mut job = Job::new(
        Uuid::new_v4(),
        "test_job".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    job.mark_as_processing();
    
    assert_eq!(job.status, JobStatus::Processing);
    assert_eq!(job.attempt_count, 1);
}

#[tokio::test]
#[serial]
async fn test_job_mark_as_succeeded() {
    let mut job = Job::new(
        Uuid::new_v4(),
        "test_job".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    job.mark_as_succeeded(Some("Success output".to_string()));
    
    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(job.output, Some("Success output".to_string()));
    assert!(job.completed_at.is_some());
}

#[tokio::test]
#[serial]
async fn test_job_mark_as_failed() {
    let mut job = Job::new(
        Uuid::new_v4(),
        "test_job".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    job.mark_as_failed(Some("Error message".to_string()));
    
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.error_message, Some("Error message".to_string()));
    assert!(job.completed_at.is_some());
}

#[tokio::test]
#[serial]
async fn test_job_has_expired_false() {
    let job = Job::new(
        Uuid::new_v4(),
        "test_job".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    assert!(!job.has_expired());
}

#[tokio::test]
#[serial]
async fn test_job_has_expired_true() {
    let mut job = Job::new(
        Uuid::new_v4(),
        "test_job".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    job.timeout_at = Some(Utc::now() - chrono::Duration::minutes(1)); // Already expired
    
    assert!(job.has_expired());
}
```

### 3.2. Testing untuk Layanan (Services)

```rust
// File: tests/unit/service_tests.rs
use taskforge::services::retry_service::{RetryConfig, RetryService};
use std::time::Duration;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn test_retry_config_new() {
    let config = RetryConfig::new(5, 1, 300, 2.0);
    
    assert_eq!(config.max_attempts, 5);
    assert_eq!(config.initial_delay_seconds, 1);
    assert_eq!(config.max_delay_seconds, 300);
    assert_eq!(config.backoff_multiplier, 2.0);
}

#[tokio::test]
#[serial]
async fn test_retry_config_calculate_delay_first_attempt() {
    let config = RetryConfig::new(5, 2, 60, 2.0);
    
    let delay = config.calculate_delay(1);
    assert_eq!(delay, Duration::from_secs(2)); // Initial delay
}

#[tokio::test]
#[serial]
async fn test_retry_config_calculate_delay_second_attempt() {
    let config = RetryConfig::new(5, 1, 60, 2.0);
    
    let delay = config.calculate_delay(2);
    assert_eq!(delay, Duration::from_secs(2)); // 1 * 2^1
}

#[tokio::test]
#[serial]
async fn test_retry_config_calculate_delay_third_attempt() {
    let config = RetryConfig::new(5, 1, 60, 2.0);
    
    let delay = config.calculate_delay(3);
    assert_eq!(delay, Duration::from_secs(4)); // 1 * 2^2
}

#[tokio::test]
#[serial]
async fn test_retry_config_calculate_delay_with_max_limit() {
    let config = RetryConfig::new(5, 1, 10, 2.0); // Max delay is 10 seconds
    
    let delay = config.calculate_delay(5); // Would be 1 * 2^4 = 16 seconds without limit
    assert_eq!(delay, Duration::from_secs(10)); // Limited to max
}

#[tokio::test]
#[serial]
async fn test_retry_config_validate_success() {
    let config = RetryConfig::new(3, 1, 60, 2.0);
    assert!(config.validate().is_ok());
}

#[tokio::test]
#[serial]
async fn test_retry_config_validate_zero_max_attempts() {
    let config = RetryConfig {
        max_attempts: 0,
        initial_delay_seconds: 1,
        max_delay_seconds: 60,
        backoff_multiplier: 2.0,
        retry_on_status_codes: vec![500, 502, 503, 504],
        retry_on_errors: vec!["timeout".to_string(), "connection_error".to_string()],
        enable_jitter: true,
        jitter_factor: 0.1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    
    assert!(config.validate().is_err());
}

#[tokio::test]
#[serial]
async fn test_retry_config_validate_invalid_multiplier() {
    let config = RetryConfig {
        max_attempts: 3,
        initial_delay_seconds: 1,
        max_delay_seconds: 60,
        backoff_multiplier: 0.5, // Less than 1.0
        retry_on_status_codes: vec![500, 502, 503, 504],
        retry_on_errors: vec!["timeout".to_string(), "connection_error".to_string()],
        enable_jitter: true,
        jitter_factor: 0.1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    
    assert!(config.validate().is_err());
}

#[tokio::test]
#[serial]
async fn test_retry_config_should_retry_true() {
    let config = RetryConfig::new(3, 1, 60, 2.0);
    
    // Should retry for known error types
    assert!(config.should_retry(1, "timeout"));
    assert!(config.should_retry(1, "connection_error"));
    assert!(config.should_retry(1, "temporary_failure"));
}

#[tokio::test]
#[serial]
async fn test_retry_config_should_retry_false_unknown_error() {
    let config = RetryConfig::new(3, 1, 60, 2.0);
    
    // Should not retry for unknown error types
    assert!(!config.should_retry(1, "unknown_error"));
}

#[tokio::test]
#[serial]
async fn test_retry_config_should_retry_false_max_attempts_reached() {
    let config = RetryConfig::new(2, 1, 60, 2.0);
    
    // Should not retry after max attempts
    assert!(!config.should_retry(3, "timeout")); // More than max attempts (2)
}

#[tokio::test]
#[serial]
async fn test_retry_config_validate_jitter_factor() {
    let config = RetryConfig {
        max_attempts: 3,
        initial_delay_seconds: 1,
        max_delay_seconds: 60,
        backoff_multiplier: 2.0,
        retry_on_status_codes: vec![500, 502, 503, 504],
        retry_on_errors: vec!["timeout".to_string()],
        enable_jitter: true,
        jitter_factor: 1.5, // Invalid: > 1.0
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    
    assert!(config.validate().is_err());
}
```

## 4. Integration Testing

### 4.1. Testing Database Integration

```rust
// File: tests/integration/database_tests.rs
use taskforge::{
    database::Database,
    models::{Job, JobQueue, Organization},
    services::{job_service::JobService, queue_service::QueueService},
};
use sqlx::{PgPool, Row};
use uuid::Uuid;
use serial_test::serial;
use chrono::{DateTime, Utc};

// Setup function untuk testing database
async fn setup_test_db() -> PgPool {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:password@localhost/test_taskforge".to_string());
    
    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database");
    
    // Clean up any existing test data
    sqlx::query("DELETE FROM jobs WHERE created_at > $1")
        .bind(Utc::now() - chrono::Duration::hours(1)) // Hanya hapus data dalam 1 jam terakhir
        .execute(&pool)
        .await
        .unwrap();
    
    sqlx::query("DELETE FROM job_queues WHERE created_at > $1")
        .bind(Utc::now() - chrono::Duration::hours(1))
        .execute(&pool)
        .await
        .unwrap();
    
    pool
}

#[tokio::test]
#[serial]
async fn test_job_crud_operations() {
    let pool = setup_test_db().await;
    let db = Database::new(pool);
    let queue_service = QueueService::new(db.clone());
    let job_service = JobService::new(db.clone(), queue_service);
    
    // Create a test queue
    let queue = JobQueue::new(
        Uuid::new_v4(), // project_id
        "test_integration_queue".to_string(),
        Some("Test queue for integration testing".to_string()),
        5,
        serde_json::json!({}),
    );
    
    let created_queue = queue_service.create_queue(queue).await.unwrap();
    
    // Create a job
    let job = Job::new(
        created_queue.id,
        "integration_test_job".to_string(),
        serde_json::json!({"test": "integration_data"}),
        5,
        Some(3),
    );
    
    let created_job = job_service.create_job(job).await.unwrap();
    
    // Retrieve the job
    let retrieved_job = job_service.get_job_by_id(created_job.id).await.unwrap().unwrap();
    
    assert_eq!(retrieved_job.job_type, "integration_test_job");
    assert_eq!(retrieved_job.queue_id, created_queue.id);
    assert_eq!(retrieved_job.status, JobStatus::Pending);
    
    // Update job status
    let updated_job = job_service.update_job_status(created_job.id, JobStatus::Processing).await.unwrap();
    assert_eq!(updated_job.status, JobStatus::Processing);
    
    // Cleanup
    sqlx::query("DELETE FROM jobs WHERE id = $1")
        .bind(created_job.id)
        .execute(&db.pool())
        .await
        .unwrap();
    
    sqlx::query("DELETE FROM job_queues WHERE id = $1")
        .bind(created_queue.id)
        .execute(&db.pool())
        .await
        .unwrap();
}

#[tokio::test]
#[serial]
async fn test_queue_job_relationship() {
    let pool = setup_test_db().await;
    let db = Database::new(pool);
    let queue_service = QueueService::new(db.clone());
    let job_service = JobService::new(db.clone(), queue_service);
    
    // Create queue
    let queue = JobQueue::new(
        Uuid::new_v4(), // project_id
        "relationship_test_queue".to_string(),
        Some("Queue for relationship testing".to_string()),
        5,
        serde_json::json!({}),
    );
    
    let created_queue = queue_service.create_queue(queue).await.unwrap();
    
    // Create multiple jobs for the same queue
    let job1 = Job::new(
        created_queue.id,
        "job_type_1".to_string(),
        serde_json::json!({"data": 1}),
        5,
        Some(3),
    );
    
    let job2 = Job::new(
        created_queue.id,
        "job_type_2".to_string(),
        serde_json::json!({"data": 2}),
        7,
        Some(2),
    );
    
    let created_job1 = job_service.create_job(job1).await.unwrap();
    let created_job2 = job_service.create_job(job2).await.unwrap();
    
    // Get jobs by queue
    let jobs_in_queue = job_service.get_jobs_by_queue(created_queue.id).await.unwrap();
    
    assert_eq!(jobs_in_queue.len(), 2);
    assert!(jobs_in_queue.iter().any(|j| j.id == created_job1.id));
    assert!(jobs_in_queue.iter().any(|j| j.id == created_job2.id));
    
    // Verify job priorities were preserved
    let job1_in_result = jobs_in_queue.iter().find(|j| j.id == created_job1.id).unwrap();
    let job2_in_result = jobs_in_queue.iter().find(|j| j.id == created_job2.id).unwrap();
    
    assert_eq!(job1_in_result.priority, 5);
    assert_eq!(job2_in_result.priority, 7);
    
    // Cleanup
    sqlx::query("DELETE FROM jobs WHERE queue_id = $1")
        .bind(created_queue.id)
        .execute(&db.pool())
        .await
        .unwrap();
    
    sqlx::query("DELETE FROM job_queues WHERE id = $1")
        .bind(created_queue.id)
        .execute(&db.pool())
        .await
        .unwrap();
}

#[tokio::test]
#[serial]
async fn test_job_statistics() {
    let pool = setup_test_db().await;
    let db = Database::new(pool);
    let queue_service = QueueService::new(db.clone());
    let job_service = JobService::new(db.clone(), queue_service);
    
    // Create queue
    let queue = JobQueue::new(
        Uuid::new_v4(), // project_id
        "stats_test_queue".to_string(),
        Some("Queue for statistics testing".to_string()),
        5,
        serde_json::json!({}),
    );
    
    let created_queue = queue_service.create_queue(queue).await.unwrap();
    
    // Create jobs with different statuses
    let job1 = Job::new(
        created_queue.id,
        "stats_job_1".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    let job2 = Job::new(
        created_queue.id,
        "stats_job_2".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    let job3 = Job::new(
        created_queue.id,
        "stats_job_3".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    let created_job1 = job_service.create_job(job1).await.unwrap();
    let created_job2 = job_service.create_job(job2).await.unwrap();
    let created_job3 = job_service.create_job(job3).await.unwrap();
    
    // Update statuses for different jobs
    job_service.update_job_status(created_job1.id, JobStatus::Succeeded).await.unwrap();
    job_service.update_job_status(created_job2.id, JobStatus::Failed).await.unwrap();
    // job3 remains pending
    
    // Get statistics
    let stats = job_service.get_queue_statistics(created_queue.id).await.unwrap();
    
    assert_eq!(stats.pending_jobs, 1);
    assert_eq!(stats.succeeded_jobs, 1);
    assert_eq!(stats.failed_jobs, 1);
    
    // Cleanup
    sqlx::query("DELETE FROM jobs WHERE queue_id = $1")
        .bind(created_queue.id)
        .execute(&db.pool())
        .await
        .unwrap();
    
    sqlx::query("DELETE FROM job_queues WHERE id = $1")
        .bind(created_queue.id)
        .execute(&db.pool())
        .await
        .unwrap();
}
```

### 4.2. Testing Service Integration

```rust
// File: tests/integration/service_integration_tests.rs
use taskforge::{
    services::{
        job_service::JobService,
        queue_service::QueueService,
        worker_service::WorkerService,
        retry_service::RetryService,
    },
    database::Database,
};
use uuid::Uuid;
use serial_test::serial;

async fn setup_integration_services() -> (JobService, QueueService, WorkerService, RetryService) {
    let pool = setup_test_db().await;
    let db = Database::new(pool);
    
    let queue_service = QueueService::new(db.clone());
    let worker_service = WorkerService::new(db.clone());
    let retry_service = RetryService::new(db.clone());
    let job_service = JobService::new(db, queue_service.clone());
    
    (job_service, queue_service, worker_service, retry_service)
}

#[tokio::test]
#[serial]
async fn test_job_queue_integration() {
    let (job_service, queue_service, _, _) = setup_integration_services().await;
    
    // Create a queue
    let queue = queue_service
        .create_queue(
            Uuid::new_v4(), // project_id
            "integration_test_queue".to_string(),
            Some("Queue for integration testing".to_string()),
            5,
            serde_json::json!({
                "max_concurrent_jobs": 10,
                "max_retries": 3,
                "timeout_seconds": 300
            }),
        )
        .await
        .unwrap();
    
    // Submit a job to the queue
    let job = job_service
        .create_job(Job::new(
            queue.id,
            "integration_test_job".to_string(),
            serde_json::json!({"test": "integration"}),
            5,
            Some(3),
        ))
        .await
        .unwrap();
    
    // Verify job was created and linked to the correct queue
    assert_eq!(job.queue_id, queue.id);
    assert_eq!(job.job_type, "integration_test_job");
    
    // Get jobs by queue to verify relationship
    let jobs_in_queue = job_service
        .get_jobs_by_queue(queue.id)
        .await
        .unwrap();
    
    assert_eq!(jobs_in_queue.len(), 1);
    assert_eq!(jobs_in_queue[0].id, job.id);
    
    // Cleanup
    cleanup_integration_test_resources(&job_service, &queue_service, job.id, queue.id).await;
}

#[tokio::test]
#[serial]
async fn test_retry_mechanism_integration() {
    let (job_service, queue_service, _, retry_service) = setup_integration_services().await;
    
    // Create a queue
    let queue = queue_service
        .create_queue(
            Uuid::new_v4(), // project_id
            "retry_test_queue".to_string(),
            Some("Queue for retry testing".to_string()),
            5,
            serde_json::json!({
                "retry_config": {
                    "max_attempts": 3,
                    "initial_delay_seconds": 1,
                    "max_delay_seconds": 60,
                    "backoff_multiplier": 2.0
                }
            }),
        )
        .await
        .unwrap();
    
    // Create a failing job
    let mut job = Job::new(
        queue.id,
        "failing_test_job".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    job.status = JobStatus::Failed;
    job.attempt_count = 1;
    
    let job = job_service.create_job(job).await.unwrap();
    
    // Process the retry
    retry_service.schedule_retry(job.id, Some("Test failure".to_string())).await.unwrap();
    
    // Verify retry was scheduled
    let scheduled_retries = retry_service.get_scheduled_retries_for_job(job.id).await.unwrap();
    assert_eq!(scheduled_retries.len(), 1);
    
    // Verify job status changed to Scheduled
    let updated_job = job_service.get_job_by_id(job.id).await.unwrap().unwrap();
    assert_eq!(updated_job.status, JobStatus::Scheduled);
    
    // Cleanup
    cleanup_integration_test_resources(&job_service, &queue_service, job.id, queue.id).await;
}

#[tokio::test]
#[serial]
async fn test_worker_assignment_integration() {
    let (job_service, queue_service, worker_service, _) = setup_integration_services().await;
    
    // Create a queue
    let queue = queue_service
        .create_queue(
            Uuid::new_v4(), // project_id
            "worker_assignment_queue".to_string(),
            Some("Queue for worker assignment testing".to_string()),
            5,
            serde_json::json!({}),
        )
        .await
        .unwrap();
    
    // Create a worker
    let worker = worker_service
        .register_worker(
            queue.project_id,
            "test_worker".to_string(),
            crate::models::worker::WorkerType::General,
            5, // max_concurrent_jobs
            serde_json::json!({"capabilities": ["general"]}),
        )
        .await
        .unwrap();
    
    // Create a job
    let job = job_service
        .create_job(Job::new(
            queue.id,
            "assignable_test_job".to_string(),
            serde_json::json!({}),
            5,
            Some(3),
        ))
        .await
        .unwrap();
    
    // Assign job to worker (simulated)
    // In real implementation, this would happen through the scheduler
    let assignment_result = worker_service.assign_job_to_worker(job.id, worker.id).await;
    
    // For this test, we're just verifying the integration between components
    assert!(assignment_result.is_ok());
    
    // Cleanup
    worker_service.deregister_worker(worker.id).await.unwrap();
    cleanup_integration_test_resources(&job_service, &queue_service, job.id, queue.id).await;
}

async fn cleanup_integration_test_resources(
    job_service: &JobService,
    queue_service: &QueueService,
    job_id: Uuid,
    queue_id: Uuid,
) {
    // Cleanup in reverse order to respect foreign key constraints
    sqlx::query("DELETE FROM jobs WHERE id = $1")
        .bind(job_id)
        .execute(&job_service.db.pool())
        .await
        .unwrap();
    
    sqlx::query("DELETE FROM job_queues WHERE id = $1")
        .bind(queue_id)
        .execute(&queue_service.db.pool())
        .await
        .unwrap();
}
```

## 5. End-to-End Testing

### 5.1. Testing Alur Bisnis Lengkap

```rust
// File: tests/e2e/job_flow_tests.rs
use taskforge::{
    services::{
        job_service::JobService,
        queue_service::QueueService,
        worker_service::WorkerService,
        authentication_service::AuthenticationService,
    },
    models::{Job, JobQueue, JobStatus},
};
use uuid::Uuid;
use serial_test::serial;
use reqwest::Client;
use serde_json::Value;

struct TestHarness {
    client: Client,
    base_url: String,
    auth_token: String,
    job_service: JobService,
    queue_service: QueueService,
    worker_service: WorkerService,
}

impl TestHarness {
    async fn new() -> Self {
        let client = Client::new();
        let base_url = std::env::var("TEST_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:8000".to_string());
        
        // In a real implementation, we would authenticate to get a token
        // For this test, we'll use a placeholder
        let auth_token = "test_token"; // This would come from authentication
        
        // Initialize services (in real implementation, these would connect to the running app)
        let pool = setup_test_db().await;
        let db = Database::new(pool);
        
        let queue_service = QueueService::new(db.clone());
        let worker_service = WorkerService::new(db.clone());
        let job_service = JobService::new(db, queue_service.clone());
        
        Self {
            client,
            base_url,
            auth_token: auth_token.to_string(),
            job_service,
            queue_service,
            worker_service,
        }
    }
    
    async fn create_test_queue(&self) -> JobQueue {
        self.queue_service
            .create_queue(
                Uuid::new_v4(), // project_id
                "e2e_test_queue".to_string(),
                Some("Queue for end-to-end testing".to_string()),
                5,
                serde_json::json!({
                    "max_concurrent_jobs": 10,
                    "max_retries": 3,
                    "timeout_seconds": 300,
                    "retry_config": {
                        "max_attempts": 3,
                        "initial_delay_seconds": 1,
                        "max_delay_seconds": 60,
                        "backoff_multiplier": 2.0
                    }
                }),
            )
            .await
            .unwrap()
    }
    
    async fn submit_job_via_api(&self, queue_name: &str, job_type: &str, payload: Value) -> reqwest::Response {
        let url = format!("{}/api/v1/jobs", self.base_url);
        
        let request_body = serde_json::json!({
            "queue_name": queue_name,
            "job_type": job_type,
            "payload": payload,
            "priority": 5
        });
        
        self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .unwrap()
    }
    
    async fn get_job_via_api(&self, job_id: Uuid) -> reqwest::Response {
        let url = format!("{}/api/v1/jobs/{}", self.base_url, job_id);
        
        self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.auth_token))
            .send()
            .await
            .unwrap()
    }
}

#[tokio::test]
#[serial]
async fn test_complete_job_flow_e2e() {
    let harness = TestHarness::new().await;
    let queue = harness.create_test_queue().await;
    
    // Submit a job via API
    let response = harness
        .submit_job_via_api(
            &queue.name,
            "e2e_test_job",
            serde_json::json!({"test": "e2e", "value": 42}),
        )
        .await;
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    let response_json: Value = response.json().await.unwrap();
    assert!(response_json["success"].as_bool().unwrap());
    
    let job_id_str = response_json["data"]["id"].as_str().unwrap();
    let job_id = Uuid::parse_str(job_id_str).unwrap();
    
    // Verify job was created in the database
    let job = harness.job_service.get_job_by_id(job_id).await.unwrap().unwrap();
    assert_eq!(job.job_type, "e2e_test_job");
    assert_eq!(job.status, JobStatus::Pending);
    assert_eq!(job.queue_id, queue.id);
    
    // Get job details via API
    let job_response = harness.get_job_via_api(job_id).await;
    assert_eq!(job_response.status(), reqwest::StatusCode::OK);
    
    let job_response_json: Value = job_response.json().await.unwrap();
    assert!(job_response_json["success"].as_bool().unwrap());
    assert_eq!(job_response_json["data"]["id"], job_id_str);
    assert_eq!(job_response_json["data"]["status"], "pending");
    
    // Simulate job processing (in real world, this would be handled by a worker)
    let updated_job = harness.job_service
        .update_job_status(job_id, JobStatus::Processing)
        .await
        .unwrap();
    assert_eq!(updated_job.status, JobStatus::Processing);
    
    // Simulate job completion
    let mut completed_job = updated_job;
    completed_job.mark_as_succeeded(Some("Job completed successfully".to_string()));
    let completed_job = harness.job_service.update_job(completed_job).await.unwrap();
    
    assert_eq!(completed_job.status, JobStatus::Succeeded);
    
    // Verify final status via API
    let final_response = harness.get_job_via_api(job_id).await;
    let final_json: Value = final_response.json().await.unwrap();
    
    assert_eq!(final_json["data"]["status"], "succeeded");
    assert_eq!(final_json["data"]["output"], "Job completed successfully");
    
    // Cleanup
    cleanup_e2e_test_resources(&harness.job_service, &harness.queue_service, job_id, queue.id).await;
}

#[tokio::test]
#[serial]
async fn test_job_retry_flow_e2e() {
    let harness = TestHarness::new().await;
    let queue = harness.create_test_queue().await;
    
    // Submit a job that will eventually succeed after retries
    let response = harness
        .submit_job_via_api(
            &queue.name,
            "retry_test_job",
            serde_json::json!({"will_succeed_on_retry": true}),
        )
        .await;
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    let response_json: Value = response.json().await.unwrap();
    assert!(response_json["success"].as_bool().unwrap());
    
    let job_id_str = response_json["data"]["id"].as_str().unwrap();
    let job_id = Uuid::parse_str(job_id_str).unwrap();
    
    // Simulate first failure
    let job = harness.job_service.get_job_by_id(job_id).await.unwrap().unwrap();
    assert_eq!(job.attempt_count, 0);
    
    let mut failed_job = job;
    failed_job.mark_as_failed(Some("Temporary failure".to_string()));
    let updated_job = harness.job_service.update_job(failed_job).await.unwrap();
    assert_eq!(updated_job.status, JobStatus::Failed);
    
    // In a real system, the retry scheduler would pick this up
    // For this test, we'll manually trigger the retry process
    // This simulates what would happen in the retry service
    let mut retry_job = harness.job_service.get_job_by_id(job_id).await.unwrap().unwrap();
    if retry_job.can_be_retried() {
        retry_job.status = JobStatus::Pending;
        retry_job.attempt_count += 1;
        let retried_job = harness.job_service.update_job(retry_job).await.unwrap();
        
        assert_eq!(retried_job.status, JobStatus::Pending);
        assert_eq!(retried_job.attempt_count, 1);
    }
    
    // Now simulate successful completion on retry
    let mut job_for_success = harness.job_service.get_job_by_id(job_id).await.unwrap().unwrap();
    job_for_success.mark_as_processing();
    job_for_success.mark_as_succeeded(Some("Succeeded on retry".to_string()));
    let final_job = harness.job_service.update_job(job_for_success).await.unwrap();
    
    assert_eq!(final_job.status, JobStatus::Succeeded);
    assert_eq!(final_job.attempt_count, 1); // One retry was needed
    
    // Cleanup
    cleanup_e2e_test_resources(&harness.job_service, &harness.queue_service, job_id, queue.id).await;
}

#[tokio::test]
#[serial]
async fn test_scheduled_job_flow_e2e() {
    let harness = TestHarness::new().await;
    let queue = harness.create_test_queue().await;
    
    // Submit a scheduled job
    let future_time = chrono::Utc::now() + chrono::Duration::seconds(10); // 10 seconds in the future
    
    let response = harness.client
        .post(&format!("{}/api/v1/jobs/scheduled", harness.base_url))
        .header("Authorization", format!("Bearer {}", harness.auth_token))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "queue_name": queue.name,
            "job_type": "scheduled_test_job",
            "payload": {"scheduled": true, "time": future_time.to_rfc3339()},
            "scheduled_for": future_time.to_rfc3339(),
            "priority": 5
        }))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    
    let response_json: Value = response.json().await.unwrap();
    assert!(response_json["success"].as_bool().unwrap());
    
    let job_id_str = response_json["data"]["id"].as_str().unwrap();
    let job_id = Uuid::parse_str(job_id_str).unwrap();
    
    // Verify job was created with scheduled status
    let job = harness.job_service.get_job_by_id(job_id).await.unwrap().unwrap();
    assert_eq!(job.status, JobStatus::Scheduled);
    assert!(job.scheduled_for.is_some());
    
    // Verify scheduled time is in the future
    let scheduled_time = job.scheduled_for.unwrap();
    assert!(scheduled_time > chrono::Utc::now());
    
    // Cleanup
    cleanup_e2e_test_resources(&harness.job_service, &harness.queue_service, job_id, queue.id).await;
}

async fn cleanup_e2e_test_resources(
    job_service: &JobService,
    queue_service: &QueueService,
    job_id: Uuid,
    queue_id: Uuid,
) {
    // In E2E tests, we might want to clean up differently
    // For now, we'll use the same cleanup as integration tests
    sqlx::query("DELETE FROM jobs WHERE id = $1")
        .bind(job_id)
        .execute(&job_service.db.pool())
        .await
        .unwrap();
    
    sqlx::query("DELETE FROM job_queues WHERE id = $1")
        .bind(queue_id)
        .execute(&queue_service.db.pool())
        .await
        .unwrap();
}
```

## 6. Property-Based Testing

### 6.1. Testing dengan Proptest

```rust
// File: tests/property/job_property_tests.rs
use proptest::prelude::*;
use taskforge::models::job::{Job, JobStatus};

// Property: Job dengan prioritas valid harus dapat divalidasi
proptest! {
    #[test]
    fn test_valid_job_priority_always_validates(priority in 0i32..=9) {
        let job = Job::new(
            uuid::Uuid::new_v4(), // queue_id
            "test_job".to_string(),
            serde_json::json!({}),
            priority,
            Some(3),
        );
        
        assert!(job.validate().is_ok());
        assert_eq!(job.priority, priority);
    }
}

// Property: Job tidak boleh memiliki prioritas di luar rentang
proptest! {
    #[test]
    fn test_invalid_job_priority_fails_validation(priority in (i32::MIN..0).prop_filter("Must be out of range", |&x| x < 0 || x > 9)) {
        let result = std::panic::catch_unwind(|| {
            Job::new(
                uuid::Uuid::new_v4(), // queue_id
                "test_job".to_string(),
                serde_json::json!({}),
                priority,
                Some(3),
            )
        });
        
        // Dalam implementasi nyata, kita mungkin ingin mengembalikan error daripada panic
        // Tapi untuk keperluan test ini, kita asumsikan validasi terjadi setelah pembuatan
        let job = Job::new(
            uuid::Uuid::new_v4(), // queue_id
            "test_job".to_string(),
            serde_json::json!({}),
            5, // gunakan nilai valid untuk pembuatan
            Some(3),
        );
        
        // Modifikasi prioritas setelah pembuatan untuk pengujian validasi
        let mut job = job;
        job.priority = priority;
        assert!(job.validate().is_err());
    }
}

// Property: Job dengan jumlah maksimum percobaan positif harus valid
proptest! {
    #[test]
    fn test_valid_max_attempts_always_validates(max_attempts in 1i32..=10) {
        let job = Job::new(
            uuid::Uuid::new_v4(), // queue_id
            "test_job".to_string(),
            serde_json::json!({}),
            5,
            Some(max_attempts),
        );
        
        assert!(job.validate().is_ok());
        assert_eq!(job.max_attempts, max_attempts);
    }
}

// Property: Job dengan jumlah maksimum percobaan tidak valid harus gagal validasi
proptest! {
    #[test]
    fn test_invalid_max_attempts_fails_validation(max_attempts in (i32::MIN..=0).prop_filter("Must be invalid", |&x| x <= 0)) {
        let job = Job::new(
            uuid::Uuid::new_v4(), // queue_id
            "test_job".to_string(),
            serde_json::json!({}),
            5,
            Some(max_attempts),
        );
        
        // Karena kita tidak bisa membuat job dengan max_attempts <= 0,
        // kita perlu menguji validasi setelah pembuatan
        let mut job = job;
        job.max_attempts = max_attempts;
        assert!(job.validate().is_err());
    }
}

// Property: Job yang gagal dengan percobaan di bawah batas harus bisa diretry
proptest! {
    #[test]
    fn test_failed_job_below_max_attempts_can_be_retried(
        attempt_count in 1i32..10,
        max_attempts in 1i32..=20
    ) {
        if attempt_count < max_attempts {
            let mut job = Job::new(
                uuid::Uuid::new_v4(), // queue_id
                "test_job".to_string(),
                serde_json::json!({}),
                5,
                Some(max_attempts),
            );
            
            job.status = JobStatus::Failed;
            job.attempt_count = attempt_count;
            
            assert!(job.can_be_retried());
        }
    }
}

// Property: Job yang gagal dengan percobaan melebihi batas tidak boleh bisa diretry
proptest! {
    #[test]
    fn test_failed_job_above_max_attempts_cannot_be_retried(
        attempt_count in 1i32..=20,
        max_attempts in 1i32..=20
    ) {
        if attempt_count >= max_attempts {
            let mut job = Job::new(
                uuid::Uuid::new_v4(), // queue_id
                "test_job".to_string(),
                serde_json::json!({}),
                5,
                Some(max_attempts),
            );
            
            job.status = JobStatus::Failed;
            job.attempt_count = attempt_count;
            
            assert!(!job.can_be_retried());
        }
    }
}

// Strategy untuk menghasilkan job yang valid
fn valid_job_strategy() -> impl Strategy<Value = Job> {
    (
        prop::collection::vec(any::<u8>(), 16..=16), // For UUID generation
        "[a-zA-Z_][a-zA-Z0-9_]{1,99}", // Valid job type
        any::<serde_json::Value>(),
        0i32..=9, // Priority
        1i32..=10, // Max attempts
    )
        .prop_map(|(uuid_bytes, job_type, payload, priority, max_attempts)| {
            let queue_id = uuid::Uuid::from_slice(&uuid_bytes).unwrap();
            Job::new(
                queue_id,
                job_type,
                payload,
                priority,
                Some(max_attempts),
            )
        })
}

proptest! {
    #[test]
    fn test_valid_job_strategy_always_produces_valid_jobs(job in valid_job_strategy()) {
        assert!(job.validate().is_ok());
        assert!(job.priority >= 0 && job.priority <= 9);
        assert!(job.max_attempts >= 1 && job.max_attempts <= 10);
    }
}
```

## 7. Testing Kinerja dan Beban

### 7.1. Benchmark Testing

```rust
// File: benches/job_benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Bencher, Criterion};
use taskforge::models::job::Job;
use uuid::Uuid;

fn create_job_benchmark(c: &mut Criterion) {
    c.bench_function("create_job", |b| {
        b.iter(|| {
            let job = Job::new(
                black_box(Uuid::new_v4()),
                black_box("benchmark_job".to_string()),
                black_box(serde_json::json!({"data": "benchmark"})),
                black_box(5),
                black_box(Some(3)),
            );
            black_box(job)
        })
    });
}

fn validate_job_benchmark(c: &mut Criterion) {
    let valid_job = Job::new(
        Uuid::new_v4(),
        "valid_job".to_string(),
        serde_json::json!({}),
        5,
        Some(3),
    );
    
    c.bench_function("validate_job", |b| {
        b.iter(|| {
            black_box(&valid_job).validate().unwrap()
        })
    });
}

fn job_status_transition_benchmark(c: &mut Criterion) {
    c.bench_function("job_status_transitions", |b| {
        b.iter(|| {
            let mut job = Job::new(
                black_box(Uuid::new_v4()),
                black_box("transition_job".to_string()),
                black_box(serde_json::json!({"data": "transition"})),
                black_box(5),
                black_box(Some(3)),
            );
            
            // Simulate common transitions
            job.mark_as_processing();
            job.mark_as_succeeded(black_box(Some("Success".to_string())));
        })
    });
}

criterion_group!(
    benches,
    create_job_benchmark,
    validate_job_benchmark,
    job_status_transition_benchmark
);
criterion_main!(benches);
```

### 7.2. Load Testing Simulation

```rust
// File: tests/load_simulation.rs
use tokio::time::{Duration, Instant};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[tokio::test]
async fn simulate_concurrent_job_submissions() {
    let harness = TestHarness::new().await;
    let queue = harness.create_test_queue().await;
    
    let success_count = Arc::new(AtomicUsize::new(0));
    let error_count = Arc::new(AtomicUsize::new(0));
    
    let start_time = Instant::now();
    
    // Submit 100 jobs concurrently
    let mut handles = vec![];
    for i in 0..100 {
        let harness_clone = harness.clone();
        let queue_name = queue.name.clone();
        let success_count_clone = Arc::clone(&success_count);
        let error_count_clone = Arc::clone(&error_count);
        
        let handle = tokio::spawn(async move {
            let response = harness_clone
                .submit_job_via_api(
                    &queue_name,
                    &format!("load_test_job_{}", i),
                    serde_json::json!({"test_id": i}),
                )
                .await;
            
            if response.status().is_success() {
                success_count_clone.fetch_add(1, Ordering::SeqCst);
            } else {
                error_count_clone.fetch_add(1, Ordering::SeqCst);
            }
        });
        
        handles.push(handle);
        
        // Small delay to prevent overwhelming the system
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    
    // Wait for all submissions to complete
    for handle in handles {
        handle.await.unwrap();
    }
    
    let elapsed = start_time.elapsed();
    let total_requests = success_count.load(Ordering::SeqCst) + error_count.load(Ordering::SeqCst);
    let requests_per_second = total_requests as f64 / elapsed.as_secs_f64();
    
    println!("Load test results:");
    println!("  Total requests: {}", total_requests);
    println!("  Successful: {}", success_count.load(Ordering::SeqCst));
    println!("  Errors: {}", error_count.load(Ordering::SeqCst));
    println!("  Time elapsed: {:?}", elapsed);
    println!("  Requests per second: {:.2}", requests_per_second);
    
    // Assertions - these would be adjusted based on expected performance
    assert!(requests_per_second > 10.0, "Throughput too low: {} req/s", requests_per_second);
    assert!(elapsed < Duration::from_secs(30), "Test took too long: {:?}", elapsed);
    
    // Cleanup - in a real test, we'd clean up all the submitted jobs
    // For brevity, we'll skip cleanup in this example
}

#[tokio::test]
async fn test_memory_usage_under_load() {
    use std::mem;
    
    // Capture initial memory usage
    let initial_allocs = get_current_allocations();
    
    // Submit a series of jobs
    let harness = TestHarness::new().await;
    let queue = harness.create_test_queue().await;
    
    for i in 0..1000 {
        let _response = harness
            .submit_job_via_api(
                &queue.name,
                &format!("memory_test_job_{}", i),
                serde_json::json!({"iteration": i}),
            )
            .await;
        
        // Process every 100 iterations to simulate memory cleanup
        if i % 100 == 0 {
            tokio::task::yield_now().await; // Allow other tasks to run
        }
    }
    
    // Allow time for any async cleanup
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let final_allocs = get_current_allocations();
    let alloc_delta = final_allocs as i64 - initial_allocs as i64;
    
    println!("Memory allocation delta: {} allocations", alloc_delta);
    
    // In a real test, we might want to ensure memory usage doesn't grow unbounded
    // This assertion would depend on the specific memory characteristics we expect
    assert!(alloc_delta.abs() < 10000, "Memory usage grew unexpectedly: {} allocations", alloc_delta);
}

// Helper function to get current allocations (implementation would depend on allocator used)
fn get_current_allocations() -> usize {
    // This is a placeholder - in a real implementation, this would interface
    // with the allocator to get current allocation counts
    0
}
```

## 8. Continuous Testing dan CI/CD

### 8.1. Konfigurasi Testing untuk CI/CD

```yaml
# File: .github/workflows/test.yml
name: Test Suite

on:
  push:
    branches: [ main, develop ]
  pull_request:
    branches: [ main ]

env:
  CARGO_TERM_COLOR: always
  TEST_DATABASE_URL: postgresql://postgres:password@localhost/test_taskforge

jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v3
    
    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        profile: minimal
        toolchain: stable
        override: true
    
    - name: Install dependencies
      run: |
        sudo apt-get update
        sudo apt-get install -y libpq-dev pkg-config
    
    - name: Run unit tests
      run: |
        cargo test --lib -- --nocapture
      env:
        RUST_LOG: debug

  integration-tests:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:14
        env:
          POSTGRES_PASSWORD: password
          POSTGRES_DB: test_taskforge
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5432:5432
    
    steps:
    - uses: actions/checkout@v3
    
    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        profile: minimal
        toolchain: stable
        override: true
    
    - name: Install dependencies
      run: |
        sudo apt-get update
        sudo apt-get install -y libpq-dev pkg-config
    
    - name: Run integration tests
      run: |
        cargo test --test integration -- --nocapture
      env:
        TEST_DATABASE_URL: ${{ env.TEST_DATABASE_URL }}

  e2e-tests:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:14
        env:
          POSTGRES_PASSWORD: password
          POSTGRES_DB: test_taskforge
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5432:5432
      redis:
        image: redis:7-alpine
        options: >-
          --health-cmd "redis-cli ping"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 6379:6379
    
    steps:
    - uses: actions/checkout@v3
    
    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        profile: minimal
        toolchain: stable
        override: true
    
    - name: Install dependencies
      run: |
        sudo apt-get update
        sudo apt-get install -y libpq-dev pkg-config
    
    - name: Run E2E tests
      run: |
        cargo test --test e2e -- --nocapture
      env:
        TEST_DATABASE_URL: ${{ env.TEST_DATABASE_URL }}
        TEST_REDIS_URL: redis://localhost:6379

  property-tests:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v3
    
    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        profile: minimal
        toolchain: stable
        override: true
    
    - name: Run property tests
      run: |
        cargo test --test property -- --nocapture

  benchmarks:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v3
    
    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        profile: minimal
        toolchain: stable
        override: true
    
    - name: Run benchmarks
      run: |
        cargo bench

  coverage:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v3
    
    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        profile: minimal
        toolchain: stable
        override: true
    
    - name: Install cargo-tarpaulin
      run: |
        cargo install cargo-tarpaulin
    
    - name: Run coverage
      run: |
        cargo tarpaulin --verbose --workspace --timeout 120 --out Html
      env:
        TEST_DATABASE_URL: ${{ env.TEST_DATABASE_URL }}
    
    - name: Upload coverage reports to Codecov
      uses: codecov/codecov-action@v3
      with:
        file: ./tarpaulin-report.html
        fail_ci_if_error: true
```

### 8.2. Testing Tools dan Utilities

```rust
// File: tests/common/mod.rs
use taskforge::{database::Database, models::Organization, services::authentication_service::AuthenticationService};
use sqlx::{PgPool, Row};
use uuid::Uuid;
use chrono::{DateTime, Utc};

pub struct TestDatabase {
    pub pool: PgPool,
}

impl TestDatabase {
    pub async fn new() -> Self {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://postgres:password@localhost/test_taskforge".to_string());
        
        let pool = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test database");
        
        // Create a fresh test database instance
        Self { pool }
    }
    
    pub async fn cleanup(&self) -> Result<(), sqlx::Error> {
        // Clean up all test data
        sqlx::query("DELETE FROM jobs WHERE created_at > $1")
            .bind(Utc::now() - chrono::Duration::hours(1))
            .execute(&self.pool)
            .await?;
        
        sqlx::query("DELETE FROM job_queues WHERE created_at > $1")
            .bind(Utc::now() - chrono::Duration::hours(1))
            .execute(&self.pool)
            .await?;
        
        Ok(())
    }
    
    pub async fn create_test_organization(&self) -> Organization {
        let org = Organization::new(
            "Test Organization".to_string(),
            "test@example.com".to_string(),
            crate::models::organization::OrganizationTier::Pro,
        );
        
        sqlx::query!(
            "INSERT INTO organizations (id, name, billing_email, tier, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6)",
            org.id,
            org.name,
            org.billing_email,
            org.tier as crate::models::organization::OrganizationTier,
            org.created_at,
            org.updated_at
        )
        .execute(&self.pool)
        .await
        .expect("Failed to create test organization");
        
        org
    }
    
    pub async fn create_test_project(&self, organization_id: Uuid) -> crate::models::Project {
        let project = crate::models::Project::new(
            organization_id,
            "Test Project".to_string(),
            Some("Test project for integration testing".to_string()),
        );
        
        sqlx::query!(
            "INSERT INTO projects (id, organization_id, name, description, is_active, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            project.id,
            project.organization_id,
            project.name,
            project.description,
            project.is_active,
            project.created_at,
            project.updated_at
        )
        .execute(&self.pool)
        .await
        .expect("Failed to create test project");
        
        project
    }
}

pub struct TestSetup {
    pub db: TestDatabase,
    pub auth_service: AuthenticationService,
}

impl TestSetup {
    pub async fn new() -> Self {
        let db = TestDatabase::new().await;
        let auth_service = AuthenticationService::new(
            "test_secret_key_for_testing_purposes_only".to_string(),
        );
        
        Self { db, auth_service }
    }
    
    pub async fn cleanup(&self) -> Result<(), sqlx::Error> {
        self.db.cleanup().await
    }
}

#[macro_export]
macro_rules! assert_job_status {
    ($job:expr, $expected_status:expr) => {
        assert_eq!($job.status, $expected_status, 
            "Expected job status {:?}, but got {:?}", 
            $expected_status, 
            $job.status
        );
    };
}

#[macro_export]
macro_rules! assert_job_has_output {
    ($job:expr) => {
        assert!($job.output.is_some(), "Expected job to have output, but it was None");
        assert!(!$job.output.as_ref().unwrap().is_empty(), "Expected job output to be non-empty");
    };
}

#[macro_export]
macro_rules! assert_job_failed_with_error {
    ($job:expr, $expected_error:expr) => {
        assert_eq!($job.status, crate::models::job::JobStatus::Failed, 
            "Expected job to be in Failed status");
        assert!($job.error_message.is_some(), "Expected job to have error message");
        assert!($job.error_message.as_ref().unwrap().contains($expected_error),
            "Expected error message to contain '{}', but got: {:?}", 
            $expected_error, 
            $job.error_message
        );
    };
}
```

## 9. Best Practices dan Rekomendasi

### 9.1. Praktik Terbaik untuk Testing

1. **Gunakan pendekatan testing pyramid** - fokus pada unit test, sedikit integration, minimal E2E
2. **Gunakan test fixtures dan factories** - untuk membuat data uji yang konsisten
3. **Gunakan property-based testing** - untuk menguji berbagai kasus tepi
4. **Implementasikan test isolation** - setiap test harus independen
5. **Gunakan test doubles secara bijak** - hanya mocking dependencies eksternal

### 9.2. Coverage dan Quality Metrics

1. **Target coverage: 80%+** - untuk aplikasi produksi
2. **Branch coverage: 70%+** - untuk memastikan jalur logika tercakup
3. **Mutation testing** - untuk memastikan kualitas test
4. **Integration test coverage** - untuk memastikan komponen berinteraksi dengan benar
5. **Performance benchmarking** - untuk mendeteksi regresi kinerja

### 9.3. Maintenance dan Scalability

1. **Gunakan test linter** - untuk memastikan konsistensi kode test
2. **Gunakan parallel testing** - untuk efisiensi waktu eksekusi
3. **Implementasikan continuous testing** - dalam pipeline CI/CD
4. **Gunakan test flakiness detection** - untuk mengidentifikasi test yang tidak konsisten
5. **Gunakan performance regression testing** - untuk mendeteksi penurunan kinerja