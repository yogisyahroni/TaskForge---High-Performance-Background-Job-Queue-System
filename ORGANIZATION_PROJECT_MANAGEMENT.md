# Manajemen Organisasi dan Proyek untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan sistem manajemen organisasi dan proyek dalam aplikasi TaskForge. Sistem ini merupakan fondasi dari arsitektur multi-tenant, di mana setiap organisasi memiliki kumpulan proyek yang terisolasi satu sama lain.

## 2. Arsitektur Multi-Tenant

### 2.1. Model Organisasi

Organisasi adalah entitas tingkat atas dalam sistem TaskForge yang berfungsi sebagai wadah untuk proyek-proyek dan pengguna-pengguna:

```rust
// File: src/models/organization.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "organization_tier", rename_all = "lowercase")]
pub enum OrganizationTier {
    Starter,
    Pro,
    Enterprise,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub billing_email: String,
    pub tier: OrganizationTier,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Organization {
    pub fn new(name: String, billing_email: String, tier: OrganizationTier) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            billing_email,
            tier,
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("Organization name must be between 1 and 100 characters".to_string());
        }
        
        if !self.billing_email.contains('@') || self.billing_email.len() > 254 {
            return Err("Invalid billing email format".to_string());
        }
        
        Ok(())
    }
    
    pub fn can_create_project(&self, current_project_count: u32) -> bool {
        match self.tier {
            OrganizationTier::Starter => current_project_count < 1,
            OrganizationTier::Pro => current_project_count < 5,
            OrganizationTier::Enterprise => current_project_count < 100, // unlimited for practical purposes
        }
    }
    
    pub fn get_max_projects(&self) -> u32 {
        match self.tier {
            OrganizationTier::Starter => 1,
            OrganizationTier::Pro => 5,
            OrganizationTier::Enterprise => 10,
        }
    }
}
```

### 2.2. Model Proyek

Proyek adalah entitas yang mengelompokkan queue, job, dan worker dalam organisasi:

```rust
// File: src/models/project.rs
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
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    pub fn new(organization_id: Uuid, name: String, description: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            organization_id,
            name,
            description,
            is_active: true,
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("Project name must be between 1 and 100 characters".to_string());
        }
        
        if let Some(desc) = &self.description {
            if desc.len() > 500 {
                return Err("Project description cannot exceed 500 characters".to_string());
            }
        
        Ok(())
    }
    
    pub fn can_be_used(&self) -> bool {
        self.is_active
    }
}
```

## 3. Sistem Manajemen Organisasi

### 3.1. Fungsi-fungsi Utama

```rust
// File: src/services/organization_service.rs
use crate::{
    database::Database,
    models::{Organization, OrganizationTier},
    auth::middleware::AuthenticatedUser,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;

pub struct OrganizationService {
    db: Database,
}

impl OrganizationService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn create_organization(
        &self,
        name: String,
        billing_email: String,
        tier: OrganizationTier,
    ) -> Result<Organization, SqlxError> {
        let org = Organization::new(name, billing_email, tier);
        org.validate().map_err(|e| SqlxError::RowNotFound)?;
        
        self.db.create_organization(org).await
    }
    
    pub async fn get_organization_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<Organization>, SqlxError> {
        self.db.get_organization_by_id(id).await
    }
    
    pub async fn get_organization_by_user(
        &self,
        user_id: Uuid,
    ) -> Result<Option<Organization>, SqlxError> {
        self.db.get_organization_by_user_id(user_id).await
    }
    
    pub async fn update_organization_tier(
        &self,
        organization_id: Uuid,
        new_tier: OrganizationTier,
    ) -> Result<(), SqlxError> {
        self.db.update_organization_tier(organization_id, new_tier).await
    }
    
    pub async fn get_organization_usage(
        &self,
        organization_id: Uuid,
    ) -> Result<OrganizationUsage, SqlxError> {
        let project_count = self.db.get_project_count_for_org(organization_id).await?;
        let user_count = self.db.get_user_count_for_org(organization_id).await?;
        let job_count = self.db.get_job_count_for_org(organization_id).await?;
        
        Ok(OrganizationUsage {
            project_count,
            user_count,
            job_count,
        })
    }
}

pub struct OrganizationUsage {
    pub project_count: u32,
    pub user_count: u32,
    pub job_count: u64,
}
```

