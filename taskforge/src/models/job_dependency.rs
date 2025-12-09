use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "dependency_type", rename_all = "lowercase")]
pub enum DependencyType {
    Sequential,  // Child job hanya bisa mulai setelah parent job selesai
    Parallel,    // Child job bisa mulai bersamaan dengan parent job
    Conditional, // Child job hanya mulai jika parent job hasilnya sesuai kondisi
    FanIn,       // Banyak job menunggu satu job
    FanOut,      // Satu job menunggu banyak job
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "dependency_status", rename_all = "lowercase")]
pub enum DependencyStatus {
    Pending,    // Ketergantungan belum terpenuhi
    Resolved,   // Ketergantungan terpenuhi
    Failed,     // Ketergantungan gagal dipenuhi
    Cancelled,  // Ketergantungan dibatalkan
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobDependency {
    pub id: Uuid,
    pub parent_job_id: Uuid,    // Job yang harus selesai dulu
    pub child_job_id: Uuid,     // Job yang menunggu parent
    pub dependency_type: DependencyType,
    pub condition: Option<String>, // Kondisi tambahan (misal: "succeeded", "failed", "always")
    pub status: DependencyStatus,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<Uuid>, // ID job execution yang menyelesaikan ketergantungan
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl JobDependency {
    pub fn new(
        parent_job_id: Uuid,
        child_job_id: Uuid,
        dependency_type: DependencyType,
        condition: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            parent_job_id,
            child_job_id,
            dependency_type,
            condition,
            status: DependencyStatus::Pending,
            resolved_at: None,
            resolved_by: None,
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.parent_job_id == self.child_job_id {
            return Err("Parent and child job cannot be the same".to_string());
        }
        
        Ok(())
    }
    
    pub fn is_resolved(&self) -> bool {
        matches!(self.status, DependencyStatus::Resolved)
    }
    
    pub fn can_execute_child(&self, parent_status: &str) -> bool {
        match self.dependency_type {
            DependencyType::Sequential => {
                match self.condition.as_deref().unwrap_or("always") {
                    "succeeded" => parent_status == "succeeded",
                    "failed" => parent_status == "failed",
                    "always" => matches!(parent_status, "succeeded" | "failed"),
                    _ => matches!(parent_status, "succeeded" | "failed"),
                }
            },
            DependencyType::Parallel => {
                matches!(parent_status, "started" | "processing")
            },
            DependencyType::Conditional => {
                match self.condition.as_deref().unwrap_or("succeeded") {
                    "succeeded" => parent_status == "succeeded",
                    "failed" => parent_status == "failed",
                    _ => parent_status == "succeeded", // Default behavior
                }
            },
            DependencyType::FanIn | DependencyType::FanOut => {
                // For fan-in/fan-out, need additional logic to check all dependencies
                matches!(parent_status, "succeeded" | "failed")
            },
        }
    }
    
    pub fn mark_as_resolved(&mut self, resolved_by: Uuid) {
        self.status = DependencyStatus::Resolved;
        self.resolved_at = Some(Utc::now());
        self.resolved_by = Some(resolved_by);
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_failed(&mut self) {
        self.status = DependencyStatus::Failed;
        self.resolved_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_cancelled(&mut self) {
        self.status = DependencyStatus::Cancelled;
        self.resolved_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn is_met(&self, parent_job_status: &str) -> bool {
        match self.dependency_type {
            DependencyType::Sequential => {
                // Untuk sequential, dependency terpenuhi jika parent job selesai
                matches!(parent_job_status, "succeeded" | "failed")
            },
            DependencyType::Parallel => {
                // Untuk parallel, dependency terpenuhi saat parent job mulai
                matches!(parent_job_status, "processing" | "succeeded" | "failed")
            },
            DependencyType::Conditional => {
                // Untuk conditional, cek kondisi tertentu
                if let Some(condition) = &self.condition {
                    match condition.as_str() {
                        "succeeded" => parent_job_status == "succeeded",
                        "failed" => parent_job_status == "failed",
                        _ => false, // Kondisi tidak dikenal
                    }
                } else {
                    false // Tidak ada kondisi berarti tidak terpenuhi
                }
            },
            DependencyType::FanIn => {
                // Dalam fan-in, semua parent job harus selesai
                matches!(parent_job_status, "succeeded" | "failed")
            },
            DependencyType::FanOut => {
                // Dalam fan-out, cukup satu parent job selesai
                matches!(parent_job_status, "processing" | "succeeded" | "failed")
            },
        }
    }
    
    pub fn get_required_status(&self) -> Vec<&str> {
        match self.dependency_type {
            DependencyType::Sequential => vec!["succeeded", "failed"],
            DependencyType::Parallel => vec!["processing", "succeeded", "failed"],
            DependencyType::Conditional => {
                if let Some(condition) = &self.condition {
                    match condition.as_str() {
                        "succeeded" => vec!["succeeded"],
                        "failed" => vec!["failed"],
                        _ => vec![], // Kondisi tidak dikenal
                    }
                } else {
                    vec![] // Tidak ada kondisi berarti tidak ada status yang diperlukan
                }
            },
            DependencyType::FanIn => vec!["succeeded", "failed"],
            DependencyType::FanOut => vec!["processing", "succeeded", "failed"],
        }
    }
}