# Manajemen API Key untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan sistem manajemen API key dalam aplikasi TaskForge. Sistem ini memungkinkan pengguna untuk mengakses layanan TaskForge secara programatik dengan otentikasi yang aman dan otorisasi berbasis perizinan.

## 2. Arsitektur Sistem API Key

### 2.1. Model API Key

API key dalam TaskForge dirancang untuk memberikan akses terotentikasi dan terotorisasi ke layanan, dengan fitur keamanan dan manajemen yang komprehensif:

```rust
// File: src/models/api_key.rs
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key_hash: String,      // Hash dari API key (menggunakan bcrypt/scrypt)
    pub name: String,          // Nama deskriptif untuk API key
    pub permissions: Vec<String>, // Daftar izin akses (misal: ["job:read", "job:write", "queue:read"])
    pub last_used_at: Option<DateTime<Utc>>, // Waktu terakhir digunakan
    pub expires_at: Option<DateTime<Utc>>,   // Waktu kadaluarsa (opsional)
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ApiKey {
    pub fn new(
        project_id: Uuid,
        name: String,
        permissions: Vec<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> (Self, String) {
        // Generate API key acak
        let raw_key = generate_secure_api_key();
        let key_hash = hash_api_key(&raw_key);
        
        let now = Utc::now();
        let api_key = Self {
            id: Uuid::new_v4(),
            project_id,
            key_hash,
            name,
            permissions,
            last_used_at: None,
            expires_at,
            created_at: now,
            updated_at: now,
        };
        
        (api_key, raw_key)
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("API key name must be between 1 and 100 characters".to_string());
        }
        
        if self.permissions.is_empty() {
            return Err("API key must have at least one permission".to_string());
        }
        
        // Validasi format permission
        for perm in &self.permissions {
            if !is_valid_permission_format(perm) {
                return Err(format!("Invalid permission format: {}", perm));
            }
        }
        
        Ok(())
    }
    
    pub fn is_valid(&self) -> bool {
        // Cek apakah API key sudah kadaluarsa
        if let Some(expires_at) = self.expires_at {
            if Utc::now() > expires_at {
                return false;
            }
        }
        
        // Bisa ditambahkan validasi lainnya di sini
        true
    }
    
    pub fn has_permission(&self, required_permission: &str) -> bool {
        self.permissions.contains(&required_permission.to_string())
    }
    
    pub fn has_any_permission(&self, required_permissions: &[&str]) -> bool {
        required_permissions.iter().any(|&perm| self.has_permission(perm))
    }
}

// Fungsi bantu untuk generate dan hash API key
fn generate_secure_api_key() -> String {
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

fn hash_api_key(api_key: &str) -> String {
    use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
    use rand::rngs::OsRng;
    
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2.hash_password(api_key.as_bytes(), &salt).unwrap();
    
    password_hash.to_string()
}

fn is_valid_permission_format(permission: &str) -> bool {
    // Format permission: resource:action (misal: job:read, queue:write)
    let parts: Vec<&str> = permission.split(':').collect();
    if parts.len() != 2 {
        return false;
    }
    
    let resource = parts[0];
    let action = parts[1];
    
    // Validasi karakter yang diizinkan
    resource.chars().all(|c| c.is_alphanumeric() || c == '_') && 
    action.chars().all(|c| c.is_alphanumeric() || c == '_')
}
```

### 2.2. Permission System

Sistem permission dalam TaskForge mengikuti pola `resource:action`:

- `job:read` - Membaca informasi job
- `job:write` - Membuat, memperbarui, atau menghapus job
- `queue:read` - Membaca informasi antrian
- `queue:write` - Membuat, memperbarui, atau menghapus antrian
- `worker:read` - Membaca status worker
- `worker:write` - Mengelola worker
- `project:read` - Membaca informasi proyek
- `project:write` - Mengelola proyek

## 3. Sistem Otentikasi API Key

### 3.1. Middleware Otentikasi API Key