### 3.2. Endpoint API Organisasi

```rust
// File: src/handlers/organization_handler.rs
use axum::{
    extract::{State, Path, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use crate::{
    services::organization_service::{OrganizationService, OrganizationUsage},
    auth::middleware::{AuthenticatedUser, check_permission},
    models::OrganizationTier,
};

#[derive(Deserialize)]
pub struct CreateOrganizationRequest {
    pub name: String,
    pub billing_email: String,
    pub tier: Option<OrganizationTier>,
}

#[derive(Serialize)]
pub struct CreateOrganizationResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub billing_email: String,
    pub tier: OrganizationTier,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn create_organization(
    State(org_service): State<OrganizationService>,
    authenticated_user: AuthenticatedUser,
    Json(payload): Json<CreateOrganizationRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // Hanya admin yang bisa membuat organisasi
    if !check_permission(&authenticated_user, "organization:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let tier = payload.tier.unwrap_or(OrganizationTier::Starter);
    
    let organization = org_service
        .create_organization(payload.name, payload.billing_email, tier)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(CreateOrganizationResponse {
        id: organization.id,
        name: organization.name,
        billing_email: organization.billing_email,
        tier: organization.tier,
        created_at: organization.created_at,
    }))
}

pub async fn get_organization(
    State(org_service): State<OrganizationService>,
    authenticated_user: AuthenticatedUser,
    Path(org_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "organization:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Pastikan user adalah bagian dari organisasi ini
    let user_org = org_service
        .get_organization_by_user(authenticated_user.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    if user_org.as_ref().map(|o| o.id) != Some(org_id) {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let organization = org_service
        .get_organization_by_id(org_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    Ok(Json(organization))
}

pub async fn get_organization_usage(
    State(org_service): State<OrganizationService>,
    authenticated_user: AuthenticatedUser,
    Path(org_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "organization:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Pastikan user adalah bagian dari organisasi ini
    let user_org = org_service
        .get_organization_by_user(authenticated_user.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    if user_org.as_ref().map(|o| o.id) != Some(org_id) {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let usage = org_service
        .get_organization_usage(org_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(usage))
}
```

## 4. Sistem Manajemen Proyek

### 4.1. Fungsi-fungsi Utama

```rust
// File: src/services/project_service.rs
use crate::{
    database::Database,
    models::Project,
    auth::middleware::AuthenticatedUser,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;

pub struct ProjectService {
    db: Database,
}

impl ProjectService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn create_project(
        &self,
        organization_id: Uuid,
        name: String,
        description: Option<String>,
    ) -> Result<Project, SqlxError> {
        // Periksa apakah organisasi sudah mencapai batas proyek
        let current_count = self.db.get_project_count_for_org(organization_id).await?;
        let org = self.db.get_organization_by_id(organization_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if !org.can_create_project(current_count) {
            return Err(SqlxError::RowNotFound); // Using this for quota exceeded
        }
        
        let project = Project::new(organization_id, name, description);
        project.validate().map_err(|e| SqlxError::RowNotFound)?;
        
        self.db.create_project(project).await
    }
    
    pub async fn get_project_by_id(
        &self,
        project_id: Uuid,
    ) -> Result<Option<Project>, SqlxError> {
        self.db.get_project_by_id(project_id).await
    }
    
    pub async fn get_projects_by_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<Project>, SqlxError> {
        self.db.get_projects_by_organization(organization_id).await
    }
    
    pub async fn update_project(
        &self,
        project_id: Uuid,
        name: Option<String>,
        description: Option<String>,
        is_active: Option<bool>,
    ) -> Result<(), SqlxError> {
        self.db.update_project(project_id, name, description, is_active).await
    }
    
    pub async fn delete_project(
        &self,
        project_id: Uuid,
    ) -> Result<(), SqlxError> {
        // Perlu memastikan tidak ada queue aktif di proyek ini
        let active_queues = self.db.get_active_queues_count(project_id).await?;
        if active_queues > 0 {
            return Err(SqlxError::RowNotFound); // Tidak bisa menghapus proyek dengan queue aktif
        }
        
        self.db.delete_project(project_id).await
    }
    
    pub async fn can_access_project(
        &self,
        user: &AuthenticatedUser,
        project_id: Uuid,
    ) -> Result<bool, SqlxError> {
        // Periksa apakah proyek milik organisasi yang sama dengan user
        let project = self.get_project_by_id(project_id).await?;
        match project {
            Some(proj) => Ok(proj.organization_id == user.organization_id),
            None => Ok(false),
        }
    }
}
```

