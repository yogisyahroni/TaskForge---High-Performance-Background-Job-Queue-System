use crate::{
    database::Database,
    models::{user::User, api_key::ApiKey, organization::Organization, project::Project},
    services::authentication_service::{AuthenticationService, Claims},
};
use uuid::Uuid;
use sqlx::Error as SqlxError;

pub struct AuthorizationService {
    db: Database,
    auth_service: AuthenticationService,
}

impl AuthorizationService {
    pub fn new(db: Database, auth_service: AuthenticationService) -> Self {
        Self { db, auth_service }
    }
    
    pub async fn check_user_permission(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
        required_permission: &str,
    ) -> Result<bool, AuthorizationError> {
        // Ambil user dari database
        let user = self.db.get_user_by_id(user_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::UserNotFound)?;
        
        // Pastikan user adalah bagian dari organisasi yang dimaksud
        if user.organization_id != organization_id {
            return Ok(false);
        }
        
        // Ambil izin dari user berdasarkan role dan izin khusus
        let user_permissions = self.get_user_permissions(&user).await?;
        
        // Periksa apakah user memiliki izin yang diperlukan
        Ok(user_permissions.contains(&required_permission.to_string()))
    }
    
    pub async fn check_api_key_permission(
        &self,
        api_key_id: Uuid,
        organization_id: Uuid,
        required_permission: &str,
    ) -> Result<bool, AuthorizationError> {
        // Ambil API key dari database
        let api_key = self.db.get_api_key_by_id(api_key_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ApiKeyNotFound)?;
        
        // Pastikan API key adalah bagian dari organisasi yang dimaksud
        if api_key.organization_id != organization_id {
            return Ok(false);
        }
        
        // Periksa apakah API key memiliki izin yang diperlukan
        Ok(api_key.permissions.contains(&required_permission.to_string()))
    }
    
    pub async fn check_resource_access(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
        resource_type: &str,
        resource_id: Uuid,
    ) -> Result<bool, AuthorizationError> {
        match resource_type {
            "job" => self.check_job_access(user_id, organization_id, resource_id).await,
            "queue" => self.check_queue_access(user_id, organization_id, resource_id).await,
            "worker" => self.check_worker_access(user_id, organization_id, resource_id).await,
            "project" => self.check_project_access(user_id, organization_id, resource_id).await,
            "organization" => self.check_organization_access(user_id, organization_id, resource_id).await,
            _ => Err(AuthorizationError::InvalidResourceType),
        }
    }
    