```rust
// File: src/middleware/api_key_auth.rs
use axum::{
    extract::{Request, State},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use crate::{
    services::api_key_service::ApiKeyService,
    auth::middleware::AuthenticatedUser,
    models::ApiKey,
};

pub async fn api_key_auth_middleware(
    State(api_key_service): State<Arc<ApiKeyService>>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Ambil header Authorization
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|header| header.to_str().ok());
    
    let api_key_str = if let Some(auth_header) = auth_header {
        // Coba parsing format "Bearer <api_key>" atau "API-Key <api_key>"
        if let Some(key) = auth_header.strip_prefix("Bearer ").or_else(|| 
            auth_header.strip_prefix("API-Key ")
        ) {
            key.trim().to_string()
        } else {
            return Err(StatusCode::UNAUTHORIZED);
        }
    } else {
        // Coba dari header kustom
        request
            .headers()
            .get("X-API-Key")
            .and_then(|header| header.to_str().ok())
            .map(|s| s.to_string())
            .ok_or(StatusCode::UNAUTHORIZED)?
    };
    
    // Validasi API key
    let api_key = api_key_service
        .validate_api_key(&api_key_str)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    if let Some(api_key) = api_key {
        if !api_key.is_valid() {
            return Err(StatusCode::UNAUTHORIZED);
        }
        
        // Perbarui waktu penggunaan terakhir
        api_key_service
            .update_last_used(api_key.id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        
        // Dapatkan informasi proyek untuk menentukan organisasi
        let project = api_key_service
            .get_project_by_id(api_key.project_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;
        
        // Buat user terotentikasi berdasarkan API key
        let authenticated_user = AuthenticatedUser {
            user_id: api_key.id, // Gunakan ID API key sebagai user ID
            organization_id: project.organization_id,
            permissions: api_key.permissions,
        };
        
        // Masukkan ke request extensions
        request.extensions_mut().insert(authenticated_user);
        
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
```

### 3.2. Layanan API Key

```rust
// File: src/services/api_key_service.rs
use crate::{
    database::Database,
    models::ApiKey,
    utils::crypto,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};

pub struct ApiKeyService {
    db: Database,
}

impl ApiKeyService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn create_api_key(
        &self,
        project_id: Uuid,
        name: String,
        permissions: Vec<String>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<(ApiKey, String), SqlxError> {
        let (api_key, raw_key) = ApiKey::new(project_id, name, permissions, expires_at);
        api_key.validate().map_err(|_| SqlxError::RowNotFound)?;
        
        let created_key = self.db.create_api_key(api_key).await?;
        Ok((created_key, raw_key))
    }
    
    pub async fn validate_api_key(
        &self,
        raw_api_key: &str,
    ) -> Result<Option<ApiKey>, SqlxError> {
        let key_hash = crypto::hash_api_key(raw_api_key);
        self.db.get_api_key_by_hash(&key_hash).await
    }
    
    pub async fn get_api_key_by_id(
        &self,
        api_key_id: Uuid,
    ) -> Result<Option<ApiKey>, SqlxError> {
        self.db.get_api_key_by_id(api_key_id).await
    }
    
    pub async fn get_api_keys_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ApiKey>, SqlxError> {
        self.db.get_api_keys_by_project(project_id).await
    }
    
    pub async fn update_api_key(
        &self,
        api_key_id: Uuid,
        name: Option<String>,
        permissions: Option<Vec<String>>,
        expires_at: Option<Option<DateTime<Utc>>>,
    ) -> Result<(), SqlxError> {
        self.db.update_api_key(api_key_id, name, permissions, expires_at).await
    }
    
    pub async fn update_last_used(
        &self,
        api_key_id: Uuid,
    ) -> Result<(), SqlxError> {
        self.db.update_api_key_last_used(api_key_id).await
    }
    
    pub async fn revoke_api_key(
        &self,
        api_key_id: Uuid,
    ) -> Result<(), SqlxError> {
        self.db.delete_api_key(api_key_id).await
    }
    
    pub async fn rotate_api_key(
        &self,
        api_key_id: Uuid,
    ) -> Result<String, SqlxError> {
        let old_key = self.get_api_key_by_id(api_key_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Buat API key baru dengan detail yang sama
        let (new_key, new_raw_key) = ApiKey::new(
            old_key.project_id,
            format!("{} (rotated)", old_key.name),
            old_key.permissions,
            old_key.expires_at,
        );
        
        // Simpan API key baru
        self.db.create_api_key(new_key).await?;
        
        // Hapus API key lama
        self.db.delete_api_key(api_key_id).await?;
        
        Ok(new_raw_key)
    }
    
    pub async fn get_project_by_id(
        &self,
        project_id: Uuid,
    ) -> Result<Option<crate::models::Project>, SqlxError> {
        self.db.get_project_by_id(project_id).await
    }
}
```

## 4. Endpoint API untuk Manajemen API Key

