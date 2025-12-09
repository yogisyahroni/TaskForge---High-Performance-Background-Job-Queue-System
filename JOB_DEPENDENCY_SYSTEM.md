# Sistem Ketergantungan Antar Job untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan sistem ketergantungan antar job dalam aplikasi TaskForge. Sistem ini memungkinkan pengguna untuk membuat hubungan ketergantungan antar job, di mana suatu job hanya akan dieksekusi setelah job lain selesai, menciptakan alur kerja (workflow) yang kompleks dan terkoordinasi.

## 2. Arsitektur Sistem Ketergantungan

### 2.1. Konsep Dasar Ketergantungan

Dalam sistem TaskForge, ketergantungan antar job dapat berupa:

1. **Sequential Dependencies**: Job B menunggu Job A selesai sebelum mulai
2. **Parallel Dependencies**: Job B dapat mulai saat Job A mulai
3. **Conditional Dependencies**: Job B hanya mulai jika Job A selesai dengan status tertentu
4. **Fan-in/Fan-out Patterns**: Banyak job menunggu satu job (fan-in) atau satu job menunggu banyak job (fan-out)

### 2.2. Model Ketergantungan Job

```rust
// File: src/models/job_dependency.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "dependency_type", rename_all = "lowercase")]
pub enum DependencyType {
    Sequential,   // Child job menunggu parent job selesai
    Parallel,     // Child job bisa mulai saat parent job mulai
    Conditional,  // Child job hanya mulai jika parent job hasilnya tertentu
    FanIn,        // Banyak job menunggu satu job
    FanOut,       // Satu job menunggu banyak job
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
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<Uuid>, // ID job execution yang menyelesaikan ketergantungan
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
            created_at: now,
            resolved_at: None,
            resolved_by: None,
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
                // Untuk fan-in/fan-out, perlu logika tambahan untuk menghitung semua dependensi
                matches!(parent_status, "succeeded" | "failed")
            },
        }
    }
    
    pub fn mark_as_resolved(&mut self, resolved_by: Uuid) {
        self.status = DependencyStatus::Resolved;
        self.resolved_at = Some(Utc::now());
        self.resolved_by = Some(resolved_by);
    }
    
    pub fn mark_as_failed(&mut self) {
        self.status = DependencyStatus::Failed;
        self.resolved_at = Some(Utc::now());
    }
    
    pub fn mark_as_cancelled(&mut self) {
        self.status = DependencyStatus::Cancelled;
        self.resolved_at = Some(Utc::now());
    }
}
```

### 2.3. Model Workflow

```rust
// File: src/models/workflow.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "workflow_status", rename_all = "lowercase")]
pub enum WorkflowStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Workflow {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub definition: Value,      // Definisi workflow dalam format JSON
    pub status: WorkflowStatus,
    pub created_by: Uuid,       // ID pengguna yang membuat
    pub organization_id: Uuid,  // ID organisasi (untuk multi-tenant)
    pub metadata: Value,        // Metadata tambahan
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Workflow {
    pub fn new(
        name: String,
        definition: Value,
        created_by: Uuid,
        organization_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            definition,
            status: WorkflowStatus::Pending,
            created_by,
            organization_id,
            metadata: serde_json::json!({}),
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("Workflow name must be between 1 and 100 characters".to_string());
        }
        
        // Validasi struktur definisi workflow
        validate_workflow_definition(&self.definition)
    }
    
    pub fn start(&mut self) {
        self.status = WorkflowStatus::Running;
        self.started_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn complete(&mut self, success: bool) {
        self.status = if success {
            WorkflowStatus::Succeeded
        } else {
            WorkflowStatus::Failed
        };
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn cancel(&mut self) {
        self.status = WorkflowStatus::Cancelled;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}

fn validate_workflow_definition(definition: &Value) -> Result<(), String> {
    // Validasi struktur dasar definisi workflow
    if !definition.is_object() {
        return Err("Workflow definition must be an object".to_string());
    }
    
    // Pastikan ada field 'jobs' dan 'dependencies'
    if definition.get("jobs").is_none() {
        return Err("Workflow definition must contain 'jobs' field".to_string());
    }
    
    if definition.get("dependencies").is_none() {
        return Err("Workflow definition must contain 'dependencies' field".to_string());
    }
    
    Ok(())
}
```

## 3. Layanan Ketergantungan Job

