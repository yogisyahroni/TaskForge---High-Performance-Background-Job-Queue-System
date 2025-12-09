# Model Data dan ORM Mapping untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan model data untuk aplikasi TaskForge berdasarkan Entity-Relationship Diagram (ERD) yang tercantum dalam PRD. Setiap model akan dijelaskan struktur datanya, hubungan antar model, dan implementasi ORM mapping menggunakan SQLx dalam Rust.

## 2. Model Organisasi dan Pengguna

### 2.1. Organization Model
Model ini merepresentasikan organisasi yang merupakan entitas utama dalam sistem multi-tenant.

```rust
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub billing_email: String,
    pub created_at: DateTime<Utc>,
}

// Implementasi tambahan untuk validasi dan operasi terkait organisasi
impl Organization {
    pub fn new(name: String, billing_email: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            billing_email,
            created_at: Utc::now(),
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Organization name cannot be empty".to_string());
        }
        if !self.billing_email.contains('@') {
            return Err("Invalid billing email format".to_string());
        }
        Ok(())
    }
}
```

### 2.2. User Model
Model ini merepresentasikan pengguna yang tergabung dalam organisasi.

```rust
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    Member,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
}

impl User {
    pub fn new(organization_id: Uuid, email: String, password_hash: String, role: UserRole) -> Self {
        Self {
            id: Uuid::new_v4(),
            organization_id,
            email,
            password_hash,
            role,
            created_at: Utc::now(),
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if !self.email.contains('@') {
            return Err("Invalid email format".to_string());
        }
        Ok(())
    }
}
```

### 2.3. Subscription Model
Model ini merepresentasikan langganan organisasi terhadap layanan TaskForge.

```rust
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "subscription_tier", rename_all = "lowercase")]
pub enum SubscriptionTier {
    Starter,
    Pro,
    Enterprise,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Subscription {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub tier: SubscriptionTier,
    pub limits: Value, // JSONB field untuk menyimpan batas-batas langganan
    pub current_period_end: DateTime<Utc>,
}

impl Subscription {
    pub fn new(organization_id: Uuid, tier: SubscriptionTier, limits: Value, current_period_end: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            organization_id,
            tier,
            limits,
            current_period_end,
        }
    }
    
    pub fn is_active(&self) -> bool {
        self.current_period_end > Utc::now()
    }
    
    pub fn get_max_jobs_per_month(&self) -> Option<i32> {
        self.limits.get("max_jobs_per_month")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
    }
    
    pub fn get_max_workers(&self) -> Option<i32> {
        self.limits.get("max_workers")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
    }
}
```

## 3. Model Proyek dan API Key

### 3.1. Project Model
Model ini merepresentasikan proyek yang dibuat dalam organisasi.

```rust
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Project {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Project {
    pub fn new(organization_id: Uuid, name: String, description: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            organization_id,
            name,
            description,
            created_at: Utc::now(),
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("Project name cannot be empty".to_string());
        }
        Ok(())
    }
}
```

### 3.2. ApiKey Model
Model ini merepresentasikan kunci API yang digunakan untuk mengakses layanan TaskForge.

```rust
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key_hash: String, // Hash dari kunci API untuk keamanan
    pub name: String,
    pub permissions: Vec<String>, // Array izin akses
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl ApiKey {
    pub fn new(project_id: Uuid, key_hash: String, name: String, permissions: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            key_hash,
            name,
            permissions,
            last_used_at: None,
            expires_at: None,
        }
    }
    
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            return Utc::now() > expires_at;
        }
        false
    }
    
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(&permission.to_string())
    }
}
```

## 4. Model Queue dan Job

### 4.1. JobQueue Model
Model ini merepresentasikan antrian job dalam sistem.

```rust
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobQueue {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub priority: i32, // 0-9, semakin tinggi semakin prioritas
    pub settings: Value, // JSONB field untuk konfigurasi antrian
    pub created_at: DateTime<Utc>,
}

impl JobQueue {
    pub fn new(project_id: Uuid, name: String, priority: i32, settings: Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            name,
            priority,
            settings,
            created_at: Utc::now(),
        }
    
    pub fn get_retry_policy(&self) -> Option<RetryPolicy> {
        // Implementasi untuk mengambil kebijakan retry dari settings
        // Contoh: self.settings.get("retry_policy").map(|v| serde_json::from_value(v.clone()).ok()).flatten()
        None
    }
    
    pub fn get_timeout(&self) -> Option<i32> {
        self.settings.get("timeout")
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: i32,
    pub base_delay: i32, // dalam detik
    pub max_delay: i32, // dalam detik
    pub backoff_multiplier: f64,
}
```

