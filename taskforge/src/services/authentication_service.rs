use crate::{
    database::Database,
    models::{user::User, api_key::ApiKey},
    config::AuthenticationSettings,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use secrecy::{Secret, SecretString};
use chrono::{DateTime, Utc, Duration};
use bcrypt::{hash, verify, DEFAULT_COST};
use rand::{distributions::Alphanumeric, Rng};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,  // Subject (user ID)
    pub org: String,  // Organization ID
    pub exp: u64,     // Expiration time
    pub iat: u64,     // Issued at time
    pub permissions: Vec<String>, // Permissions for the user
    pub scope: String, // Scope of the token
}

#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub secret: Secret<String>,
    pub expiry_time_minutes: i64,
    pub refresh_secret: Secret<String>,
    pub refresh_expiry_time_minutes: i64,
}

impl JwtConfig {
    pub fn new(
        secret: String,
        expiry_time_minutes: i64,
        refresh_secret: String,
        refresh_expiry_time_minutes: i64,
    ) -> Self {
        Self {
            secret: Secret::new(secret),
            expiry_time_minutes,
            refresh_secret: Secret::new(refresh_secret),
            refresh_expiry_time_minutes,
        }
    }
}

pub struct AuthenticationService {
    jwt_config: JwtConfig,
    db: Database,
}

impl AuthenticationService {
    pub fn new(config: AuthenticationSettings, db: Database) -> Self {
        let jwt_config = JwtConfig::new(
            config.jwt_secret.expose_secret().clone(),
            config.jwt_expiry_time_minutes,
            config.refresh_secret.expose_secret().clone(),
            config.refresh_expiry_time_minutes,
        );
        
        Self { jwt_config, db }
    }
    