### 3.1. Layanan Ketergantungan Utama

```rust
// File: src/services/job_dependency_service.rs
use crate::{
    models::{JobDependency, DependencyType, DependencyStatus, Job},
    services::job_service::JobService,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};

pub struct JobDependencyService {
    db: Database, // Gantilah dengan tipe database Anda
    job_service: JobService,
}

impl JobDependencyService {
    pub fn new(db: Database, job_service: JobService) -> Self {
        Self { db, job_service }
    }
    
    pub async fn create_dependency(
        &self,
        parent_job_id: Uuid,
        child_job_id: Uuid,
        dependency_type: DependencyType,
        condition: Option<String>,
    ) -> Result<JobDependency, SqlxError> {
        let dependency = JobDependency::new(parent_job_id, child_job_id, dependency_type, condition);
        dependency.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        self.db.create_job_dependency(dependency).await
    }
    
    pub async fn create_sequential_dependency(
        &self,
        parent_job_id: Uuid,
        child_job_id: Uuid,
    ) -> Result<JobDependency, SqlxError> {
        self.create_dependency(parent_job_id, child_job_id, DependencyType::Sequential, None).await
    }
    
    pub async fn create_conditional_dependency(
        &self,
        parent_job_id: Uuid,
        child_job_id: Uuid,
        condition: &str,
    ) -> Result<JobDependency, SqlxError> {
        self.create_dependency(parent_job_id, child_job_id, DependencyType::Conditional, Some(condition.to_string())).await
    }
    
    pub async fn get_dependencies_for_job(
        &self,
        job_id: Uuid,
    ) -> Result<Vec<JobDependency>, SqlxError> {
        self.db.get_job_dependencies(job_id).await
    }
    
    pub async fn get_parent_dependencies(
        &self,
        job_id: Uuid,
    ) -> Result<Vec<JobDependency>, SqlxError> {
        self.db.get_parent_dependencies(job_id).await
    }
    
    pub async fn get_child_dependencies(
        &self,
        job_id: Uuid,
    ) -> Result<Vec<JobDependency>, SqlxError> {
        self.db.get_child_dependencies(job_id).await
    }
    
    pub async fn resolve_dependency(
        &self,
        dependency_id: Uuid,
        resolved_by: Uuid,
    ) -> Result<(), SqlxError> {
        let mut dependency = self.db.get_job_dependency_by_id(dependency_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        dependency.mark_as_resolved(resolved_by);
        self.db.update_job_dependency(dependency).await?;
        
        Ok(())
    }
    
    pub async fn check_job_dependencies(
        &self,
        job_id: Uuid,
    ) -> Result<DependencyCheckResult, SqlxError> {
        let dependencies = self.get_parent_dependencies(job_id).await?;
        
        if dependencies.is_empty() {
            // Tidak ada ketergantungan, job bisa dieksekusi
            return Ok(DependencyCheckResult {
                can_execute: true,
                pending_dependencies: 0,
                failed_dependencies: 0,
            });
        }
        
        let mut pending_count = 0;
        let mut failed_count = 0;
        
        for dependency in dependencies {
            match dependency.status {
                DependencyStatus::Resolved => {
                    // Ketergantungan terpenuhi, lanjutkan ke pemeriksaan berikutnya
                    continue;
                },
                DependencyStatus::Pending => {
                    // Cek apakah parent job sudah selesai dan memenuhi kondisi
                    if let Some(parent_job) = self.job_service.get_job_by_id(dependency.parent_job_id).await? {
                        if dependency.can_execute_child(&parent_job.status.as_str()) {
                            // Ketergantungan terpenuhi, update status
                            self.resolve_dependency(dependency.id, parent_job.id).await?;
                        } else {
                            pending_count += 1;
                        }
                    } else {
                        // Parent job tidak ditemukan, mungkin gagal
                        failed_count += 1;
                    }
                },
                DependencyStatus::Failed => {
                    failed_count += 1;
                },
                DependencyStatus::Cancelled => {
                    // Jika dependensi dibatalkan, kita harus memutuskan apakah child job tetap jalan
                    failed_count += 1;
                },
            }
        }
        
        Ok(DependencyCheckResult {
            can_execute: pending_count == 0 && failed_count == 0,
            pending_dependencies: pending_count,
            failed_dependencies: failed_count,
        })
    }
    
    pub async fn get_dependency_graph(
        &self,
        job_id: Uuid,
    ) -> Result<DependencyGraph, SqlxError> {
        let mut graph = DependencyGraph::new();
        
        // Ambil semua dependensi secara rekursif
        self.build_dependency_graph(job_id, &mut graph).await?;
        
        Ok(graph)
    }
    
    async fn build_dependency_graph(
        &self,
        job_id: Uuid,
        graph: &mut DependencyGraph,
    ) -> Result<(), SqlxError> {
        let dependencies = self.get_parent_dependencies(job_id).await?;
        
        for dependency in dependencies {
            graph.add_dependency(dependency.parent_job_id, dependency.child_job_id);
            
            // Rekursi ke parent job untuk membangun grafik lengkap
            self.build_dependency_graph(dependency.parent_job_id, graph).await?;
        }
        
        Ok(())
    }
    
    pub async fn cancel_job_dependencies(
        &self,
        job_id: Uuid,
    ) -> Result<(), SqlxError> {
        // Batalkan semua ketergantungan yang menunggu job ini
        let child_dependencies = self.get_child_dependencies(job_id).await?;
        
        for mut dependency in child_dependencies {
            dependency.mark_as_cancelled();
            self.db.update_job_dependency(dependency).await?;
        }
        
        Ok(())
    }
    
    pub async fn get_workflow_jobs(
        &self,
        workflow_id: Uuid,
    ) -> Result<Vec<WorkflowJob>, SqlxError> {
        self.db.get_workflow_jobs(workflow_id).await
    }
}

pub struct DependencyCheckResult {
    pub can_execute: bool,
    pub pending_dependencies: u32,
    pub failed_dependencies: u32,
}

#[derive(Debug, Clone)]
pub struct DependencyGraph {
    pub nodes: std::collections::HashMap<Uuid, Vec<Uuid>>, // parent -> children
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            nodes: std::collections::HashMap::new(),
        }
    }
    
    pub fn add_dependency(&mut self, parent: Uuid, child: Uuid) {
        self.nodes.entry(parent).or_insert_with(Vec::new).push(child);
    }
    
    pub fn get_children(&self, parent: Uuid) -> Vec<Uuid> {
        self.nodes.get(&parent).cloned().unwrap_or_default()
    }
    
    pub fn get_roots(&self) -> Vec<Uuid> {
        // Job yang tidak memiliki parent (root nodes)
        let all_nodes: std::collections::HashSet<Uuid> = self.nodes.keys().copied().collect();
        let child_nodes: std::collections::HashSet<Uuid> = self.nodes.values()
            .flatten()
            .copied()
            .collect();
        
        all_nodes.difference(&child_nodes).copied().collect()
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowJob {
    pub job_id: Uuid,
    pub job_type: String,
    pub dependencies: Vec<Uuid>,
    pub dependents: Vec<Uuid>,
    pub status: String,
}
```

