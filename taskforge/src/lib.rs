//! TaskForge - High-performance background job queue system
//! 
//! TaskForge adalah platform SaaS background job queue yang dirancang untuk 
//! memberikan keandalan dan kinerja tinggi, khususnya untuk bisnis yang 
//! proses latar belakangnya bersifat kritis (fintech, e-commerce, data processing).

// Ekspor modul-modul utama
pub mod config;
pub mod database;
pub mod models;
pub mod services;
pub mod api;
pub mod middleware;
pub mod utils;
pub mod errors;

// Ekspor model-model penting
pub use models::{
    Job,
    JobQueue,
    Worker,
    ApiKey,
    Organization,
    Project,
    JobExecution,
    JobStatus,
    WorkerStatus,
    WorkerType,
    OrganizationTier,
    UserRole,
    ExecutionStatus,
    LogLevel,
    QueueWorkerAssignment,
    RetryConfig,
    RetryHistory,
    Subscription,
    UsageRecord,
    AuditLog,
    QueueSettings,
    WorkerCapabilities,
    JobDependency,
    JobFilter,
    ExecutionMetrics,
    ExecutionResult,
    ExecutionStats,
    ProjectApiKey,
    ProjectSettings,
    ProjectFilter,
    ProjectApiKeyRole,
    SecurityPolicy,
    SecurityPolicyType,
    AuditEventType,
    OrganizationUsage,
    UsageLimits,
    CurrentUsage,
    RemainingQuota,
    RetryRecommendation,
    RetryStats,
    WorkerMetrics,
    QueueHealth,
    QueueHealthDetailed,
    WorkerHealth,
    WorkerHealthDetailed,
    WorkerLoadInfo,
    QueueStats,
    OrganizationHealth,
    SystemHealth,
    SystemMetrics,
    BillingSummary,
    Invoice,
    InvoiceStatus,
    Payment,
    PaymentMethod,
    PaymentStatus,
    JobPerformanceMetrics,
    UsageMetrics,
    SecurityAuditLog,
    ApiCallRecord,
    WorkerLoadInfo,
    ExecutionPerformanceMetrics,
    ApiKeyUsageStats,
    ApiKeySecurityReport,
    DependencyChain,
    ExecutionTimelineEvent,
    ExecutionEventType,
    JobRequirements,
    WorkerCapabilities,
    QueueAssignmentStatus,
    QueueAssignmentType,
    JobDependency,
    DependencyType,
    DependencyStatus,
    JobSchedule,
    ScheduleType,
    ScheduleStatus,
    CronExpression,
    CronField,
    JobRetryConfig,
    RetryStatus,
    RetryHistory,
    RetryResult,
    JobResult,
    JobPayload,
    QueueSettings,
    QueueHealthStats,
    QueueHealthDetailed,
    WorkerHealthStats,
    WorkerHealthDetailed,
    WorkerQueueHealth,
    QueueWorkerAssignment,
    QueueWorkerAssignmentStatus,
    QueueWorkerAssignmentType,
    WorkerAssignment,
    WorkerAssignmentStatus,
    WorkerAssignmentType,
    WorkerQueueAssignment,
    WorkerQueueAssignmentStatus,
    WorkerQueueAssignmentType,
    QueueWorkerLink,
    QueueWorkerLinkStatus,
    QueueWorkerLinkType,
    WorkerQueueLink,
    WorkerQueueLinkStatus,
    WorkerQueueLinkType,
};

// Ekspor layanan-layanan penting
pub use services::{
    JobService,
    QueueService,
    WorkerService,
    ApiService,
    ApiService as ApiKeyService,
    ExecutionService,
    RetryService,
    AuthenticationService,
    AuthorizationService,
    OrganizationService,
    ProjectService,
    MonitoringService,
    MetricsService,
    AuditService,
    SecurityService,
    HeartbeatService,
    TimeoutManager,
    BackoffStrategy,
    FixedDelayBackoff,
    ExponentialBackoff,
    LinearBackoff,
    FibonacciBackoff,
    RetryService,
    RetryManager,
    IntelligentRetryService,
    DeadLetterService,
    JobScheduler,
    CronScheduler,
    DependencyService,
    WorkerAssignmentService,
    QueueAssignmentService,
    JobAssignmentService,
    WorkerQueueService,
    QueueWorkerService,
    WorkerAssignmentService,
    QueueAssignmentService,
    JobAssignmentService,
};