### 4.2. Endpoint API Proyek

```rust
// File: src/handlers/project_handler.rs
use axum::{
    extract::{State, Path, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use crate::{
    services::project_service::ProjectService,
    auth::middleware::{AuthenticatedUser, check_permission},
    models::Project,
};

#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_active: Option<bool>,
}

pub async fn create_project(
    State(project_service): State<ProjectService>,
    authenticated_user: AuthenticatedUser,
    Json(payload): Json<CreateProjectRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "project:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let project = project_service
        .create_project(authenticated_user.organization_id, payload.name, payload.description)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(project))
}

pub async fn get_project(
    State(project_service): State<ProjectService>,
    authenticated_user: AuthenticatedUser,
    Path(project_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "project:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Pastikan user dapat mengakses proyek ini
    if !project_service
        .can_access_project(&authenticated_user, project_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let project = project_service
        .get_project_by_id(project_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    Ok(Json(project))
}

pub async fn get_projects(
    State(project_service): State<ProjectService>,
    authenticated_user: AuthenticatedUser,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "project:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let projects = project_service
        .get_projects_by_organization(authenticated_user.organization_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(projects))
}

pub async fn update_project(
    State(project_service): State<ProjectService>,
    authenticated_user: AuthenticatedUser,
    Path(project_id): Path<uuid::Uuid>,
    Json(payload): Json<UpdateProjectRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "project:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Pastikan user dapat mengakses proyek ini
    if !project_service
        .can_access_project(&authenticated_user, project_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        return Err(StatusCode::FORBIDDEN);
    }
    
    project_service
        .update_project(
            project_id,
            payload.name,
            payload.description,
            payload.is_active,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_project(
    State(project_service): State<ProjectService>,
    authenticated_user: AuthenticatedUser,
    Path(project_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "project:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Pastikan user dapat mengakses proyek ini
    if !project_service
        .can_access_project(&authenticated_user, project_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)? {
        return Err(StatusCode::FORBIDDEN);
    }
    
    project_service
        .delete_project(project_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}
```

## 5. Validasi dan Pembatasan

### 5.1. Pembatasan Berbasis Tier Organisasi

