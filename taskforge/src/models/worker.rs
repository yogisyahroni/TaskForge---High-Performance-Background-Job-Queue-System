use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "worker_type", rename_all = "lowercase")]
pub enum WorkerType {
    General,
    SpecializedCpu,
    SpecializedIo,
    SpecializedGpu,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "worker_status", rename_all = "lowercase")]
pub enum WorkerStatus {
    Online,
    Offline,
    Draining,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Worker {
    pub id: Uuid,
    pub project_id: Uuid,
    pub organization_id: Uuid,  // Menambahkan field untuk multi-tenant
    pub name: String,
    pub worker_type: WorkerType,
    pub status: WorkerStatus,
    pub max_concurrent_jobs: u32,
    pub capabilities: Value,     // JSONB field untuk kemampuan spesifik worker
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub drain_mode: bool,        // Apakah worker dalam mode drain
    pub metadata: Value,         // JSONB field untuk metadata tambahan
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Worker {
    pub fn new(
        project_id: Uuid,
        name: String,
        worker_type: WorkerType,
        max_concurrent_jobs: u32,
        capabilities: Value,
        organization_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id,
            organization_id,
            name,
            worker_type,
            status: WorkerStatus::Offline,
            max_concurrent_jobs,
            capabilities,
            last_heartbeat: None,
            drain_mode: false,
            metadata: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("Worker name must be between 1 and 100 characters".to_string());
        }
        
        if self.max_concurrent_jobs == 0 || self.max_concurrent_jobs > 1000 {
            return Err("Max concurrent jobs must be between 1 and 1000".to_string());
        }
        
        Ok(())
    }
    
    pub fn is_online(&self) -> bool {
        matches!(self.status, WorkerStatus::Online) && !self.drain_mode
    }
    
    pub fn is_healthy(&self, heartbeat_timeout_seconds: i64) -> bool {
        if let Some(last_heartbeat) = self.last_heartbeat {
            let now = Utc::now();
            let duration_since_heartbeat = now - last_heartbeat;
            duration_since_heartbeat.num_seconds() < heartbeat_timeout_seconds
        } else {
            false
        }
    }
    
    pub fn can_accept_more_jobs(&self, current_active_jobs: u32) -> bool {
        self.is_online() && 
        current_active_jobs < self.max_concurrent_jobs
    }
    
    pub fn enter_drain_mode(&mut self) {
        self.drain_mode = true;
        self.updated_at = Utc::now();
    }
    
    pub fn exit_drain_mode(&mut self) {
        self.drain_mode = false;
        self.updated_at = Utc::now();
    }
    
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = Some(Utc::now());
        if matches!(self.status, WorkerStatus::Offline | WorkerStatus::Unhealthy) {
            self.status = WorkerStatus::Online;
        }
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_offline(&mut self) {
        self.status = WorkerStatus::Offline;
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_unhealthy(&mut self) {
        self.status = WorkerStatus::Unhealthy;
        self.updated_at = Utc::now();
    }
    
    pub fn supports_capability(&self, capability: &str) -> bool {
        if let Some(capabilities_arr) = self.capabilities.as_array() {
            capabilities_arr.iter().any(|cap| {
                cap.as_str().map(|s| s == capability).unwrap_or(false)
            })
        } else if let Some(capabilities_obj) = self.capabilities.as_object() {
            capabilities_obj.contains_key(capability)
        } else {
            false
        }
    }
    
    pub fn get_supported_job_types(&self) -> Vec<String> {
        if let Some(capabilities_arr) = self.capabilities.as_array() {
            capabilities_arr.iter()
                .filter_map(|cap| cap.as_str().map(|s| s.to_string()))
                .collect()
        } else if let Some(capabilities_obj) = self.capabilities.as_object() {
            capabilities_obj.keys().cloned().collect()
        } else {
            vec![]
        }
    }
    
    pub fn can_process_job_type(&self, job_type: &str) -> bool {
        self.supports_capability(job_type) || 
        self.worker_type == WorkerType::General
    }
    
    pub fn get_load_percentage(&self, current_jobs: u32) -> f64 {
        if self.max_concurrent_jobs == 0 {
            0.0
        } else {
            (current_jobs as f64 / self.max_concurrent_jobs as f64) * 100.0
        }
    }
    
    pub fn is_overloaded(&self, current_jobs: u32) -> bool {
        let load_percentage = self.get_load_percentage(current_jobs);
        load_percentage > 90.0 // Dianggap overload jika beban > 90%
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct QueueWorkerAssignment {
    pub queue_id: Uuid,
    pub worker_id: Uuid,
    pub weight: i32,      // Bobot untuk load balancing
    pub is_active: bool,  // Apakah assignment aktif
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl QueueWorkerAssignment {
    pub fn new(queue_id: Uuid, worker_id: Uuid, weight: i32) -> Self {
        let now = Utc::now();
        Self {
            queue_id,
            worker_id,
            weight,
            is_active: true,
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.weight < 1 || self.weight > 100 {
            return Err("Assignment weight must be between 1 and 100".to_string());
        }
        
        Ok(())
    }
    
    pub fn is_active_assignment(&self) -> bool {
        self.is_active
    }
    
    pub fn get_effective_weight(&self, worker: &Worker) -> i32 {
        // Dalam implementasi nyata, kita bisa menggabungkan bobot assignment
        // dengan faktor lain seperti beban saat ini atau kesehatan worker
        self.weight
    }
}