// Ekspor konfigurasi
pub use config::Settings;

// Ekspor error types
pub use errors::AppError;

// Ekspor utility functions
pub use utils::crypto;
pub use utils::validation;
pub use utils::logger;

// Konstanta aplikasi
pub const APP_NAME: &str = "TaskForge";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const API_VERSION: &str = "v1";

/// Fungsi untuk mendapatkan informasi versi aplikasi
pub fn app_info() -> String {
    format!("{} v{} (API v{})", APP_NAME, VERSION, API_VERSION)
}

/// Trait untuk komponen-komponen aplikasi yang perlu inisialisasi
#[async_trait::async_trait]
pub trait Initializable {
    async fn initialize(&self) -> Result<(), AppError>;
}

/// Trait untuk komponen-komponen aplikasi yang perlu shutdown
#[async_trait::async_trait]
pub trait Shutdownable {
    async fn shutdown(&self) -> Result<(), AppError>;
}

/// Trait untuk komponen-komponen aplikasi yang dapat diuji kesehatan
#[async_trait::async_trait]
pub trait HealthCheckable {
    async fn health_check(&self) -> Result<bool, AppError>;
}

/// Trait untuk komponen-komponen yang mendukung multi-tenant
pub trait TenantScoped {
    fn get_organization_id(&self) -> uuid::Uuid;
}

/// Trait untuk entitas yang memiliki metadata tambahan
pub trait MetadataContainer {
    fn get_metadata(&self) -> &serde_json::Value;
    fn set_metadata(&mut self, metadata: serde_json::Value);
}

/// Trait untuk entitas yang dapat divalidasi
pub trait Validate {
    fn validate(&self) -> Result<(), String>;
}

/// Trait untuk entitas yang memiliki lifecycle
pub trait Lifecycle {
    fn is_active(&self) -> bool;
    fn can_be_deleted(&self) -> bool;
    fn can_be_updated(&self) -> bool;
}

/// Fungsi inisialisasi utama aplikasi
pub async fn initialize_app(
    settings: &Settings,
) -> Result<
    (
        JobService,
        QueueService,
        WorkerService,
        ApiKeyService,
        ExecutionService,
        RetryService,
        MonitoringService,
        AuthenticationService,
        AuthorizationService,
        OrganizationService,
        ProjectService,
    ),
    AppError
> {
    // Buat koneksi database
    let db_pool = database::establish_connection(&settings.database.connection_string().expose_secret()).await?;
    
    // Jalankan migrasi
    database::run_migrations(&db_pool).await?;
    
    // Inisialisasi layanan autentikasi
    let auth_service = AuthenticationService::new(settings.authentication.clone());
    let authz_service = AuthorizationService::new(db_pool.clone(), auth_service.clone());
    
    // Inisialisasi layanan organisasi dan proyek
    let organization_service = OrganizationService::new(db_pool.clone());
    let project_service = ProjectService::new(db_pool.clone(), organization_service.clone());
    
    // Inisialisasi layanan utama
    let queue_service = QueueService::new(db_pool.clone(), authz_service.clone());
    let worker_service = WorkerService::new(db_pool.clone(), authz_service.clone());
    let api_key_service = ApiService::new(db_pool.clone(), authz_service.clone());
    let execution_service = ExecutionService::new(
        db_pool.clone(),
        authz_service.clone(),
        worker_service.clone(),
    );
    let job_service = JobService::new(
        db_pool.clone(),
        queue_service.clone(),
        execution_service.clone(),
        authz_service.clone(),
    );
    let retry_service = RetryService::new(
        db_pool.clone(),
        job_service.clone(),
        queue_service.clone(),
        execution_service.clone(),
    );
    let monitoring_service = MonitoringService::new(
        db_pool.clone(),
        settings.monitoring.clone(),
    );
    
    Ok((
        job_service,
        queue_service,
        worker_service,
        api_key_service,
        execution_service,
        retry_service,
        monitoring_service,
        auth_service,
        authz_service,
        organization_service,
        project_service,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_info() {
        let info = app_info();
        assert!(info.contains("TaskForge"));
        assert!(info.contains(env!("CARGO_PKG_VERSION")));
        assert!(info.contains("API v1"));
    }
}