```rust
// File: src/services/usage_service.rs
use crate::database::Database;
use uuid::Uuid;
use sqlx::Error as SqlxError;

pub struct UsageService {
    db: Database,
}

impl UsageService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn check_usage_limits(
        &self,
        organization_id: Uuid,
    ) -> Result<UsageCheckResult, SqlxError> {
        let org = self.db.get_organization_by_id(organization_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let project_count = self.db.get_project_count_for_org(organization_id).await?;
        let user_count = self.db.get_user_count_for_org(organization_id).await?;
        let queue_count = self.db.get_queue_count_for_org(organization_id).await?;
        
        let max_projects = org.get_max_projects();
        
        let mut violations = Vec::new();
        
        if project_count >= max_projects {
            violations.push("project_limit_exceeded".to_string());
        }
        
        // Bisa ditambahkan pembatasan lainnya sesuai tier
        match org.tier {
            crate::models::OrganizationTier::Starter => {
                if user_count > 3 {
                    violations.push("user_limit_exceeded".to_string());
                }
                if queue_count > 5 {
                    violations.push("queue_limit_exceeded".to_string());
                }
            }
            crate::models::OrganizationTier::Pro => {
                if user_count > 10 {
                    violations.push("user_limit_exceeded".to_string());
                }
                if queue_count > 20 {
                    violations.push("queue_limit_exceeded".to_string());
                }
            }
            crate::models::OrganizationTier::Enterprise => {
                // Tidak ada batasan ketat untuk tier enterprise
            }
        }
        
        Ok(UsageCheckResult {
            violations,
            current_usage: CurrentUsage {
                projects: project_count,
                users: user_count,
                queues: queue_count,
            },
            limits: UsageLimits {
                max_projects,
                max_users: match org.tier {
                    crate::models::OrganizationTier::Starter => 3,
                    crate::models::OrganizationTier::Pro => 10,
                    crate::models::OrganizationTier::Enterprise => 100,
                },
                max_queues: match org.tier {
                    crate::models::OrganizationTier::Starter => 5,
                    crate::models::OrganizationTier::Pro => 20,
                    crate::models::OrganizationTier::Enterprise => 1000,
                },
            },
        })
    }
}

pub struct UsageCheckResult {
    pub violations: Vec<String>,
    pub current_usage: CurrentUsage,
    pub limits: UsageLimits,
}

pub struct CurrentUsage {
    pub projects: u32,
    pub users: u32,
    pub queues: u32,
}

pub struct UsageLimits {
    pub max_projects: u32,
    pub max_users: u32,
    pub max_queues: u32,
}
```

### 5.2. Middleware Validasi Penggunaan

```rust
// File: src/middleware/usage_limit_middleware.rs
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use http::StatusCode;
use crate::services::usage_service::UsageService;

pub async fn usage_limit_middleware(
    State(usage_service): State<UsageService>,
    authenticated_user: crate::auth::middleware::AuthenticatedUser,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Periksa apakah ada pelanggaran batas penggunaan
    let usage_result = usage_service
        .check_usage_limits(authenticated_user.organization_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // Jika ada pelanggaran penting, tolak permintaan
    if usage_result.violations.iter().any(|v| {
        v.contains("project_limit_exceeded") || 
        v.contains("queue_limit_exceeded")
    }) {
        return Err(StatusCode::PAYLOAD_TOO_LARGE); // 413
    }
    
    Ok(next.run(request).await)
}
```

## 6. Manajemen Pengguna dalam Organisasi

### 6.1. Model Pengguna Organisasi

```rust
// File: src/models/user.rs
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
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn new(organization_id: Uuid, email: String, password_hash: String, role: UserRole) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            organization_id,
            email,
            password_hash,
            role,
            is_active: true,
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if !self.email.contains('@') || self.email.len() > 254 {
            return Err("Invalid email format".to_string());
        }
        
        Ok(())
    }
    
    pub fn can_access_resource(&self, resource_org_id: Uuid) -> bool {
        self.organization_id == resource_org_id
    }
}
```

### 6.2. Layanan Manajemen Pengguna