### 4.1. Handler API Key

```rust
// File: src/handlers/api_key_handler.rs
use axum::{
    extract::{State, Path, Json},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use crate::{
    services::api_key_service::ApiKeyService,
    auth::middleware::{AuthenticatedUser, check_permission},
    models::ApiKey,
};

#[derive(Deserialize)]
pub struct CreateApiKeyRequest {
    pub name: String,
    pub permissions: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize)]
pub struct CreateApiKeyResponse {
    pub id: uuid::Uuid,
    pub name: String,
    pub permissions: Vec<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub api_key: String, // API key yang baru dibuat - hanya ditampilkan sekali
}

#[derive(Deserialize)]
pub struct UpdateApiKeyRequest {
    pub name: Option<String>,
    pub permissions: Option<Vec<String>>,
    pub expires_at: Option<Option<chrono::DateTime<chrono::Utc>>>,
}

pub async fn create_api_key(
    State(api_key_service): State<ApiKeyService>,
    authenticated_user: AuthenticatedUser,
    Json(payload): Json<CreateApiKeyRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // Periksa izin untuk membuat API key
    if !check_permission(&authenticated_user, "api_key:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Dapatkan proyek terkait dari parameter atau default
    // Dalam implementasi nyata, ini mungkin dari path parameter atau request body
    let project_id = /* dapatkan dari request */ uuid::Uuid::new_v4();
    
    let (api_key, raw_key) = api_key_service
        .create_api_key(
            project_id,
            payload.name,
            payload.permissions,
            payload.expires_at,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(CreateApiKeyResponse {
        id: api_key.id,
        name: api_key.name,
        permissions: api_key.permissions,
        expires_at: api_key.expires_at,
        created_at: api_key.created_at,
        api_key: raw_key, // Ini hanya ditampilkan sekali saat pembuatan
    }))
}

pub async fn get_api_key(
    State(api_key_service): State<ApiKeyService>,
    authenticated_user: AuthenticatedUser,
    Path(api_key_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "api_key:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let api_key = api_key_service
        .get_api_key_by_id(api_key_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan API key milik proyek dalam organisasi yang sama
    let project = api_key_service
        .get_project_by_id(api_key.project_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    Ok(Json(api_key))
}

pub async fn get_api_keys(
    State(api_key_service): State<ApiKeyService>,
    authenticated_user: AuthenticatedUser,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "api_key:read") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Dapatkan semua proyek dalam organisasi
    let projects = /* implementasikan untuk mendapatkan proyek dalam organisasi */;
    
    let mut all_api_keys = Vec::new();
    for project in projects {
        let project_api_keys = api_key_service
            .get_api_keys_by_project(project.id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        
        all_api_keys.extend(project_api_keys);
    }
    
    Ok(Json(all_api_keys))
}

pub async fn update_api_key(
    State(api_key_service): State<ApiKeyService>,
    authenticated_user: AuthenticatedUser,
    Path(api_key_id): Path<uuid::Uuid>,
    Json(payload): Json<UpdateApiKeyRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "api_key:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let api_key = api_key_service
        .get_api_key_by_id(api_key_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan API key milik proyek dalam organisasi yang sama
    let project = api_key_service
        .get_project_by_id(api_key.project_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    api_key_service
        .update_api_key(
            api_key_id,
            payload.name,
            payload.permissions,
            payload.expires_at,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_api_key(
    State(api_key_service): State<ApiKeyService>,
    authenticated_user: AuthenticatedUser,
    Path(api_key_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "api_key:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let api_key = api_key_service
        .get_api_key_by_id(api_key_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan API key milik proyek dalam organisasi yang sama
    let project = api_key_service
        .get_project_by_id(api_key.project_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    api_key_service
        .revoke_api_key(api_key_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}

pub async fn rotate_api_key(
    State(api_key_service): State<ApiKeyService>,
    authenticated_user: AuthenticatedUser,
    Path(api_key_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    if !check_permission(&authenticated_user, "api_key:write") {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let api_key = api_key_service
        .get_api_key_by_id(api_key_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan API key milik proyek dalam organisasi yang sama
    let project = api_key_service
        .get_project_by_id(api_key.project_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    if project.organization_id != authenticated_user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let new_raw_key = api_key_service
        .rotate_api_key(api_key_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(Json(serde_json::json!({
        "api_key": new_raw_key,
        "message": "API key rotated successfully"
    })))
}
```