### 3.2. Layanan Workflow

```rust
// File: src/services/workflow_service.rs
use crate::{
    models::{Workflow, WorkflowStatus, JobDependency, DependencyType},
    services::{job_service::JobService, job_dependency_service::JobDependencyService},
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use serde_json::Value;

pub struct WorkflowService {
    db: Database, // Gantilah dengan tipe database Anda
    job_service: JobService,
    dependency_service: JobDependencyService,
}

impl WorkflowService {
    pub fn new(
        db: Database,
        job_service: JobService,
        dependency_service: JobDependencyService,
    ) -> Self {
        Self {
            db,
            job_service,
            dependency_service,
        }
    }
    
    pub async fn create_workflow(
        &self,
        name: String,
        definition: Value,
        created_by: Uuid,
        organization_id: Uuid,
    ) -> Result<Workflow, SqlxError> {
        let mut workflow = Workflow::new(name, definition, created_by, organization_id);
        workflow.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        self.db.create_workflow(workflow).await
    }
    
    pub async fn execute_workflow(
        &self,
        workflow_id: Uuid,
    ) -> Result<(), SqlxError> {
        let mut workflow = self.db.get_workflow_by_id(workflow_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if matches!(workflow.status, WorkflowStatus::Running | WorkflowStatus::Succeeded) {
            return Err(SqlxError::Decode("Workflow is already running or completed".into()));
        }
        
        // Parse definisi workflow dan buat job serta ketergantungannya
        self.create_workflow_jobs(&workflow).await?;
        
        // Mulai workflow
        workflow.start();
        self.db.update_workflow(workflow).await?;
        
        Ok(())
    }
    
    async fn create_workflow_jobs(&self, workflow: &Workflow) -> Result<(), SqlxError> {
        let definition = &workflow.definition;
        
        // Ambil daftar job dari definisi
        let jobs = definition.get("jobs")
            .and_then(|j| j.as_array())
            .ok_or(SqlxError::Decode("Invalid workflow definition".into()))?;
        
        // Buat semua job terlebih dahulu
        let mut job_map = std::collections::HashMap::new();
        for job_def in jobs {
            let job_type = job_def.get("type")
                .and_then(|t| t.as_str())
                .ok_or(SqlxError::Decode("Job type is required".into()))?;
            
            let payload = job_def.get("payload")
                .unwrap_or(&serde_json::Value::Null)
                .clone();
            
            let priority = job_def.get("priority")
                .and_then(|p| p.as_i64())
                .unwrap_or(0) as i32;
            
            // Buat job baru
            let new_job = crate::models::Job::new(
                Uuid::new_v4(), // Akan diisi saat pembuatan
                job_type.to_string(),
                payload,
                priority,
                None,
            );
            
            let created_job = self.job_service.create_job(new_job).await?;
            job_map.insert(
                job_def.get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or(""), // Gunakan ID dari definisi
                created_job.id,
            );
        }
        
        // Buat ketergantungan antar job
        let dependencies = definition.get("dependencies")
            .and_then(|d| d.as_array())
            .unwrap_or(&vec![]);
        
        for dep_def in dependencies {
            let parent_id = dep_def.get("from")
                .and_then(|f| f.as_str())
                .ok_or(SqlxError::Decode("Dependency 'from' field is required".into()))?;
            
            let child_id = dep_def.get("to")
                .and_then(|t| t.as_str())
                .ok_or(SqlxError::Decode("Dependency 'to' field is required".into()))?;
            
            let parent_job_id = *job_map.get(parent_id)
                .ok_or(SqlxError::Decode(format!("Parent job {} not found", parent_id).into()))?;
            
            let child_job_id = *job_map.get(child_id)
                .ok_or(SqlxError::Decode(format!("Child job {} not found", child_id).into()))?;
            
            let dependency_type = match dep_def.get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("sequential") {
                "parallel" => DependencyType::Parallel,
                "conditional" => DependencyType::Conditional,
                _ => DependencyType::Sequential,
            };
            
            let condition = dep_def.get("condition")
                .and_then(|c| c.as_str())
                .map(|c| c.to_string());
            
            self.dependency_service
                .create_dependency(parent_job_id, child_job_id, dependency_type, condition)
                .await?;
        }
        
        Ok(())
    }
    
    pub async fn get_workflow_status(
        &self,
        workflow_id: Uuid,
    ) -> Result<WorkflowStatus, SqlxError> {
        let workflow = self.db.get_workflow_by_id(workflow_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        Ok(workflow.status)
    }
    
    pub async fn cancel_workflow(
        &self,
        workflow_id: Uuid,
    ) -> Result<(), SqlxError> {
        let mut workflow = self.db.get_workflow_by_id(workflow_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if matches!(workflow.status, WorkflowStatus::Succeeded | WorkflowStatus::Failed | WorkflowStatus::Cancelled) {
            return Err(SqlxError::Decode("Cannot cancel completed workflow".into()));
        }
        
        // Batalkan semua job dalam workflow
        let workflow_jobs = self.db.get_workflow_jobs(workflow_id).await?;
        for job in workflow_jobs {
            self.job_service.cancel_job(job.job_id).await?;
        }
        
        // Batalkan semua ketergantungan
        self.dependency_service.cancel_job_dependencies(workflow_id).await?;
        
        // Update status workflow
        workflow.cancel();
        self.db.update_workflow(workflow).await?;
        
        Ok(())
    }
    
    pub async fn get_workflow_jobs_status(
        &self,
        workflow_id: Uuid,
    ) -> Result<Vec<JobStatusInfo>, SqlxError> {
        let workflow_jobs = self.db.get_workflow_jobs(workflow_id).await?;
        let mut job_statuses = Vec::new();
        
        for workflow_job in workflow_jobs {
            let job = self.job_service.get_job_by_id(workflow_job.job_id).await?;
            if let Some(job) = job {
                job_statuses.push(JobStatusInfo {
                    job_id: job.id,
                    job_type: job.job_type,
                    status: format!("{:?}", job.status),
                    created_at: job.created_at,
                    updated_at: job.updated_at,
                });
            }
        }
        
        Ok(job_statuses)
    }
}

pub struct JobStatusInfo {
    pub job_id: Uuid,
    pub job_type: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

## 4. Resolving Dependencies dan Job Scheduling

### 4.1. Dependency Resolver

```rust
// File: src/services/dependency_resolver.rs
use crate::{
    models::{Job, JobDependency, DependencyStatus},
    services::{job_service::JobService, job_dependency_service::JobDependencyService},
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};

