pub mod organization;
pub mod project;
pub mod user;
pub mod job_queue;
pub mod job;
pub mod worker;
pub mod api_key;
pub mod job_execution;
pub mod job_dependency;
pub mod subscription;
pub mod usage_record;
pub mod audit_log;
pub mod queue_worker_assignment;
pub mod retry_config;
pub mod retry_history;

pub use organization::Organization;
pub use project::Project;
pub use user::User;
pub use job_queue::JobQueue;
pub use job::Job;
pub use worker::Worker;
pub use api_key::ApiKey;
pub use job_execution::JobExecution;
pub use job_dependency::JobDependency;
pub use subscription::Subscription;
pub use usage_record::UsageRecord;
pub use audit_log::AuditLog;
pub use queue_worker_assignment::QueueWorkerAssignment;
pub use retry_config::RetryConfig;
pub use retry_history::RetryHistory;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, Type};

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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "worker_status", rename_all = "lowercase")]
pub enum WorkerStatus {
    Online,
    Offline,
    Draining,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "worker_type", rename_all = "lowercase")]
pub enum WorkerType {
    General,
    SpecializedCpu,
    SpecializedIo,
    SpecializedGpu,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "organization_tier", rename_all = "lowercase")]
pub enum OrganizationTier {
    Starter,
    Pro,
    Enterprise,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    Member,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "api_key_scope", rename_all = "snake_case")]
pub enum ApiKeyScope {
    JobRead,
    JobWrite,
    QueueRead,
    QueueWrite,
    WorkerRead,
    WorkerWrite,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "subscription_tier", rename_all = "lowercase")]
pub enum SubscriptionTier {
    Starter,
    Pro,
    Enterprise,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "dependency_type", rename_all = "lowercase")]
pub enum DependencyType {
    Sequential,
    Parallel,
    Conditional,
    FanIn,
    FanOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "dependency_status", rename_all = "lowercase")]
pub enum DependencyStatus {
    Pending,
    Resolved,
    Failed,
    Cancelled,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "retry_status", rename_all = "lowercase")]
pub enum RetryStatus {
    Scheduled,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "audit_event_type", rename_all = "snake_case")]
pub enum AuditEventType {
    Login,
    Logout,
    ApiAccess,
    JobSubmission,
    JobCompletion,
    JobFailure,
    QueueManagement,
    WorkerRegistration,
    UserManagement,
    ConfigurationChange,
    SecurityIncident,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BaseModel {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl BaseModel {
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

/// Trait untuk entitas yang dapat diotentikasi dan diautorisasi
pub trait TenantScoped {
    fn get_organization_id(&self) -> Uuid;
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