## 5. Keamanan API Key

### 5.1. Praktik Keamanan Terbaik

```rust
// File: src/utils/crypto.rs
use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
use rand::rngs::OsRng;

pub fn hash_api_key(api_key: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2.hash_password(api_key.as_bytes(), &salt).unwrap();
    
    password_hash.to_string()
}

pub fn verify_api_key(api_key: &str, expected_hash: &str) -> bool {
    use argon2::PasswordVerifier;
    use argon2::password_hash::PasswordHash;
    
    let parsed_hash = PasswordHash::new(expected_hash).unwrap();
    let argon2 = Argon2::default();
    
    argon2.verify_password(api_key.as_bytes(), &parsed_hash).is_ok()
}

pub fn generate_secure_api_key() -> String {
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
```

### 5.2. Rate Limiting Berbasis API Key

```rust
// File: src/middleware/rate_limit.rs
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct ApiKeyRateLimiter {
    // Rate limits per API key
    limits: Arc<RwLock<HashMap<Uuid, Vec<RequestRecord>>>>,
    // Default rate limit: 100 requests per hour
    default_requests_per_hour: u32,
}

struct RequestRecord {
    timestamp: Instant,
    endpoint: String,
}

impl ApiKeyRateLimiter {
    pub fn new(default_requests_per_hour: u32) -> Self {
        Self {
            limits: Arc::new(RwLock::new(HashMap::new())),
            default_requests_per_hour,
        }
    }
    
    pub async fn is_allowed(
        &self,
        api_key_id: Uuid,
        endpoint: &str,
        max_requests: Option<u32>,
    ) -> bool {
        let now = Instant::now();
        let max_requests = max_requests.unwrap_or(self.default_requests_per_hour);
        
        let mut limits = self.limits.write().await;
        let records = limits.entry(api_key_id).or_insert_with(Vec::new);
        
        // Hapus request lama (lebih dari 1 jam)
        records.retain(|record| {
            now.duration_since(record.timestamp) < Duration::from_secs(3600)
        });
        
        // Periksa apakah jumlah request melebihi batas
        if records.len() >= max_requests as usize {
            return false;
        }
        
        // Tambahkan request baru
        records.push(RequestRecord {
            timestamp: now,
            endpoint: endpoint.to_string(),
        });
        
        true
    }
    
    pub async fn get_remaining_requests(
        &self,
        api_key_id: Uuid,
        max_requests: Option<u32>,
    ) -> u32 {
        let max_requests = max_requests.unwrap_or(self.default_requests_per_hour);
        let limits = self.limits.read().await;
        
        if let Some(records) = limits.get(&api_key_id) {
            max_requests.saturating_sub(records.len() as u32)
        } else {
            max_requests
        }
    }
}
```

### 5.3. Middleware Rate Limiting

```rust
// File: src/middleware/api_key_rate_limit.rs
use axum::{
    extract::{Request, State},
    http::{StatusCode, HeaderMap},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use crate::middleware::rate_limit::ApiKeyRateLimiter;

pub async fn api_key_rate_limit_middleware(
    State(rate_limiter): State<Arc<ApiKeyRateLimiter>>,
    authenticated_user: crate::auth::middleware::AuthenticatedUser,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let api_key_id = authenticated_user.user_id; // Dalam konteks API key, user_id adalah ID API key
    let path = request.uri().path();
    
    // Cek apakah permintaan diizinkan berdasarkan rate limit
    if !rate_limiter
        .is_allowed(api_key_id, path, None)
        .await {
        // Dapatkan header rate limit untuk respons
        let remaining = rate_limiter
            .get_remaining_requests(api_key_id, None)
            .await;
        
        let mut response = Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("X-RateLimit-Remaining", remaining.to_string())
            .header("Retry-After", "3600") // 1 jam
            .body(axum::body::Body::from("Rate limit exceeded"))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        
        return Ok(response);
    }
    
    Ok(next.run(request).await)
}
```

## 6. Audit dan Monitoring

### 6.1. Audit Log API Key

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
    
    pub async fn log_api_key_action(
        &self,
        api_key_id: Uuid,
        project_id: Uuid,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        details: Option<Value>,
    ) -> Result<(), sqlx::Error> {
        self.db.insert_api_key_audit_log(
            api_key_id,
            project_id,
            action,
            resource_type,
            resource_id,
            details,
        ).await
    }
}
```

### 6.2. Monitoring Penggunaan API Key

```rust
// File: src/services/monitoring_service.rs
use crate::database::Database;
use uuid::Uuid;
use chrono::{DateTime, Utc};