### 4.2. Job Model
Model ini merepresentasikan job individual dalam sistem.

```rust
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
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Job {
    pub id: Uuid,
    pub queue_id: Uuid,
    pub job_type: String, // Nama tipe job untuk routing ke worker yang tepat
    pub payload: Value,   // JSONB field untuk data job
    pub status: JobStatus,
    pub priority: i32,    // 0-9, semakin tinggi semakin prioritas
    pub scheduled_for: Option<DateTime<Utc>>, // Waktu eksekusi dijadwalkan
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Job {
    pub fn new(queue_id: Uuid, job_type: String, payload: Value, priority: i32) -> Self {
        Self {
            id: Uuid::new_v4(),
            queue_id,
            job_type,
            payload,
            status: JobStatus::Pending,
            priority,
            scheduled_for: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
    
    pub fn new_scheduled(queue_id: Uuid, job_type: String, payload: Value, priority: i32, scheduled_for: DateTime<Utc>) -> Self {
        let mut job = Self::new(queue_id, job_type, payload, priority);
        job.status = JobStatus::Scheduled;
        job.scheduled_for = Some(scheduled_for);
        job
    }
    
    pub fn is_ready_for_execution(&self) -> bool {
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
    
    pub fn can_be_retried(&self, max_attempts: i32) -> bool {
        // Implementasi logika untuk menentukan apakah job bisa diretry
        // Misalnya, jika status adalah Failed dan jumlah attempt < max_attempts
        matches!(self.status, JobStatus::Failed) && self.get_attempt_count() < max_attempts
    }
    
    fn get_attempt_count(&self) -> i32 {
        // Implementasi untuk mendapatkan jumlah percobaan dari payload atau tabel eksekusi
        1 // Placeholder
    }
}
```

## 5. Model Worker dan Eksekusi

### 5.1. Worker Model
Model ini merepresentasikan worker yang mengeksekusi job.

```rust
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "worker_type", rename_all = "lowercase")]
pub enum WorkerType {
    General,
    SpecializedCpu,
    SpecializedIo,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "worker_status", rename_all = "lowercase")]
pub enum WorkerStatus {
    Online,
    Offline,
    Draining,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Worker {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub worker_type: WorkerType,
    pub status: WorkerStatus,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl Worker {
    pub fn new(project_id: Uuid, name: String, worker_type: WorkerType) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            name,
            worker_type,
            status: WorkerStatus::Offline,
            last_heartbeat: None,
            created_at: Utc::now(),
        }
    }
    
    pub fn is_healthy(&self, heartbeat_timeout_seconds: i64) -> bool {
        if let Some(last_heartbeat) = self.last_heartbeat {
            let now = Utc::now();
            let duration_since_heartbeat = now.timestamp() - last_heartbeat.timestamp();
            duration_since_heartbeat < heartbeat_timeout_seconds
        } else {
            false
        }
    }
    
    pub fn is_available(&self) -> bool {
        matches!(self.status, WorkerStatus::Online)
    }
}
```

### 5.2. QueueWorkerAssignment Model
Model ini merepresentasikan hubungan assignment antara queue dan worker.

```rust
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct QueueWorkerAssignment {
    pub queue_id: Uuid,
    pub worker_id: Uuid,
    pub weight: i32,      // Bobot untuk load balancing
    pub is_active: bool,  // Apakah assignment aktif
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl QueueWorkerAssignment {
    pub fn new(queue_id: Uuid, worker_id: Uuid, weight: i32) -> Self {
        Self {
            queue_id,
            worker_id,
            weight,
            is_active: true,
            created_at: chrono::Utc::now(),
        }
    }
}
```

### 5.3. JobExecution Model
Model ini merepresentasikan eksekusi spesifik dari job.

