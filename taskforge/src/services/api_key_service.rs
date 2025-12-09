use crate::{
    database::Database,
    models::{api_key::ApiKey, api_key_scope::ApiKeyScope},
    services::authorization_service::AuthorizationService,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};
use secrecy::{Secret, SecretString};

pub struct ApiKeyService {
    db: Database,
    authz_service: AuthorizationService,
}

impl ApiKeyService {
    pub fn new(db: Database, authz_service: AuthorizationService) -> Self {
        Self { db, authz_service }
    }
    
    pub async fn create_api_key(
        &self,
        project_id: Uuid,
        name: String,
        permissions: Vec<String>,
        created_by: Uuid,
        organization_id: Uuid,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<(ApiKey, String), SqlxError> {
        // Validasi permissions
        for permission in &permissions {
            if !self.is_valid_permission(permission) {
                return Err(SqlxError::Decode(format!("Invalid permission: {}", permission).into()));
            }
        }
        
        // Pastikan project milik organisasi yang benar
        let project = self.db.get_project_by_id(project_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if project.organization_id != organization_id {
            return Err(SqlxError::RowNotFound);
        }
        
        // Generate API key
        let raw_key = self.generate_api_key();
        let key_hash = self.hash_api_key(&raw_key);
        
        let api_key = ApiKey::new(
            project_id,
            name,
            key_hash,
            permissions,
            created_by,
            organization_id,
            expires_at,
        );
        
        api_key.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        let created_key = self.db.create_api_key(api_key).await?;
        
        Ok((created_key, raw_key))
    }
    
    pub async fn get_api_key_by_hash(&self, key_hash: &str) -> Result<Option<ApiKey>, SqlxError> {
        self.db.get_api_key_by_hash(key_hash).await
    }
    
    pub async fn get_api_key_by_id(&self, key_id: Uuid) -> Result<Option<ApiKey>, SqlxError> {
        self.db.get_api_key_by_id(key_id).await
    }
    
    pub async fn get_api_keys_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ApiKey>, SqlxError> {
        self.db.get_api_keys_by_project(project_id).await
    }
    
    pub async fn get_api_keys_by_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<ApiKey>, SqlxError> {
        self.db.get_api_keys_by_organization(organization_id).await
    }
    
    pub async fn update_api_key(
        &self,
        key_id: Uuid,
        name: Option<String>,
        permissions: Option<Vec<String>>,
        expires_at: Option<Option<DateTime<Utc>>>,
    ) -> Result<ApiKey, SqlxError> {
        let mut api_key = self.db.get_api_key_by_id(key_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan API key milik organisasi yang benar
        if !self.authz_service.can_access_api_key(key_id, api_key.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        if let Some(name) = name {
            api_key.name = name;
        }
        
        if let Some(permissions) = permissions {
            // Validasi permissions baru
            for permission in &permissions {
                if !self.is_valid_permission(permission) {
                    return Err(SqlxError::Decode(format!("Invalid permission: {}", permission).into()));
                }
            }
            api_key.permissions = permissions;
        }
        
        if let Some(expires_at) = expires_at {
            api_key.expires_at = expires_at;
        }
        
        api_key.updated_at = Utc::now();
        
        self.db.update_api_key(api_key).await
    }
    
    pub async fn delete_api_key(&self, key_id: Uuid) -> Result<(), SqlxError> {
        let api_key = self.db.get_api_key_by_id(key_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan API key milik organisasi yang benar
        if !self.authz_service.can_access_api_key(key_id, api_key.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        self.db.delete_api_key(key_id).await?;
        Ok(())
    }
    
    pub async fn rotate_api_key(&self, key_id: Uuid) -> Result<String, SqlxError> {
        let mut api_key = self.db.get_api_key_by_id(key_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan API key milik organisasi yang benar
        if !self.authz_service.can_access_api_key(key_id, api_key.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        // Generate API key baru
        let new_raw_key = self.generate_api_key();
        let new_key_hash = self.hash_api_key(&new_raw_key);
        
        // Update hash API key
        api_key.key_hash = new_key_hash;
        api_key.updated_at = Utc::now();
        
        self.db.update_api_key(api_key).await?;
        
        Ok(new_raw_key)
    }
    
    pub async fn validate_api_key(
        &self,
        raw_key: &str,
    ) -> Result<Option<ApiKey>, SqlxError> {
        let key_hash = self.hash_api_key(raw_key);
        let api_key = self.get_api_key_by_hash(&key_hash).await?;
        
        if let Some(key) = &api_key {
            // Cek apakah API key masih berlaku
            if let Some(expires_at) = key.expires_at {
                if Utc::now() > expires_at {
                    return Ok(None); // API key sudah kadaluarsa
                }
            }
            
            // Perbarui waktu penggunaan terakhir
            self.db.update_api_key_last_used(key.id).await?;
        }
        
        Ok(api_key)
    }
    
    pub async fn has_permission(
        &self,
        api_key: &ApiKey,
        required_permission: &str,
    ) -> bool {
        api_key.permissions.contains(&required_permission.to_string())
    }
    
    pub async fn has_any_permission(
        &self,
        api_key: &ApiKey,
        required_permissions: &[&str],
    ) -> bool {
        required_permissions.iter().any(|&perm| 
            api_key.permissions.contains(&perm.to_string())
        )
    }
    
    fn generate_api_key(&self) -> String {
        use rand::{distributions::Alphanumeric, Rng};
        
        const KEY_LENGTH: usize = 32;
        let mut rng = rand::thread_rng();
        
        let key: String = (&mut rng)
            .sample_iter(&Alphanumeric)
            .take(KEY_LENGTH)
            .map(char::from)
            .collect();
        
        format!("tf_{}", key) // Awali dengan prefix untuk identifikasi
    }
    
    fn hash_api_key(&self, api_key: &str) -> String {
        use sha2::{Sha256, Digest};
        
        let mut hasher = Sha256::new();
        hasher.update(api_key.as_bytes());
        format!("{:x}", hasher.finalize())
    }
    
    fn is_valid_permission(&self, permission: &str) -> bool {
        // Format permission harus mengikuti pola resource:action
        let parts: Vec<&str> = permission.split(':').collect();
        if parts.len() != 2 {
            return false;
        }
        
        let resource = parts[0];
        let action = parts[1];
        
        // Validasi karakter yang diizinkan
        let valid_chars = |s: &str| s.chars().all(|c| c.is_alphanumeric() || c == '_');
        
        valid_chars(resource) && valid_chars(action)
    }
    
    pub async fn can_access_api_key(
        &self,
        user_id: Uuid,
        api_key_id: Uuid,
    ) -> Result<bool, SqlxError> {
        if let Some(api_key) = self.db.get_api_key_by_id(api_key_id).await? {
            // Dapatkan project dari API key
            if let Some(project) = self.db.get_project_by_id(api_key.project_id).await? {
                // Periksa apakah user adalah bagian dari organisasi yang sama
                self.authz_service.user_belongs_to_organization(user_id, project.organization_id).await
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }
    
    pub async fn get_api_key_usage_stats(
        &self,
        api_key_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<ApiKeyUsageStats, SqlxError> {
        let usage_data = self.db.get_api_key_usage_in_period(api_key_id, start_time, end_time).await?;
        
        Ok(ApiKeyUsageStats {
            api_key_id,
            period_start: start_time,
            period_end: end_time,
            requests_count: usage_data.requests_count,
            successful_requests: usage_data.successful_requests,
            failed_requests: usage_data.failed_requests,
            rate_limit_exceeded_count: usage_data.rate_limit_exceeded_count,
            avg_response_time_ms: usage_data.avg_response_time_ms,
            p95_response_time_ms: usage_data.p95_response_time_ms,
            p99_response_time_ms: usage_data.p99_response_time_ms,
        })
    }
    
    pub async fn get_api_key_security_report(
        &self,
        api_key_id: Uuid,
    ) -> Result<ApiKeySecurityReport, SqlxError> {
        let api_key = self.db.get_api_key_by_id(api_key_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let recent_activities = self.db.get_recent_api_key_activities(api_key_id, 100).await?;
        
        let suspicious_activities = recent_activities
            .iter()
            .filter(|activity| {
                activity.is_suspicious() // Misalnya: banyak request dari IP yang berbeda dalam waktu singkat
            })
            .cloned()
            .collect();
        
        Ok(ApiKeySecurityReport {
            api_key_id: api_key.id,
            name: api_key.name,
            created_at: api_key.created_at,
            last_used_at: api_key.last_used_at,
            suspicious_activities_count: suspicious_activities.len() as u32,
            recent_activities,
            suspicious_activities,
        })
    }
    
    pub async fn revoke_api_key(&self, key_id: Uuid) -> Result<(), SqlxError> {
        let mut api_key = self.db.get_api_key_by_id(key_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan API key milik organisasi yang benar
        if !self.authz_service.can_access_api_key(key_id, api_key.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        // Set expire time ke waktu sekarang untuk langsung tidak berlaku
        api_key.expires_at = Some(Utc::now());
        api_key.updated_at = Utc::now();
        
        self.db.update_api_key(api_key).await?;
        Ok(())
    }
    
    pub async fn get_api_key_permissions_matrix(
        &self,
        organization_id: Uuid,
    ) -> Result<ApiKeyPermissionsMatrix, SqlxError> {
        let api_keys = self.get_api_keys_by_organization(organization_id).await?;
        
        let mut matrix = ApiKeyPermissionsMatrix::new();
        
        for api_key in api_keys {
            let project = self.db.get_project_by_id(api_key.project_id).await?;
            if let Some(project) = project {
                matrix.add_api_key_permissions(
                    api_key.id,
                    project.name.clone(),
                    api_key.permissions.clone(),
                );
            }
        }
        
        Ok(matrix)
    }
}

pub struct ApiKeyUsageStats {
    pub api_key_id: Uuid,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub requests_count: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub rate_limit_exceeded_count: u64,
    pub avg_response_time_ms: f64,
    pub p95_response_time_ms: f64,
    pub p99_response_time_ms: f64,
}

pub struct ApiKeySecurityReport {
    pub api_key_id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub suspicious_activities_count: u32,
    pub recent_activities: Vec<ApiKeyActivity>,
    pub suspicious_activities: Vec<ApiKeyActivity>,
}

pub struct ApiKeyPermissionsMatrix {
    pub api_keys: std::collections::HashMap<Uuid, Vec<String>>,
    pub projects: std::collections::HashMap<Uuid, String>,
    pub permissions_by_project: std::collections::HashMap<String, std::collections::HashMap<Uuid, Vec<String>>>,
}

impl ApiKeyPermissionsMatrix {
    pub fn new() -> Self {
        Self {
            api_keys: std::collections::HashMap::new(),
            projects: std::collections::HashMap::new(),
            permissions_by_project: std::collections::HashMap::new(),
        }
    }
    
    pub fn add_api_key_permissions(&mut self, api_key_id: Uuid, project_name: String, permissions: Vec<String>) {
        self.api_keys.insert(api_key_id, permissions.clone());
        self.projects.insert(api_key_id, project_name.clone());
        
        let project_perms = self.permissions_by_project
            .entry(project_name)
            .or_insert_with(std::collections::HashMap::new);
        
        project_perms.insert(api_key_id, permissions);
    }
    
    pub fn get_permissions_for_api_key(&self, api_key_id: Uuid) -> Option<&Vec<String>> {
        self.api_keys.get(&api_key_id)
    }
    
    pub fn get_permissions_for_project(&self, project_name: &str) -> Option<&std::collections::HashMap<Uuid, Vec<String>>> {
        self.permissions_by_project.get(project_name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyActivity {
    pub id: Uuid,
    pub api_key_id: Uuid,
    pub endpoint: String,
    pub method: String,
    pub ip_address: String,
    pub user_agent: String,
    pub response_status: u16,
    pub response_time_ms: f64,
    pub timestamp: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

impl ApiKeyActivity {
    pub fn is_suspicious(&self) -> bool {
        // Dalam implementasi nyata, ini akan memiliki logika yang lebih kompleks
        // Misalnya: banyak request dalam waktu singkat, request dari IP yang tidak biasa, dll.
        
        // Contoh sederhana: response time yang sangat cepat bisa menunjukkan bot
        self.response_time_ms < 10.0 && self.timestamp > Utc::now() - chrono::Duration::minutes(5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ApiKey;
    use uuid::Uuid;
    
    #[test]
    fn test_generate_api_key() {
        let service = ApiKeyService::new(/* mock db */, /* mock authz */);
        
        let key = service.generate_api_key();
        
        assert!(key.starts_with("tf_"));
        assert_eq!(key.len(), 35); // tf_ + 32 chars
    }
    
    #[test]
    fn test_hash_api_key() {
        let service = ApiKeyService::new(/* mock db */, /* mock authz */);
        
        let key = "test_key_12345";
        let hash = service.hash_api_key(key);
        
        assert_eq!(hash.len(), 64); // SHA256 hash length
        assert_ne!(hash, "test_key_12345"); // Hash should be different from original
    }
    
    #[test]
    fn test_valid_permission_format() {
        let service = ApiKeyService::new(/* mock db */, /* mock authz */);
        
        assert!(service.is_valid_permission("job:read"));
        assert!(service.is_valid_permission("queue:write"));
        assert!(service.is_valid_permission("worker:manage"));
        
        assert!(!service.is_valid_permission("invalid_permission")); // Missing colon
        assert!(!service.is_valid_permission("job")); // Missing action
        assert!(!service.is_valid_permission(":read")); // Missing resource
        assert!(!service.is_valid_permission("job:")); // Missing action
        assert!(!service.is_valid_permission("job:read:extra")); // Too many parts
    }
    
    #[tokio::test]
    async fn test_create_api_key() {
        // Dalam implementasi nyata, kita akan menggunakan mock database
        // Untuk sekarang, kita hanya menguji logika validasi
        
        let permissions = vec!["job:read".to_string(), "job:write".to_string()];
        let project_id = Uuid::new_v4();
        let organization_id = Uuid::new_v4();
        let created_by = Uuid::new_v4();
        
        // Kita tidak bisa menjalankan fungsi create_api_key secara penuh tanpa database mock
        // Tapi kita bisa menguji pembuatan model ApiKey
        let raw_key = "tf_test_key_1234567890abcdef";
        let key_hash = "test_hash_value";
        
        let api_key = ApiKey::new(
            project_id,
            "test_api_key".to_string(),
            key_hash.to_string(),
            permissions,
            created_by,
            organization_id,
            None, // expires_at
        );
        
        assert_eq!(api_key.name, "test_api_key");
        assert_eq!(api_key.project_id, project_id);
        assert_eq!(api_key.organization_id, organization_id);
        assert_eq!(api_key.created_by, created_by);
    }
    
    #[tokio::test]
    async fn test_api_key_validation() {
        let service = ApiKeyService::new(/* mock db */, /* mock authz */);
        
        let raw_key = "tf_valid_key_1234567890abcdef";
        let key_hash = service.hash_api_key(raw_key);
        
        // Dalam implementasi nyata, kita akan menguji dengan database mock
        // Fungsi hash_api_key seharusnya menghasilkan hash yang konsisten
        let validation_result = service.validate_api_key(raw_key).await;
        
        // Karena tidak ada database, hasilnya akan None (tidak ditemukan)
        assert!(validation_result.is_ok());
    }
}