    pub fn generate_token(&self, user_id: Uuid, organization_id: Uuid, permissions: Vec<String>) -> Result<String, jsonwebtoken::errors::Error> {
        let now = Utc::now();
        let expiry = now + Duration::minutes(self.jwt_config.expiry_time_minutes);
        
        let claims = Claims {
            sub: user_id.to_string(),
            org: organization_id.to_string(),
            exp: expiry.timestamp() as u64,
            iat: now.timestamp() as u64,
            permissions,
            scope: "access".to_string(),
        };
        
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_config.secret.expose_secret().as_ref()),
        )
    }
    
    pub fn generate_refresh_token(&self, user_id: Uuid, organization_id: Uuid) -> Result<String, jsonwebtoken::errors::Error> {
        let now = Utc::now();
        let expiry = now + Duration::minutes(self.jwt_config.refresh_expiry_time_minutes);
        
        let claims = Claims {
            sub: user_id.to_string(),
            org: organization_id.to_string(),
            exp: expiry.timestamp() as u64,
            iat: now.timestamp() as u64,
            permissions: vec!["refresh".to_string()], // Special permission for refresh tokens
            scope: "refresh".to_string(),
        };
        
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_config.refresh_secret.expose_secret().as_ref()),
        )
    }
    
    pub fn validate_token(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let mut validation = Validation::default();
        validation.validate_exp = true;
        validation.validate_iat = true;
        validation.set_issuer(&["taskforge"]);
        
        decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.jwt_config.secret.expose_secret().as_ref()),
            &validation,
        )
        .map(|data| data.claims)
    }
    
    pub fn validate_refresh_token(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let mut validation = Validation::default();
        validation.validate_exp = true;
        validation.validate_iat = true;
        validation.set_issuer(&["taskforge"]);
        
        decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.jwt_config.refresh_secret.expose_secret().as_ref()),
            &validation,
        )
        .map(|data| data.claims)
    }
    
    pub fn is_token_expired(&self, claims: &Claims) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        
        now > claims.exp
    }
    
    pub fn extract_user_id(&self, claims: &Claims) -> Result<Uuid, uuid::Error> {
        Uuid::parse_str(&claims.sub)
    }
    
    pub fn extract_organization_id(&self, claims: &Claims) -> Result<Uuid, uuid::Error> {
        Uuid::parse_str(&claims.org)
    }
    
    pub fn has_permission(&self, claims: &Claims, required_permission: &str) -> bool {
        claims.permissions.contains(&required_permission.to_string())
    }
    
    pub fn has_any_permission(&self, claims: &Claims, required_permissions: &[&str]) -> bool {
        required_permissions.iter().any(|&perm| 
            claims.permissions.contains(&perm.to_string())
        )
    }
    
    pub async fn authenticate_user(&self, email: &str, password: &str) -> Result<Option<(User, String, String)>, AuthenticationError> {
        // Ambil user dari database
        let user = self.db.get_user_by_email(email).await
            .map_err(|_| AuthenticationError::DatabaseError)?
            .ok_or(AuthenticationError::InvalidCredentials)?;
        
        // Verifikasi password
        let valid = verify(password, &user.password_hash.expose_secret())
            .map_err(|_| AuthenticationError::InvalidCredentials)?;
        
        if !valid {
            return Err(AuthenticationError::InvalidCredentials);
        }
        
        // Periksa apakah user aktif
        if !user.is_active {
            return Err(AuthenticationError::UserInactive);
        }
        
        // Dapatkan izin dari user
        let permissions = self.get_user_permissions(&user).await?;
        
        // Generate tokens
        let access_token = self.generate_token(user.id, user.organization_id, permissions)
            .map_err(|_| AuthenticationError::TokenGenerationFailed)?;
        
        let refresh_token = self.generate_refresh_token(user.id, user.organization_id)
            .map_err(|_| AuthenticationError::TokenGenerationFailed)?;
        
        Ok(Some((user, access_token, refresh_token)))
    }
    
    pub async fn authenticate_api_key(&self, api_key: &str) -> Result<Option<(ApiKey, String)>, AuthenticationError> {
        // Dalam implementasi nyata, kita perlu meng-hash API key sebelum membandingkan
        let key_hash = self.hash_api_key(api_key);
        
        let api_key_record = self.db.get_api_key_by_hash(&key_hash).await
            .map_err(|_| AuthenticationError::DatabaseError)?
            .ok_or(AuthenticationError::InvalidCredentials)?;
        
        // Periksa apakah API key masih berlaku
        if let Some(expires_at) = api_key_record.expires_at {
            if Utc::now() > expires_at {
                return Err(AuthenticationError::TokenExpired);
            }
        }
        
        if !api_key_record.is_active {
            return Err(AuthenticationError::TokenInactive);
        }
        
        // Dapatkan izin dari API key
        let permissions = api_key_record.permissions.clone();
        
        // Generate token untuk API key
        let access_token = self.generate_token_for_api_key(
            api_key_record.id,
            api_key_record.organization_id,
            permissions,
        )?;
        
        // Update waktu penggunaan terakhir
        self.db.update_api_key_last_used(api_key_record.id).await
            .map_err(|_| AuthenticationError::DatabaseError)?;
        
        Ok(Some((api_key_record, access_token)))
    }
    
    fn hash_api_key(&self, api_key: &str) -> String {
        use sha2::{Sha256, Digest};
        
        let mut hasher = Sha256::new();
        hasher.update(api_key.as_bytes());
        format!("{:x}", hasher.finalize())
    }
    
    fn generate_token_for_api_key(&self, api_key_id: Uuid, organization_id: Uuid, permissions: Vec<String>) -> Result<String, jsonwebtoken::errors::Error> {
        let now = Utc::now();
        let expiry = now + Duration::minutes(self.jwt_config.expiry_time_minutes);
        
        let claims = Claims {
            sub: api_key_id.to_string(), // Gunakan ID API key sebagai subject
            org: organization_id.to_string(),
            exp: expiry.timestamp() as u64,
            iat: now.timestamp() as u64,
            permissions,
            scope: "api_key".to_string(),
        };
        
        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_config.secret.expose_secret().as_ref()),
        )
    }
    
    async fn get_user_permissions(&self, user: &User) -> Result<Vec<String>, AuthenticationError> {
        // Dalam implementasi nyata, ini akan mengambil izin dari database
        // berdasarkan role dan izin khusus user
        match user.role {
            crate::models::user::UserRole::Admin => Ok(vec![
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
            ]),
            crate::models::user::UserRole::Member => Ok(vec![
                "job:read".to_string(),
                "job:write".to_string(),
                "queue:read".to_string(),
                "worker:read".to_string(),
                "project:read".to_string(),
            ]),
        }
    }
    
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<String, AuthenticationError> {
        let claims = self.validate_refresh_token(refresh_token)
            .map_err(|_| AuthenticationError::InvalidToken)?;
        
        let user_id = self.extract_user_id(&claims)
            .map_err(|_| AuthenticationError::InvalidToken)?;
        
        let organization_id = self.extract_organization_id(&claims)
            .map_err(|_| AuthenticationError::InvalidToken)?;
        
        // Ambil ulang user untuk memastikan izin terbaru
        let user = self.db.get_user_by_id(user_id).await
            .map_err(|_| AuthenticationError::DatabaseError)?
            .ok_or(AuthenticationError::UserNotFound)?;
        
        if !user.is_active {
            return Err(AuthenticationError::UserInactive);
        }
        
        let permissions = self.get_user_permissions(&user).await?;
        
        let new_access_token = self.generate_token(user.id, organization_id, permissions)
            .map_err(|_| AuthenticationError::TokenGenerationFailed)?;
        
        Ok(new_access_token)
    }
    
    pub async fn hash_password(&self, password: &str) -> Result<String, AuthenticationError> {
        hash(password, DEFAULT_COST)
            .map_err(|_| AuthenticationError::PasswordHashingFailed)
    }
    
    pub fn verify_password(&self, password: &str, hash: &str) -> Result<bool, AuthenticationError> {
        verify(password, hash)
            .map_err(|_| AuthenticationError::PasswordVerificationFailed)
    }
    
    pub fn generate_api_key(&self) -> String {
        const API_KEY_LENGTH: usize = 32;
        let mut rng = rand::thread_rng();
        let api_key: String = (&mut rng)
            .sample_iter(&Alphanumeric)
            .take(API_KEY_LENGTH)
            .map(char::from)
            .collect();
        
        format!("tf_{}", api_key) // Prefix untuk membedakan dari token lain
    }
}

