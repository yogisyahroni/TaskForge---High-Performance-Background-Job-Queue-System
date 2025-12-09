use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use secrecy::{Secret, SecretString};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "api_key_scope", rename_all = "snake_case")]
pub enum ApiKeyScope {
    JobRead,
    JobWrite,
    QueueRead,
    QueueWrite,
    WorkerRead,
    WorkerWrite,
    OrganizationRead,
    OrganizationWrite,
    ProjectRead,
    ProjectWrite,
    UserRead,
    UserWrite,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub project_id: Uuid,
    pub organization_id: Uuid,  // Menambahkan field untuk multi-tenant
    pub name: String,
    pub key_hash: String,       // Hash dari API key (tidak menyimpan key dalam bentuk plaintext)
    pub permissions: Vec<String>, // Izin akses dalam bentuk string
    pub created_by: Uuid,      // ID user yang membuat API key
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub metadata: serde_json::Value, // JSONB field untuk metadata tambahan
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ApiKey {
    pub fn new(
        project_id: Uuid,
        name: String,
        key_hash: String,
        permissions: Vec<String>,
        created_by: Uuid,
        organization_id: Uuid,
        expires_at: Option<DateTime<Utc>>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            organization_id,
            name,
            key_hash,
            permissions,
            created_by,
            last_used_at: None,
            expires_at,
            is_active: true,
            metadata: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("API key name must be between 1 and 100 characters".to_string());
        }
        
        if self.permissions.is_empty() {
            return Err("API key must have at least one permission".to_string());
        }
        
        // Validasi format permission
        for permission in &self.permissions {
            if !Self::is_valid_permission_format(permission) {
                return Err(format!("Invalid permission format: {}", permission));
            }
        }
        
        if let Some(expires_at) = self.expires_at {
            if expires_at <= Utc::now() {
                return Err("API key expiration time must be in the future".to_string());
            }
        }
        
        Ok(())
    }
    
    fn is_valid_permission_format(permission: &str) -> bool {
        // Format permission harus mengikuti pola resource:action
        let parts: Vec<&str> = permission.split(':').collect();
        if parts.len() != 2 {
            return false;
        }
        
        let resource = parts[0];
        let action = parts[1];
        
        // Validasi karakter yang diizinkan
        resource.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') &&
        action.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    }
    
    pub fn has_permission(&self, required_permission: &str) -> bool {
        self.permissions.contains(&required_permission.to_string()) || 
        self.permissions.contains(&"admin".to_string())
    }
    
    pub fn has_any_permission(&self, required_permissions: &[&str]) -> bool {
        required_permissions.iter().any(|&perm| self.has_permission(perm))
    }
    
    pub fn has_all_permissions(&self, required_permissions: &[&str]) -> bool {
        required_permissions.iter().all(|&perm| self.has_permission(perm))
    }
    
    pub fn is_valid(&self) -> bool {
        if !self.is_active {
            return false;
        }
        
        if let Some(expires_at) = self.expires_at {
            if Utc::now() > expires_at {
                return false;
            }
        }
        
        true
    }
    
    pub fn mark_as_used(&mut self) {
        self.last_used_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.updated_at = Utc::now();
    }
    
    pub fn activate(&mut self) {
        self.is_active = true;
        self.updated_at = Utc::now();
    }
    
    pub fn can_access_resource(&self, resource_org_id: Uuid) -> bool {
        self.organization_id == resource_org_id
    }
    
