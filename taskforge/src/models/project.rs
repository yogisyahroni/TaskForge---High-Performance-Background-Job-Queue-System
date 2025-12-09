use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Project {
    pub id: Uuid,
    pub organization_id: Uuid,  // ID organisasi untuk multi-tenant
    pub name: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub metadata: Value,        // JSONB field untuk metadata tambahan
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    pub fn new(
        organization_id: Uuid,
        name: String,
        description: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            organization_id,
            name,
            description,
            is_active: true,
            metadata: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("Project name must be between 1 and 100 characters".to_string());
        }
        
        if let Some(description) = &self.description {
            if description.len() > 500 {
                return Err("Project description cannot exceed 500 characters".to_string());
            }
        }
        
        Ok(())
    }
    
    pub fn can_be_used(&self) -> bool {
        self.is_active
    }
    
    pub fn update_name(&mut self, new_name: String) -> Result<(), String> {
        if new_name.is_empty() || new_name.len() > 100 {
            return Err("Project name must be between 1 and 100 characters".to_string());
        }
        
        self.name = new_name;
        self.updated_at = Utc::now();
        Ok(())
    }
    
    pub fn update_description(&mut self, new_description: Option<String>) -> Result<(), String> {
        if let Some(description) = &new_description {
            if description.len() > 500 {
                return Err("Project description cannot exceed 500 characters".to_string());
            }
        }
        
        self.description = new_description;
        self.updated_at = Utc::now();
        Ok(())
    }
    
    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.updated_at = Utc::now();
    }
    
    pub fn activate(&mut self) {
        self.is_active = true;
        self.updated_at = Utc::now();
    }
    
    pub fn belongs_to_organization(&self, organization_id: Uuid) -> bool {
        self.organization_id == organization_id
    }
    
    pub fn get_queue_count(&self, db_connection: &PgPool) -> Result<i64, sqlx::Error> {
        use crate::schema::job_queues::dsl::*;
        use diesel::prelude::*;
        
        let count = job_queues
            .filter(project_id.eq(self.id))
            .count()
            .get_result::<i64>(db_connection)?;
        
        Ok(count)
    }
    
    pub fn get_job_count(&self, db_connection: &PgPool) -> Result<i64, sqlx::Error> {
        use crate::schema::jobs::dsl::*;
        use diesel::prelude::*;
        
        let count = jobs
            .filter(project_id.eq(self.id))
            .count()
            .get_result::<i64>(db_connection)?;
        
        Ok(count)
    }
    
    pub fn get_worker_count(&self, db_connection: &PgPool) -> Result<i64, sqlx::Error> {
        use crate::schema::workers::dsl::*;
        use diesel::prelude::*;
        
        let count = workers
            .filter(project_id.eq(self.id))
            .count()
            .get_result::<i64>(db_connection)?;
        
        Ok(count)
    }
    
    pub fn get_api_key_count(&self, db_connection: &PgPool) -> Result<i64, sqlx::Error> {
        use crate::schema::api_keys::dsl::*;
        use diesel::prelude::*;
        
        let count = api_keys
            .filter(project_id.eq(self.id))
            .count()
            .get_result::<i64>(db_connection)?;
        
        Ok(count)
    }
    
    pub fn get_usage_stats(&self, db_connection: &PgPool) -> Result<ProjectUsageStats, sqlx::Error> {
        Ok(ProjectUsageStats {
            project_id: self.id,
            queue_count: self.get_queue_count(db_connection)?,
            job_count: self.get_job_count(db_connection)?,
            worker_count: self.get_worker_count(db_connection)?,
            api_key_count: self.get_api_key_count(db_connection)?,
        })
    }
    
    pub fn is_within_organization_limits(
        &self,
        organization: &Organization,
        db_connection: &PgPool,
    ) -> Result<bool, sqlx::Error> {
        let current_usage = self.get_usage_stats(db_connection)?;
        let org_limits = organization.get_usage_limits();
        
        Ok(
            current_usage.queue_count < org_limits.max_queues as i64 &&
            current_usage.worker_count < org_limits.max_workers as i64
        )
    }
}