#[derive(Debug)]
pub enum AuthenticationError {
    InvalidCredentials,
    UserInactive,
    TokenExpired,
    TokenInactive,
    TokenGenerationFailed,
    InvalidToken,
    UserNotFound,
    DatabaseError,
    PasswordHashingFailed,
    PasswordVerificationFailed,
}

impl std::fmt::Display for AuthenticationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthenticationError::InvalidCredentials => write!(f, "Invalid credentials"),
            AuthenticationError::UserInactive => write!(f, "User account is inactive"),
            AuthenticationError::TokenExpired => write!(f, "Token has expired"),
            AuthenticationError::TokenInactive => write!(f, "Token is inactive"),
            AuthenticationError::TokenGenerationFailed => write!(f, "Failed to generate token"),
            AuthenticationError::InvalidToken => write!(f, "Invalid token"),
            AuthenticationError::UserNotFound => write!(f, "User not found"),
            AuthenticationError::DatabaseError => write!(f, "Database error"),
            AuthenticationError::PasswordHashingFailed => write!(f, "Password hashing failed"),
            AuthenticationError::PasswordVerificationFailed => write!(f, "Password verification failed"),
        }
    }
}

impl std::error::Error for AuthenticationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AuthenticationSettings;
    use secrecy::Secret;
    use uuid::Uuid;
    
    #[tokio::test]
    async fn test_jwt_generation_and_validation() {
        let auth_settings = AuthenticationSettings {
            jwt_secret: Secret::new("test_secret_key_for_testing_only".to_string()),
            jwt_expiry_time_minutes: 60,
            refresh_secret: Secret::new("test_refresh_secret_key_for_testing_only".to_string()),
            refresh_expiry_time_minutes: 10080, // 7 days
            password_min_length: 8,
            password_require_uppercase: true,
            password_require_lowercase: true,
            password_require_numbers: true,
            password_require_symbols: false,
            rate_limit_requests_per_minute: 1000,
            max_login_attempts: 5,
            login_attempt_reset_window_minutes: 15,
        };
        
        // Kita tidak bisa membuat instance Database untuk testing, jadi kita hanya akan
        // menguji fungsi-fungsi yang tidak memerlukan database
        let auth_service = AuthenticationService::new(auth_settings, /* mock database */);
        
        let user_id = Uuid::new_v4();
        let org_id = Uuid::new_v4();
        let permissions = vec!["job:read".to_string(), "job:write".to_string()];
        
        // Generate token
        let token = auth_service.generate_token(user_id, org_id, permissions.clone()).unwrap();
        
        // Validate token
        let claims = auth_service.validate_token(&token).unwrap();
        
        assert_eq!(claims.sub, user_id.to_string());
        assert_eq!(claims.org, org_id.to_string());
        assert_eq!(claims.permissions, permissions);
        assert_eq!(claims.scope, "access");
        
        // Verify that token is not expired (within 1 hour)
        assert!(!auth_service.is_token_expired(&claims));
    }
    
    #[test]
    fn test_password_hashing() {
        let auth_settings = AuthenticationSettings {
            jwt_secret: Secret::new("test_secret".to_string()),
            jwt_expiry_time_minutes: 60,
            refresh_secret: Secret::new("test_refresh_secret".to_string()),
            refresh_expiry_time_minutes: 10080,
            password_min_length: 8,
            password_require_uppercase: true,
            password_require_lowercase: true,
            password_require_numbers: true,
            password_require_symbols: false,
            rate_limit_requests_per_minute: 1000,
            max_login_attempts: 5,
            login_attempt_reset_window_minutes: 15,
        };
        
        let auth_service = AuthenticationService::new(auth_settings, /* mock database */);
        let password = "my_secure_password";
        
        // Hash password
        let hashed = auth_service.hash_password(password).unwrap();
        
        // Verify password
        assert!(auth_service.verify_password(password, &hashed).unwrap());
        
        // Verify wrong password fails
        assert!(!auth_service.verify_password("wrong_password", &hashed).unwrap());
    }
    
    #[test]
    fn test_api_key_generation() {
        let auth_settings = AuthenticationSettings {
            jwt_secret: Secret::new("test_secret".to_string()),
            jwt_expiry_time_minutes: 60,
            refresh_secret: Secret::new("test_refresh_secret".to_string()),
            refresh_expiry_time_minutes: 10080,
            password_min_length: 8,
            password_require_uppercase: true,
            password_require_lowercase: true,
            password_require_numbers: true,
            password_require_symbols: false,
            rate_limit_requests_per_minute: 1000,
            max_login_attempts: 5,
            login_attempt_reset_window_minutes: 15,
        };
        
        let auth_service = AuthenticationService::new(auth_settings, /* mock database */);
        let api_key = auth_service.generate_api_key();
        
        assert!(api_key.starts_with("tf_"));
        assert_eq!(api_key.len(), 35); // tf_ + 32 chars
        
        let hash = auth_service.hash_api_key(&api_key);
        assert_eq!(hash.len(), 64); // SHA256 hash length
    }
}