    pub fn get_permission_level(&self, resource: &str) -> PermissionLevel {
        if self.permissions.contains(&format!("{}:admin", resource)) {
            PermissionLevel::Admin
        } else if self.permissions.contains(&format!("{}:write", resource)) {
            PermissionLevel::Write
        } else if self.permissions.contains(&format!("{}:read", resource)) {
            PermissionLevel::Read
        } else {
            PermissionLevel::None
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PermissionLevel {
    None,
    Read,
    Write,
    Admin,
}

impl PermissionLevel {
    pub fn can_read(&self) -> bool {
        matches!(self, PermissionLevel::Read | PermissionLevel::Write | PermissionLevel::Admin)
    }
    
    pub fn can_write(&self) -> bool {
        matches!(self, PermissionLevel::Write | PermissionLevel::Admin)
    }
    
    pub fn is_admin(&self) -> bool {
        matches!(self, PermissionLevel::Admin)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyCreationRequest {
    pub name: String,
    pub permissions: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyCreationResponse {
    pub id: Uuid,
    pub name: String,
    pub key: String,  // Key yang akan ditampilkan sekali saat pembuatan
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyVerificationResult {
    pub is_valid: bool,
    pub api_key: Option<ApiKey>,
    pub error: Option<String>,
}

impl ApiKeyVerificationResult {
    pub fn new(is_valid: bool, api_key: Option<ApiKey>, error: Option<String>) -> Self {
        Self {
            is_valid,
            api_key,
            error,
        }
    }
    
    pub fn valid(api_key: ApiKey) -> Self {
        Self::new(true, Some(api_key), None)
    }
    
    pub fn invalid(error: String) -> Self {
        Self::new(false, None, Some(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;
    
    #[test]
    fn test_api_key_validation_success() {
        let api_key = ApiKey::new(
            Uuid::new_v4(),
            "test_key".to_string(),
            "hashed_key".to_string(),
            vec!["job:read".to_string(), "job:write".to_string()],
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
        );
        
        assert!(api_key.validate().is_ok());
    }
    
    #[test]
    fn test_api_key_validation_invalid_permission_format() {
        let api_key = ApiKey::new(
            Uuid::new_v4(),
            "test_key".to_string(),
            "hashed_key".to_string(),
            vec!["invalid_permission".to_string()], // Tidak mengikuti format resource:action
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
        );
        
        assert!(api_key.validate().is_err());
    }
    
    #[test]
    fn test_api_key_validation_empty_name() {
        let api_key = ApiKey::new(
            Uuid::new_v4(),
            "".to_string(), // Nama kosong
            "hashed_key".to_string(),
            vec!["job:read".to_string()],
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
        );
        
        assert!(api_key.validate().is_err());
    }
    
    #[test]
    fn test_api_key_has_permission() {
        let api_key = ApiKey::new(
            Uuid::new_v4(),
            "test_key".to_string(),
            "hashed_key".to_string(),
            vec!["job:read".to_string(), "queue:write".to_string()],
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
        );
        
        assert!(api_key.has_permission("job:read"));
        assert!(api_key.has_permission("queue:write"));
        assert!(!api_key.has_permission("worker:read"));
    }
    
    #[test]
    fn test_api_key_has_permission_admin() {
        let api_key = ApiKey::new(
            Uuid::new_v4(),
            "admin_key".to_string(),
            "hashed_key".to_string(),
            vec!["admin".to_string()],
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
        );
        
        assert!(api_key.has_permission("any:permission"));
    }
    
    #[test]
    fn test_api_key_is_valid() {
        let api_key = ApiKey::new(
            Uuid::new_v4(),
            "valid_key".to_string(),
            "hashed_key".to_string(),
            vec!["job:read".to_string()],
            Uuid::new_v4(),
            Uuid::new_v4(),
            Some(Utc::now() + chrono::Duration::days(1)), // Berlaku 1 hari ke depan
        );
        
        assert!(api_key.is_valid());
    }
    
    #[test]
    fn test_api_key_expired() {
        let mut api_key = ApiKey::new(
            Uuid::new_v4(),
            "expired_key".to_string(),
            "hashed_key".to_string(),
            vec!["job:read".to_string()],
            Uuid::new_v4(),
            Uuid::new_v4(),
            Some(Utc::now() - chrono::Duration::days(1)), // Kadaluarsa 1 hari yang lalu
        );
        
        assert!(!api_key.is_valid());
    }
    
    #[test]
    fn test_api_key_inactive() {
        let mut api_key = ApiKey::new(
            Uuid::new_v4(),
            "inactive_key".to_string(),
            "hashed_key".to_string(),
            vec!["job:read".to_string()],
            Uuid::new_v4(),
            Uuid::new_v4(),
            Some(Utc::now() + chrono::Duration::days(1)),
        );
        
        api_key.is_active = false;
        
        assert!(!api_key.is_valid());
    }
    
    #[test]
    fn test_permission_level() {
        let api_key = ApiKey::new(
            Uuid::new_v4(),
            "level_test_key".to_string(),
            "hashed_key".to_string(),
            vec!["job:write".to_string()],
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
        );
        
        let level = api_key.get_permission_level("job");
        
        assert!(level.can_read());
        assert!(level.can_write());
        assert!(!level.is_admin());
    }
}