pub struct ProjectUsageStats {
    pub project_id: Uuid,
    pub queue_count: i64,
    pub job_count: i64,
    pub worker_count: i64,
    pub api_key_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectSettings {
    pub project_id: Uuid,
    pub default_queue_priority: i32,
    pub default_job_priority: i32,
    pub default_max_retries: i32,
    pub default_timeout_seconds: i64,
    pub enable_metrics: bool,
    pub enable_logging: bool,
    pub dead_letter_queue_id: Option<Uuid>,
    pub rate_limit_per_minute: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProjectSettings {
    pub fn new(project_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            project_id,
            default_queue_priority: 5,
            default_job_priority: 5,
            default_max_retries: 3,
            default_timeout_seconds: 300,
            enable_metrics: true,
            enable_logging: true,
            dead_letter_queue_id: None,
            rate_limit_per_minute: Some(1000),
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.default_queue_priority < 0 || self.default_queue_priority > 9 {
            return Err("Default queue priority must be between 0 and 9".to_string());
        }
        
        if self.default_job_priority < 0 || self.default_job_priority > 9 {
            return Err("Default job priority must be between 0 and 9".to_string());
        }
        
        if self.default_max_retries < 1 || self.default_max_retries > 10 {
            return Err("Default max retries must be between 1 and 10".to_string());
        }
        
        if self.default_timeout_seconds < 1 || self.default_timeout_seconds > 86400 {
            return Err("Default timeout must be between 1 second and 24 hours".to_string());
        }
        
        Ok(())
    }
    
    pub fn apply_defaults_to_job(&self, job: &mut Job) {
        if job.priority == 0 {
            job.priority = self.default_job_priority;
        }
        
        if job.max_attempts == 0 {
            job.max_attempts = self.default_max_retries;
        }
        
        // Set timeout if not already set
        if job.timeout_at.is_none() {
            job.timeout_at = Some(Utc::now() + chrono::Duration::seconds(self.default_timeout_seconds));
        }
    }
    
    pub fn apply_defaults_to_queue(&self, queue: &mut JobQueue) {
        if queue.priority == 0 {
            queue.priority = self.default_queue_priority;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProjectApiKey {
    pub id: Uuid,
    pub project_id: Uuid,
    pub api_key_id: Uuid,
    pub role: ProjectApiKeyRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "project_api_key_role", rename_all = "lowercase")]
pub enum ProjectApiKeyRole {
    Viewer,
    Editor,
    Admin,
}

impl ProjectApiKey {
    pub fn new(project_id: Uuid, api_key_id: Uuid, role: ProjectApiKeyRole) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            api_key_id,
            role,
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        // Dalam implementasi nyata, ini akan memvalidasi apakah API key dan project valid
        Ok(())
    }
    
    pub fn can_read(&self) -> bool {
        matches!(self.role, ProjectApiKeyRole::Viewer | ProjectApiKeyRole::Editor | ProjectApiKeyRole::Admin)
    }
    
    pub fn can_write(&self) -> bool {
        matches!(self.role, ProjectApiKeyRole::Editor | ProjectApiKeyRole::Admin)
    }
    
    pub fn is_admin(&self) -> bool {
        matches!(self.role, ProjectApiKeyRole::Admin)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectFilter {
    pub organization_id: Option<Uuid>,
    pub is_active: Option<bool>,
    pub name_pattern: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

impl ProjectFilter {
    pub fn new() -> Self {
        Self {
            organization_id: None,
            is_active: None,
            name_pattern: None,
            limit: None,
            offset: None,
        }
    }
    
    pub fn with_organization(mut self, org_id: Uuid) -> Self {
        self.organization_id = Some(org_id);
        self
    }
    
    pub fn with_active_status(mut self, active: bool) -> Self {
        self.is_active = Some(active);
        self
    }
    
    pub fn with_name_pattern(mut self, pattern: &str) -> Self {
        self.name_pattern = Some(pattern.to_string());
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
    use uuid::Uuid;
    
    #[test]
    fn test_project_creation() {
        let project = Project::new(
            Uuid::new_v4(),
            "Test Project".to_string(),
            Some("Test project description".to_string()),
        );
        
        assert!(!project.id.is_nil());
        assert_eq!(project.name, "Test Project");
        assert_eq!(project.description, Some("Test project description".to_string()));
        assert!(project.is_active);
        assert!(project.created_at <= Utc::now());
    }
    
    #[test]
    fn test_project_validation_success() {
        let project = Project::new(
            Uuid::new_v4(),
            "Valid Project Name".to_string(),
            Some("Valid description".to_string()),
        );
        
        assert!(project.validate().is_ok());
    }
    
    #[test]
    fn test_project_validation_invalid_name() {
        let project = Project::new(
            Uuid::new_v4(),
            "".to_string(), // Nama kosong
            None,
        );
        
        assert!(project.validate().is_err());
        
        let project = Project::new(
            Uuid::new_v4(),
            "A".repeat(101), // Nama terlalu panjang
            None,
        );
        
        assert!(project.validate().is_err());
    }
    
    #[test]
    fn test_project_validation_invalid_description() {
        let project = Project::new(
            Uuid::new_v4(),
            "Valid Name".to_string(),
            Some("A".repeat(501)), // Deskripsi terlalu panjang
        );
        
        assert!(project.validate().is_err());
    }
    
    #[test]
    fn test_project_activation() {
        let mut project = Project::new(
            Uuid::new_v4(),
            "Test Project".to_string(),
            None,
        );
        
        assert!(project.is_active);
        
        project.deactivate();
        assert!(!project.is_active);
        
        project.activate();
        assert!(project.is_active);
    }
    
    #[test]
    fn test_project_belongs_to_organization() {
        let org_id = Uuid::new_v4();
        let other_org_id = Uuid::new_v4();
        
        let project = Project::new(
            org_id,
            "Test Project".to_string(),
            None,
        );
        
        assert!(project.belongs_to_organization(org_id));
        assert!(!project.belongs_to_organization(other_org_id));
    }
    
    #[test]
    fn test_project_settings_creation() {
        let project_id = Uuid::new_v4();
        let settings = ProjectSettings::new(project_id);
        
        assert_eq!(settings.project_id, project_id);
        assert_eq!(settings.default_queue_priority, 5);
        assert_eq!(settings.default_job_priority, 5);
        assert_eq!(settings.default_max_retries, 3);
        assert_eq!(settings.default_timeout_seconds, 300);
        assert!(settings.enable_metrics);
        assert!(settings.enable_logging);
        assert_eq!(settings.dead_letter_queue_id, None);
        assert_eq!(settings.rate_limit_per_minute, Some(1000));
    }
    
    #[test]
    fn test_project_settings_validation() {
        let mut settings = ProjectSettings::new(Uuid::new_v4());
        
        // Test valid settings
        settings.default_queue_priority = 5;
        settings.default_job_priority = 7;
        settings.default_max_retries = 5;
        settings.default_timeout_seconds = 600;
        
        assert!(settings.validate().is_ok());
        
        // Test invalid priority
        settings.default_queue_priority = 15;
        assert!(settings.validate().is_err());
        
        settings.default_queue_priority = 5; // Reset
        settings.default_job_priority = -1;
        assert!(settings.validate().is_err());
    }
    
    #[test]
    fn test_project_api_key_role_permissions() {
        let viewer_key = ProjectApiKey::new(Uuid::new_v4(), Uuid::new_v4(), ProjectApiKeyRole::Viewer);
        let editor_key = ProjectApiKey::new(Uuid::new_v4(), Uuid::new_v4(), ProjectApiKeyRole::Editor);
        let admin_key = ProjectApiKey::new(Uuid::new_v4(), Uuid::new_v4(), ProjectApiKeyRole::Admin);
        
        // Viewer tests
        assert!(viewer_key.can_read());
        assert!(!viewer_key.can_write());
        assert!(!viewer_key.is_admin());
        
        // Editor tests
        assert!(editor_key.can_read());
        assert!(editor_key.can_write());
        assert!(!editor_key.is_admin());
        
        // Admin tests
        assert!(admin_key.can_read());
        assert!(admin_key.can_write());
        assert!(admin_key.is_admin());
    }
    
    #[test]
    fn test_project_filter() {
        let filter = ProjectFilter::new()
            .with_organization(Uuid::new_v4())
            .with_active_status(true)
            .with_name_pattern("test")
            .with_pagination(10, 0);
        
        assert!(filter.organization_id.is_some());
        assert_eq!(filter.is_active, Some(true));
        assert_eq!(filter.name_pattern, Some("test".to_string()));
        assert_eq!(filter.limit, Some(10));
        assert_eq!(filter.offset, Some(0));
    }
}