pub struct MonitoringService {
    db: Database,
}

impl MonitoringService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn get_api_key_usage(
        &self,
        api_key_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<ApiKeyUsage, sqlx::Error> {
        let request_count = self.db.get_api_key_request_count(
            api_key_id,
            start_time,
            end_time,
        ).await?;
        
        let avg_response_time = self.db.get_api_key_avg_response_time(
            api_key_id,
            start_time,
            end_time,
        ).await?;
        
        Ok(ApiKeyUsage {
            api_key_id,
            request_count,
            avg_response_time,
            start_time,
            end_time,
        })
    }
}

pub struct ApiKeyUsage {
    pub api_key_id: Uuid,
    pub request_count: u64,
    pub avg_response_time: f64, // dalam milidetik
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
}
```

## 7. Rotasi dan Manajemen Masa Hidup API Key

### 7.1. Kebijakan Rotasi

TaskForge menerapkan kebijakan rotasi API key untuk meningkatkan keamanan:

1. **API key otomatis kedaluwarsa** setelah periode tertentu jika tidak digunakan
2. **Notifikasi sebelum kedaluwarsa** dikirim ke pengguna
3. **Proses rotasi otomatis** untuk API key yang digunakan secara aktif

### 7.2. Proses Rotasi Otomatis

```rust
// File: src/services/rotation_service.rs
use crate::services::api_key_service::ApiKeyService;
use uuid::Uuid;
use chrono::{DateTime, Utc, Duration};

pub struct RotationService {
    api_key_service: ApiKeyService,
}

impl RotationService {
    pub fn new(api_key_service: ApiKeyService) -> Self {
        Self { api_key_service }
    }
    
    pub async fn check_and_rotate_api_keys(&self) -> Result<(), sqlx::Error> {
        // Ambil semua API key yang hampir kedaluwarsa atau tidak digunakan dalam waktu lama
        let api_keys_to_rotate = self.api_key_service
            .get_api_keys_requiring_rotation()
            .await?;
        
        for api_key in api_keys_to_rotate {
            // Kirim notifikasi ke pengguna
            self.notify_api_key_rotation(&api_key).await?;
            
            // Rotasi API key
            let new_key = self.api_key_service
                .rotate_api_key(api_key.id)
                .await?;
            
            // Log aktivitas rotasi
            self.log_rotation_activity(&api_key, &new_key).await?;
        }
        
        Ok(())
    }
    
    async fn notify_api_key_rotation(&self, api_key: &crate::models::ApiKey) -> Result<(), sqlx::Error> {
        // Implementasi notifikasi (email, webhook, dll)
        Ok(())
    }
    
    async fn log_rotation_activity(&self, old_key: &crate::models::ApiKey, new_key: &str) -> Result<(), sqlx::Error> {
        // Log aktivitas rotasi ke sistem audit
        Ok(())
    }
}
```

## 8. Best Practices dan Rekomendasi

### 8.1. Praktik Keamanan Terbaik

1. **Jangan pernah menyimpan API key dalam bentuk plain text** - selalu hash sebelum menyimpan
2. **Gunakan algoritma hashing yang kuat** seperti Argon2 atau bcrypt
3. **Batasi masa berlaku API key** - terapkan expiration time
4. **Terapkan rate limiting** berbasis API key
5. **Rotasi API key secara berkala** - terutama untuk API key yang digunakan dalam produksi
6. **Audit semua penggunaan API key** - untuk keperluan keamanan dan troubleshooting

### 8.2. Praktik Manajemen Terbaik

1. **Gunakan nama deskriptif** untuk API key agar mudah dikenali
2. **Terapkan prinsip least privilege** - berikan hanya izin yang diperlukan
3. **Gunakan lingkungan terpisah** untuk API key development dan production
4. **Gunakan secret management system** untuk menyimpan API key dengan aman
5. **Monitoring penggunaan API key** - untuk mendeteksi penggunaan yang tidak normal

### 8.3. Skala dan Kinerja

1. **Cache validasi API key** - untuk mengurangi permintaan database
2. **Gunakan indeks yang tepat** pada kolom yang sering digunakan untuk pencarian
3. **Optimalkan query database** - terutama untuk operasi validasi API key
4. **Gunakan connection pooling** - untuk efisiensi koneksi database