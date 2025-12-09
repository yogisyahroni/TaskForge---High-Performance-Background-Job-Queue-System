use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use secrecy::Secret;

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
    pub password_hash: Secret<String>,
    pub role: UserRole,
    pub is_active: bool,
    pub last_login_at: Option<DateTime<Utc>>,
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
            password_hash: Secret::new(password_hash),
            role,
            is_active: true,
            last_login_at: None,
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if !self.email.contains('@') || self.email.len() > 254 {
            return Err("Invalid email format".to_string());
        }
        
        if self.password_hash.expose_secret().len() < 8 {
            return Err("Password must be at least 8 characters long".to_string());
        }
        
        Ok(())
    }
    
    pub fn can_access_organization(&self, organization_id: Uuid) -> bool {
        self.organization_id == organization_id
    }
    
    pub fn has_role(&self, role: &UserRole) -> bool {
        match role {
            UserRole::Admin => matches!(self.role, UserRole::Admin),
            UserRole::Member => matches!(self.role, UserRole::Admin | UserRole::Member),
        }
    }
    
    pub fn is_admin(&self) -> bool {
        matches!(self.role, UserRole::Admin)
    }
    
    pub fn update_last_login(&mut self) {
        self.last_login_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}