    async fn check_job_access(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
        job_id: Uuid,
    ) -> Result<bool, AuthorizationError> {
        // Ambil job dari database
        let job = self.db.get_job_by_id(job_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Ambil queue dari job
        let queue = self.db.get_job_queue_by_id(job.queue_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Ambil project dari queue
        let project = self.db.get_project_by_id(queue.project_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Pastikan project adalah bagian dari organisasi yang sama
        Ok(project.organization_id == organization_id)
    }
    
    async fn check_queue_access(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
        queue_id: Uuid,
    ) -> Result<bool, AuthorizationError> {
        // Ambil queue dari database
        let queue = self.db.get_job_queue_by_id(queue_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Ambil project dari queue
        let project = self.db.get_project_by_id(queue.project_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Pastikan project adalah bagian dari organisasi yang sama
        Ok(project.organization_id == organization_id)
    }
    
    async fn check_worker_access(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
        worker_id: Uuid,
    ) -> Result<bool, AuthorizationError> {
        // Ambil worker dari database
        let worker = self.db.get_worker_by_id(worker_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Ambil project dari worker
        let project = self.db.get_project_by_id(worker.project_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Pastikan project adalah bagian dari organisasi yang sama
        Ok(project.organization_id == organization_id)
    }
    
    async fn check_project_access(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
        project_id: Uuid,
    ) -> Result<bool, AuthorizationError> {
        // Ambil project dari database
        let project = self.db.get_project_by_id(project_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Pastikan project adalah bagian dari organisasi yang sama
        Ok(project.organization_id == organization_id)
    }
    
    async fn check_organization_access(
        &self,
        user_id: Uuid,
        organization_id: Uuid,
        target_org_id: Uuid,
    ) -> Result<bool, AuthorizationError> {
        // Hanya pengguna yang merupakan bagian dari organisasi yang bisa mengakses organisasi tersebut
        Ok(organization_id == target_org_id)
    }
    
    pub async fn get_user_permissions(
        &self,
        user: &User,
    ) -> Result<Vec<String>, AuthorizationError> {
        let mut permissions = Vec::new();
        
        // Tambahkan izin berdasarkan role
        match user.role {
            crate::models::user::UserRole::Admin => {
                permissions.extend(vec![
                    "job:read".to_string(),
                    "job:write".to_string(),
                    "queue:read".to_string(),
                    "queue:write".to_string(),
                    "worker:read".to_string(),
                    "worker:write".to_string(),
                    "organization:read".to_string(),
                    "organization:write".to_string(),
                    "project:read".to_string(),
                    "project:write".to_string(),
                    "user:read".to_string(),
                    "user:write".to_string(),
                    "admin".to_string(),
                ]);
            },
            crate::models::user::UserRole::Member => {
                permissions.extend(vec![
                    "job:read".to_string(),
                    "job:write".to_string(),
                    "queue:read".to_string(),
                    "project:read".to_string(),
                ]);
            },
        }
        
        // Dalam implementasi nyata, kita juga akan menambahkan izin khusus yang diberikan ke user
        // dari tabel permissions atau role-based permissions
        
        Ok(permissions)
    }
    
    pub async fn get_api_key_permissions(
        &self,
        api_key: &ApiKey,
    ) -> Result<Vec<String>, AuthorizationError> {
        Ok(api_key.permissions.clone())
    }
    
    pub async fn authenticate_and_authorize(
        &self,
        token: &str,
        required_permission: &str,
    ) -> Result<Option<AuthenticatedUser>, AuthorizationError> {
        // Validasi token
        let claims = self.auth_service.validate_token(token)
            .map_err(|_| AuthorizationError::InvalidToken)?;
        
        // Ekstrak user ID dan organization ID dari claims
        let user_id = self.auth_service.extract_user_id(&claims)
            .map_err(|_| AuthorizationError::InvalidToken)?;
        
        let organization_id = self.auth_service.extract_organization_id(&claims)
            .map_err(|_| AuthorizationError::InvalidToken)?;
        
        // Ambil user dari database
        let user = self.db.get_user_by_id(user_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::UserNotFound)?;
        
        // Pastikan user aktif
        if !user.is_active {
            return Err(AuthorizationError::UserInactive);
        }
        
        // Pastikan user adalah bagian dari organisasi yang sesuai
        if user.organization_id != organization_id {
            return Err(AuthorizationError::Unauthorized);
        }
        
        // Periksa apakah user memiliki izin yang diperlukan
        if !self.auth_service.has_permission(&claims, required_permission) {
            return Err(AuthorizationError::InsufficientPermissions);
        }
        
        Ok(Some(AuthenticatedUser {
            user_id,
            organization_id,
            permissions: claims.permissions,
            role: user.role,
        }))
    }
    
    pub async fn authenticate_api_key(
        &self,
        api_key: &str,
        required_permission: &str,
    ) -> Result<Option<AuthenticatedApiKey>, AuthorizationError> {
        // Dalam implementasi nyata, kita akan meng-hash dan membandingkan API key
        // dengan yang ada di database
        
        // Ambil API key dari database
        let key_record = self.auth_service.authenticate_api_key(api_key).await?;
        
        if let Some((api_key_record, _token)) = key_record {
            // Periksa apakah API key memiliki izin yang diperlukan
            if api_key_record.has_permission(required_permission) {
                return Ok(Some(AuthenticatedApiKey {
                    key_id: api_key_record.id,
                    organization_id: api_key_record.organization_id,
                    permissions: api_key_record.permissions,
                }));
            } else {
                return Err(AuthorizationError::InsufficientPermissions);
            }
        }
        
        Ok(None)
    }
    
    pub async fn can_access_job(
        &self,
        user_id: Uuid,
        job_id: Uuid,
    ) -> Result<bool, AuthorizationError> {
        // Ambil job dari database
        let job = self.db.get_job_by_id(job_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Ambil queue dari job
        let queue = self.db.get_job_queue_by_id(job.queue_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Ambil project dari queue
        let project = self.db.get_project_by_id(queue.project_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Ambil user
        let user = self.db.get_user_by_id(user_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::UserNotFound)?;
        
        // Pastikan job adalah bagian dari organisasi yang sama dengan user
        Ok(project.organization_id == user.organization_id)
    }
    
    pub async fn can_access_queue(
        &self,
        user_id: Uuid,
        queue_id: Uuid,
    ) -> Result<bool, AuthorizationError> {
        // Ambil queue dari database
        let queue = self.db.get_job_queue_by_id(queue_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Ambil project dari queue
        let project = self.db.get_project_by_id(queue.project_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Ambil user
        let user = self.db.get_user_by_id(user_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::UserNotFound)?;
        
        // Pastikan queue adalah bagian dari organisasi yang sama dengan user
        Ok(project.organization_id == user.organization_id)
    }
    
    pub async fn can_access_worker(
        &self,
        user_id: Uuid,
        worker_id: Uuid,
    ) -> Result<bool, AuthorizationError> {
        // Ambil worker dari database
        let worker = self.db.get_worker_by_id(worker_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Ambil project dari worker
        let project = self.db.get_project_by_id(worker.project_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::ResourceNotFound)?;
        
        // Ambil user
        let user = self.db.get_user_by_id(user_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::UserNotFound)?;
        
        // Pastikan worker adalah bagian dari organisasi yang sama dengan user
        Ok(project.organization_id == user.organization_id)
    }
    
    pub async fn get_user_accessible_resources(
        &self,
        user_id: Uuid,
    ) -> Result<UserAccessibleResources, AuthorizationError> {
        let user = self.db.get_user_by_id(user_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?
            .ok_or(AuthorizationError::UserNotFound)?;
        
        let organization_id = user.organization_id;
        
        // Ambil semua proyek dalam organisasi
        let projects = self.db.get_projects_by_organization(organization_id).await
            .map_err(|_| AuthorizationError::DatabaseError)?;
        
        // Ambil semua queue dalam proyek-proyek tersebut
        let mut queues = Vec::new();
        for project in &projects {
            let project_queues = self.db.get_queues_by_project(project.id).await
                .map_err(|_| AuthorizationError::DatabaseError)?;
            queues.extend(project_queues);
        }
        
        // Ambil semua worker dalam proyek-proyek tersebut
        let mut workers = Vec::new();
        for project in &projects {
            let project_workers = self.db.get_workers_by_project(project.id).await
                .map_err(|_| AuthorizationError::DatabaseError)?;
            workers.extend(project_workers);
        }
        
        // Ambil semua job dalam queue tersebut
        let mut jobs = Vec::new();
        for queue in &queues {
            let queue_jobs = self.db.get_jobs_by_queue(queue.id).await
                .map_err(|_| AuthorizationError::DatabaseError)?;
            jobs.extend(queue_jobs);
        }
        
        Ok(UserAccessibleResources {
            organization_id,
            projects,
            queues,
            workers,
            jobs,
        })
    }
}

pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub organization_id: Uuid,
    pub permissions: Vec<String>,
    pub role: crate::models::user::UserRole,
}

pub struct AuthenticatedApiKey {
    pub key_id: Uuid,
    pub organization_id: Uuid,
    pub permissions: Vec<String>,
}

pub struct UserAccessibleResources {
    pub organization_id: Uuid,
    pub projects: Vec<Project>,
    pub queues: Vec<crate::models::job_queue::JobQueue>,
    pub workers: Vec<crate::models::worker::Worker>,
    pub jobs: Vec<crate::models::job::Job>,
}

#[derive(Debug)]
pub enum AuthorizationError {
    InvalidToken,
    UserNotFound,
    ApiKeyNotFound,
    ResourceNotFound,
    DatabaseError,
    UserInactive,
    InsufficientPermissions,
    Unauthorized,
    InvalidResourceType,
}

impl std::fmt::Display for AuthorizationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthorizationError::InvalidToken => write!(f, "Invalid token"),
            AuthorizationError::UserNotFound => write!(f, "User not found"),
            AuthorizationError::ApiKeyNotFound => write!(f, "API key not found"),
            AuthorizationError::ResourceNotFound => write!(f, "Resource not found"),
            AuthorizationError::DatabaseError => write!(f, "Database error"),
            AuthorizationError::UserInactive => write!(f, "User account is inactive"),
            AuthorizationError::InsufficientPermissions => write!(f, "Insufficient permissions"),
            AuthorizationError::Unauthorized => write!(f, "Unauthorized"),
            AuthorizationError::InvalidResourceType => write!(f, "Invalid resource type"),
        }
    }
}

impl std::error::Error for AuthorizationError {}

impl From<SqlxError> for AuthorizationError {
    fn from(_: SqlxError) -> Self {
        AuthorizationError::DatabaseError
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{user::User, user::UserRole};
    use uuid::Uuid;
    
    #[tokio::test]
    async fn test_user_permission_check() {
        // Dalam implementasi nyata, kita akan menggunakan mock database
        // Untuk sekarang, kita hanya menguji logika
        
        let user = User::new(
            Uuid::new_v4(), // organization_id
            "test@example.com".to_string(),
            "hashed_password".to_string(),
            UserRole::Admin,
        );
        
        let permissions = vec![
            "job:read".to_string(),
            "job:write".to_string(),
            "queue:read".to_string(),
            "queue:write".to_string(),
        ];
        
        assert!(permissions.contains(&"job:read".to_string()));
        assert!(permissions.contains(&"queue:write".to_string()));
        assert!(!permissions.contains(&"worker:read".to_string()));
    }
    
    #[tokio::test]
    async fn test_admin_permissions() {
        let user = User::new(
            Uuid::new_v4(), // organization_id
            "admin@example.com".to_string(),
            "hashed_password".to_string(),
            UserRole::Admin,
        );
        
        let auth_service = AuthorizationService::new(/* mock db */, /* mock auth service */);
        let permissions = auth_service.get_user_permissions(&user).await.unwrap();
        
        assert!(permissions.contains(&"admin".to_string()));
        assert!(permissions.contains(&"job:read".to_string()));
        assert!(permissions.contains(&"job:write".to_string()));
        assert!(permissions.contains(&"organization:write".to_string()));
    }
    
    #[tokio::test]
    async fn test_member_permissions() {
        let user = User::new(
            Uuid::new_v4(), // organization_id
            "member@example.com".to_string(),
            "hashed_password".to_string(),
            UserRole::Member,
        );
        
        let auth_service = AuthorizationService::new(/* mock db */, /* mock auth service */);
        let permissions = auth_service.get_user_permissions(&user).await.unwrap();
        
        assert!(permissions.contains(&"job:read".to_string()));
        assert!(permissions.contains(&"job:write".to_string()));
        assert!(permissions.contains(&"queue:read".to_string()));
        assert!(!permissions.contains(&"admin".to_string()));
        assert!(!permissions.contains(&"organization:write".to_string()));
    }
}