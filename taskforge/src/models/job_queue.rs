use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "queue_status", rename_all = "lowercase")]
pub enum QueueStatus {
    Active,
    Paused,
    Draining,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobQueue {
    pub id: Uuid,
    pub project_id: Uuid,
    pub organization_id: Uuid, // Menambahkan field untuk multi-tenant
    pub name: String,
    pub description: Option<String>,
    pub priority: i32,  // 0-9, semakin tinggi semakin prioritas
    pub settings: Value, // JSONB field untuk konfigurasi queue
    pub is_active: bool,
    pub metadata: Value, // JSONB field untuk metadata tambahan
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl JobQueue {
    pub fn new(
        project_id: Uuid,
        name: String,
        description: Option<String>,
        priority: i32,
        settings: Value,
        organization_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            organization_id,
            name,
            description,
            priority,
            settings,
            is_active: true,
            metadata: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("Queue name must be between 1 and 100 characters".to_string());
        }
        
        if self.priority < 0 || self.priority > 9 {
            return Err("Queue priority must be between 0 and 9".to_string());
        }
        
        Ok(())
    }
    
    pub fn can_accept_jobs(&self) -> bool {
        self.is_active
    }
    
    pub fn get_setting<T>(&self, key: &str) -> Option<T>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        self.settings.get(key)
            .and_then(|value| serde_json::from_value(value.clone()).ok())
    }
}