pub struct DependencyResolver {
    job_service: JobService,
    dependency_service: JobDependencyService,
}

impl DependencyResolver {
    pub fn new(job_service: JobService, dependency_service: JobDependencyService) -> Self {
        Self {
            job_service,
            dependency_service,
        }
    }
    
    pub async fn process_job_completion(
        &self,
        completed_job_id: Uuid,
    ) -> Result<(), SqlxError> {
        // Ambil semua job yang bergantung pada job yang selesai
        let dependent_jobs = self.dependency_service.get_child_dependencies(completed_job_id).await?;
        
        for dependency in dependent_jobs {
            // Periksa apakah ketergantungan terpenuhi
            if dependency.can_execute_child("succeeded") {
                // Update status ketergantungan
                self.dependency_service.resolve_dependency(dependency.id, completed_job_id).await?;
                
                // Periksa apakah semua ketergantungan untuk child job terpenuhi
                let check_result = self.dependency_service.check_job_dependencies(dependency.child_job_id).await?;
                
                if check_result.can_execute {
                    // Mulai eksekusi child job
                    self.start_ready_job(dependency.child_job_id).await?;
                }
            }
        }
        
        Ok(())
    }
    
    pub async fn process_job_failure(
        &self,
        failed_job_id: Uuid,
    ) -> Result<(), SqlxError> {
        // Ambil semua job yang bergantung pada job yang gagal
        let dependent_jobs = self.dependency_service.get_child_dependencies(failed_job_id).await?;
        
        for dependency in dependent_jobs {
            // Periksa apakah ketergantungan bisa dipenuhi dengan kegagalan
            if dependency.can_execute_child("failed") {
                // Update status ketergantungan
                self.dependency_service.resolve_dependency(dependency.id, failed_job_id).await?;
                
                // Periksa apakah semua ketergantungan untuk child job terpenuhi
                let check_result = self.dependency_service.check_job_dependencies(dependency.child_job_id).await?;
                
                if check_result.can_execute {
                    // Mulai eksekusi child job
                    self.start_ready_job(dependency.child_job_id).await?;
                }
            } else {
                // Jika ketergantungan tidak bisa dipenuhi, maka child job juga gagal
                self.mark_job_as_failed(dependency.child_job_id).await?;
            }
        }
        
        Ok(())
    }
    