```rust
// File: src/services/user_service.rs
use crate::{
    database::Database,
    models::{User, UserRole},
    auth::middleware::AuthenticatedUser,
    utils::crypto,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;

pub struct UserService {
    db: Database,
}

impl UserService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn create_user(
        &self,
        organization_id: Uuid,
        email: String,
        password: String,
        role: UserRole,
    ) -> Result<User, SqlxError> {
        // Periksa apakah email sudah digunakan
        if self.db.get_user_by_email(&email).await?.is_some() {
            return Err(SqlxError::RowNotFound); // Email already exists
        }
        
        // Hash password
        let password_hash = crypto::hash_password(&password);
        
        let user = User::new(organization_id, email, password_hash, role);
        user.validate().map_err(|e| SqlxError::RowNotFound)?;
        
        self.db.create_user(user).await
    }
    
    pub async fn get_user_by_id(
        &self,
        user_id: Uuid,
    ) -> Result<Option<User>, SqlxError> {
        self.db.get_user_by_id(user_id).await
    }
    
    pub async fn get_users_by_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<User>, SqlxError> {
        self.db.get_users_by_organization(organization_id).await
    }
    
    pub async fn update_user_role(
        &self,
        user_id: Uuid,
        new_role: UserRole,
    ) -> Result<(), SqlxError> {
        self.db.update_user_role(user_id, new_role).await
    }
    
    pub async fn deactivate_user(
        &self,
        requesting_user: &AuthenticatedUser,
        target_user_id: Uuid,
    ) -> Result<(), SqlxError> {
        // Hanya admin yang bisa menonaktifkan pengguna
        if requesting_user.permissions.iter().any(|p| p == "user:write") {
            self.db.deactivate_user(target_user_id).await
        } else {
            Err(SqlxError::RowNotFound) // Forbidden
        }
    }
}
```

## 7. Integrasi dengan Sistem Lain

### 7.1. Hubungan dengan Queue dan Job

Organisasi dan proyek menjadi dasar dari sistem queue dan job:

```rust
// Ilustrasi hubungan antar entitas
/*
Organization (1) 
    |-> User (Many)
    |-> Project (Many)
    
Project (1)
    |-> JobQueue (Many)
    |-> Worker (Many)
    |-> ApiKey (Many)
    
JobQueue (1)
    |-> Job (Many)
    
Job (1)
    |-> JobExecution (Many)
    |-> ExecutionLog (Many)
*/
```

### 7.2. Audit Trail

Semua operasi organisasi dan proyek harus di-log untuk keperluan audit:

```rust
// File: src/services/audit_service.rs
use crate::database::Database;
use uuid::Uuid;
use serde_json::Value;
use chrono::{DateTime, Utc};

pub struct AuditService {
    db: Database,
}

impl AuditService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn log_organization_action(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        details: Option<Value>,
    ) -> Result<(), sqlx::Error> {
        self.db.insert_audit_log(user_id, organization_id, action, resource_type, resource_id, details).await
    }
}
```

## 8. Best Practices dan Pertimbangan

### 8.1. Praktik Terbaik

1. **Validasi Input Ketat**: Selalu validasi data organisasi dan proyek sebelum menyimpan
2. **Isolasi Data**: Pastikan data organisasi terisolasi dengan benar
3. **Pembatasan Penggunaan**: Terapkan pembatasan berbasis tier organisasi
4. **Audit Trail**: Log semua perubahan organisasi dan proyek
5. **Keamanan**: Gunakan otorisasi yang tepat untuk setiap operasi

### 8.2. Pertimbangan Kinerja

1. **Indeks Database**: Gunakan indeks pada kolom `organization_id` dan `project_id`
2. **Caching**: Cache informasi organisasi yang sering diakses
3. **Pagination**: Terapkan pagination untuk endpoint yang mengembalikan banyak data
4. **Batch Operations**: Gunakan operasi batch untuk efisiensi

### 8.3. Pengujian

1. **Uji Isolasi Tenant**: Pastikan data tidak bocor antar organisasi
2. **Uji Pembatasan Penggunaan**: Verifikasi bahwa batasan tier diterapkan dengan benar
3. **Uji Otorisasi**: Pastikan hanya pengguna yang berwenang yang dapat mengakses sumber daya
4. **Uji Kinerja**: Uji sistem dengan beban organisasi dan proyek yang tinggi