```rust
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "execution_status", rename_all = "lowercase")]
pub enum ExecutionStatus {
    Started,
    Succeeded,
    Failed,
    Retrying,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobExecution {
    pub id: Uuid,
    pub job_id: Uuid,
    pub worker_id: Uuid,
    pub status: ExecutionStatus,
    pub output: Option<String>,      // Output dari eksekusi job
    pub attempt_number: i32,         // Nomor percobaan (untuk retry)
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl JobExecution {
    pub fn new(job_id: Uuid, worker_id: Uuid, attempt_number: i32) -> Self {
        Self {
            id: Uuid::new_v4(),
            job_id,
            worker_id,
            status: ExecutionStatus::Started,
            output: None,
            attempt_number,
            started_at: Some(Utc::now()),
            finished_at: None,
            created_at: Utc::now(),
        }
    }
    
    pub fn mark_succeeded(&mut self, output: Option<String>) {
        self.status = ExecutionStatus::Succeeded;
        self.output = output;
        self.finished_at = Some(Utc::now());
    }
    
    pub fn mark_failed(&mut self) {
        self.status = ExecutionStatus::Failed;
        self.finished_at = Some(Utc::now());
    }
    
    pub fn mark_retrying(&mut self) {
        self.status = ExecutionStatus::Retrying;
        self.finished_at = Some(Utc::now());
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

### 5.4. ExecutionLog Model
Model ini merepresentasikan log dari eksekusi job.

```rust
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "log_level", rename_all = "lowercase")]
pub enum LogLevel {
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
    pub metadata: Value,            // JSONB field untuk metadata tambahan
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
}
```

## 6. Model Ketergantungan Job

### 6.1. JobDependency Model
Model ini merepresentasikan ketergantungan antar job.

```rust
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobDependency {
    pub parent_job_id: Uuid,
    pub child_job_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl JobDependency {
    pub fn new(parent_job_id: Uuid, child_job_id: Uuid) -> Self {
        Self {
            parent_job_id,
            child_job_id,
            created_at: chrono::Utc::now(),
        }
    }
}
```

## 7. Implementasi Repository Pattern

Setiap model akan memiliki repository terkait untuk operasi database:

```rust
// Contoh struktur repository untuk Organization
use sqlx::PgPool;
use uuid::Uuid;
use crate::models::Organization;

pub struct OrganizationRepository {
    pool: PgPool,
}

impl OrganizationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub async fn create(&self, organization: Organization) -> Result<Organization, sqlx::Error> {
        sqlx::query_as!(
            Organization,
            r#"
            INSERT INTO organizations (id, name, billing_email, created_at)
            VALUES ($1, $2, $3, $4)
            RETURNING id, name, billing_email, created_at
            "#,
            organization.id,
            organization.name,
            organization.billing_email,
            organization.created_at
        )
        .fetch_one(&self.pool)
        .await
    }
    
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Organization>, sqlx::Error> {
        sqlx::query_as!(
            Organization,
            r#"
            SELECT id, name, billing_email, created_at
            FROM organizations
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
    }
    
    pub async fn find_by_email(&self, billing_email: &str) -> Result<Option<Organization>, sqlx::Error> {
        sqlx::query_as!(
            Organization,
            r#"
            SELECT id, name, billing_email, created_at
            FROM organizations
            WHERE billing_email = $1
            "#,
            billing_email
        )
        .fetch_optional(&self.pool)
        .await
    }
    
    pub async fn update(&self, organization: Organization) -> Result<Organization, sqlx::Error> {
        sqlx::query_as!(
            Organization,
            r#"
            UPDATE organizations
            SET name = $1, billing_email = $2
            WHERE id = $3
            RETURNING id, name, billing_email, created_at
            "#,
            organization.name,
            organization.billing_email,
            organization.id
        )
        .fetch_one(&self.pool)
        .await
    }
    
    pub async fn delete(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM organizations
            WHERE id = $1
            "#,
            id
        )
        .execute(&self.pool)
        .await?;
        
        Ok(result.rows_affected() > 0)
    }
}
```

## 8. Validasi dan Indeks Database

### 8.1. Indeks Penting
Berdasarkan kebutuhan performa dari PRD, berikut adalah indeks yang perlu dibuat:

```sql
-- Indeks untuk tabel Job
CREATE INDEX idx_jobs_status_scheduled_priority ON jobs(status, scheduled_for, priority);
CREATE INDEX idx_jobs_queue_id ON jobs(queue_id);
CREATE INDEX idx_jobs_created_at ON jobs(created_at);

-- Indeks untuk tabel JobExecution
CREATE INDEX idx_job_executions_job_id ON job_executions(job_id);
CREATE INDEX idx_job_executions_worker_id ON job_executions(worker_id);

-- Indeks untuk tabel Worker
CREATE INDEX idx_workers_last_heartbeat ON workers(last_heartbeat);
CREATE INDEX idx_workers_project_id ON workers(project_id);

-- Indeks untuk tabel Organization
CREATE INDEX idx_organizations_created_at ON organizations(created_at);
```

### 8.2. Validasi Data
Setiap model harus memiliki metode validasi untuk memastikan data yang disimpan memenuhi kriteria yang ditentukan:

- Email harus dalam format yang valid
- Nama organisasi dan proyek tidak boleh kosong
- Priority harus dalam rentang yang valid (0-9)
- Tanggal harus dalam format yang valid
- UUID harus dalam format yang valid