    async fn start_ready_job(&self, job_id: Uuid) -> Result<(), SqlxError> {
        let mut job = self.job_service.get_job_by_id(job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan job belum dalam status yang tidak sesuai untuk eksekusi
        match job.status {
            crate::models::JobStatus::Pending | crate::models::JobStatus::Scheduled => {
                // Update status job ke processing
                job.status = crate::models::JobStatus::Processing;
                job.updated_at = Utc::now();
                
                self.job_service.update_job(job).await?;
                
                // Dalam implementasi nyata, job ini akan dikirim ke worker
                println!("Job {} is ready to execute", job_id);
            },
            _ => {
                // Job mungkin sudah diproses sebelumnya
                return Ok(());
            }
        }
        
        Ok(())
    }
    
    async fn mark_job_as_failed(&self, job_id: Uuid) -> Result<(), SqlxError> {
        let mut job = self.job_service.get_job_by_id(job_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        job.status = crate::models::JobStatus::Failed;
        job.updated_at = Utc::now();
        
        self.job_service.update_job(job).await?;
        
        // Proses kegagalan ini untuk mempengaruhi job dependen lainnya
        self.process_job_failure(job_id).await?;
        
        Ok(())
    }
    
    pub async fn get_ready_jobs(&self) -> Result<Vec<Job>, SqlxError> {
        let all_jobs = self.job_service.get_all_jobs().await?;
        let mut ready_jobs = Vec::new();
        
        for job in all_jobs {
            if matches!(job.status, crate::models::JobStatus::Pending | crate::models::JobStatus::Scheduled) {
                let check_result = self.dependency_service.check_job_dependencies(job.id).await?;
                if check_result.can_execute {
                    ready_jobs.push(job);
                }
            }
        }
        
        Ok(ready_jobs)
    }
    
    pub async fn get_dependency_chain(
        &self,
        job_id: Uuid,
    ) -> Result<DependencyChain, SqlxError> {
        let mut chain = DependencyChain::new();
        self.build_dependency_chain(job_id, &mut chain).await?;
        Ok(chain)
    }
    
    async fn build_dependency_chain(
        &self,
        job_id: Uuid,
        chain: &mut DependencyChain,
    ) -> Result<(), SqlxError> {
        let dependencies = self.dependency_service.get_parent_dependencies(job_id).await?;
        
        for dependency in dependencies {
            chain.add_dependency(job_id, dependency.parent_job_id);
            
            // Rekursi ke parent job
            self.build_dependency_chain(dependency.parent_job_id, chain).await?;
        }
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DependencyChain {
    pub dependencies: std::collections::HashMap<Uuid, Vec<Uuid>>, // job -> parents
}

impl DependencyChain {
    pub fn new() -> Self {
        Self {
            dependencies: std::collections::HashMap::new(),
        }
    }
    
    pub fn add_dependency(&mut self, job: Uuid, parent: Uuid) {
        self.dependencies.entry(job).or_insert_with(Vec::new).push(parent);
    }
    
    pub fn get_parents(&self, job: Uuid) -> Vec<Uuid> {
        self.dependencies.get(&job).cloned().unwrap_or_default()
    }
}
```

## 5. API Endpoints untuk Ketergantungan

### 5.1. Dependency API Handler

```rust
// File: src/api/dependencies.rs
use axum::{
    extract::{Path, State, Json},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    models::{JobDependency, DependencyType},
    services::{
        authorization_service::AuthorizationService,
        job_dependency_service::JobDependencyService,
    },
};

#[derive(Deserialize)]
pub struct CreateDependencyRequest {
    pub parent_job_id: Uuid,
    pub child_job_id: Uuid,
    pub dependency_type: String, // "sequential", "parallel", "conditional"
    pub condition: Option<String>, // "succeeded", "failed", "always"
}

#[derive(Deserialize)]
pub struct CheckDependenciesRequest {
    pub job_id: Uuid,
}

#[derive(Serialize)]
pub struct CreateDependencyResponse {
    pub id: Uuid,
    pub message: String,
}

#[derive(Serialize)]
pub struct DependencyCheckResponse {
    pub can_execute: bool,
    pub pending_dependencies: u32,
    pub failed_dependencies: u32,
}

#[derive(Serialize)]
pub struct JobDependenciesResponse {
    pub dependencies: Vec<JobDependency>,
    pub total: u32,
}

pub fn create_dependency_router(
    dependency_service: Arc<JobDependencyService>,
    authz_service: Arc<AuthorizationService>,
) -> Router {
    Router::new()
        .route("/", post(create_dependency))
        .route("/check", post(check_dependencies))
        .route("/job/:job_id", get(get_job_dependencies))
        .route("/job/:job_id/parents", get(get_parent_dependencies))
        .route("/job/:job_id/children", get(get_child_dependencies))
        .route("/:dependency_id", delete(delete_dependency))
        .with_state((dependency_service, authz_service))
}

pub async fn create_dependency(
    State((dependency_service, authz_service)): State<(Arc<JobDependencyService>, Arc<AuthorizationService>)>,
    Json(request): Json<CreateDependencyRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "job_dependency:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Validasi dependency type
    let dependency_type = match request.dependency_type.as_str() {
        "sequential" => DependencyType::Sequential,
        "parallel" => DependencyType::Parallel,
        "conditional" => DependencyType::Conditional,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    
    // Ambil informasi job untuk memastikan keduanya milik organisasi yang sama
    let parent_job = dependency_service.job_service.get_job_by_id(request.parent_job_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    let child_job = dependency_service.job_service.get_job_by_id(request.child_job_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan kedua job milik organisasi yang sama
    // Dalam implementasi nyata, kita perlu mengakses queue untuk mendapatkan organisasi
    // For now, we'll assume the jobs exist and continue
    
    match dependency_service.create_dependency(
        request.parent_job_id,
        request.child_job_id,
        dependency_type,
        request.condition,
    ).await {
        Ok(dependency) => {
            let response = CreateDependencyResponse {
                id: dependency.id,
                message: "Dependency created successfully".to_string(),
            };
            Ok(Json(response))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn check_dependencies(
    State((dependency_service, authz_service)): State<(Arc<JobDependencyService>, Arc<AuthorizationService>)>,
    Json(request): Json<CheckDependenciesRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "job_dependency:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Pastikan job milik organisasi user
    let job = dependency_service.job_service.get_job_by_id(request.job_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    match dependency_service.check_job_dependencies(request.job_id).await {
        Ok(check_result) => {
            let response = DependencyCheckResponse {
                can_execute: check_result.can_execute,
                pending_dependencies: check_result.pending_dependencies,
                failed_dependencies: check_result.failed_dependencies,
            };
            Ok(Json(response))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_job_dependencies(
    State((dependency_service, authz_service)): State<(Arc<JobDependencyService>, Arc<AuthorizationService>)>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "job_dependency:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match dependency_service.get_dependencies_for_job(job_id).await {
        Ok(dependencies) => {
            let response = JobDependenciesResponse {
                dependencies,
                total: dependencies.len() as u32,
            };
            Ok(Json(response))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_parent_dependencies(
    State((dependency_service, authz_service)): State<(Arc<JobDependencyService>, Arc<AuthorizationService>)>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "job_dependency:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match dependency_service.get_parent_dependencies(job_id).await {
        Ok(dependencies) => {
            let response = JobDependenciesResponse {
                dependencies,
                total: dependencies.len() as u32,
            };
            Ok(Json(response))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_child_dependencies(
    State((dependency_service, authz_service)): State<(Arc<JobDependencyService>, Arc<AuthorizationService>)>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "job_dependency:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match dependency_service.get_child_dependencies(job_id).await {
        Ok(dependencies) => {
            let response = JobDependenciesResponse {
                dependencies,
                total: dependencies.len() as u32,
            };
            Ok(Json(response))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn delete_dependency(
    State((dependency_service, authz_service)): State<(Arc<JobDependencyService>, Arc<AuthorizationService>)>,
    Path(dependency_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "job_dependency:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let dependency = dependency_service.db.get_job_dependency_by_id(dependency_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan dependency milik organisasi user
    // Dalam implementasi nyata, kita perlu memastikan ini
    
    match dependency_service.db.delete_job_dependency(dependency_id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
```

## 6. Best Practices dan Rekomendasi

### 6.1. Praktik Terbaik untuk Ketergantungan Job

1. **Gunakan Directed Acyclic Graph (DAG)** - untuk mencegah siklus ketergantungan
2. **Terapkan batas kedalaman ketergantungan** - untuk mencegah struktur yang terlalu kompleks
3. **Gunakan monitoring dan alerting** - untuk mendeteksi ketergantungan yang tidak terpenuhi
4. **Gunakan timeout untuk ketergantungan** - untuk mencegah job yang menunggu selamanya
5. **Gunakan penamaan yang konsisten** - untuk ketergantungan dan workflow

### 6.2. Praktik Terbaik untuk Kinerja

1. **Gunakan indexing yang tepat** - pada tabel ketergantungan untuk query yang cepat
2. **Gunakan caching untuk grafik ketergantungan** - untuk menghindari perhitungan berulang
3. **Gunakan batch processing** - untuk memproses banyak ketergantungan sekaligus
4. **Gunakan event-driven architecture** - untuk merespons perubahan status job secara real-time
5. **Gunakan connection pooling** - untuk efisiensi koneksi database

### 6.3. Skalabilitas

1. **Gunakan horizontal scaling** - untuk menangani volume ketergantungan yang tinggi
2. **Gunakan distributed locking** - untuk mencegah race condition
3. **Gunakan message queue** - untuk mendistribusikan pemrosesan ketergantungan
4. **Gunakan sharding** - untuk mendistribusikan data ketergantungan ke beberapa node
5. **Gunakan load balancing** - untuk mendistribusikan permintaan pemrosesan ketergantungan