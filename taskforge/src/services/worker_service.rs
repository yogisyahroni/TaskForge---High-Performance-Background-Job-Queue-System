use crate::{
    database::Database,
    models::{worker::Worker, worker_status::WorkerStatus, worker_type::WorkerType},
    services::authorization_service::AuthorizationService,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc, Duration};

pub struct WorkerService {
    db: Database,
    authz_service: AuthorizationService,
}

impl WorkerService {
    pub fn new(db: Database, authz_service: AuthorizationService) -> Self {
        Self { db, authz_service }
    }
    
    pub async fn register_worker(
        &self,
        project_id: Uuid,
        name: String,
        worker_type: WorkerType,
        max_concurrent_jobs: u32,
        capabilities: serde_json::Value,
        organization_id: Uuid,
    ) -> Result<Worker, SqlxError> {
        // Validasi worker sebelum dibuat
        let worker = Worker::new(
            project_id,
            name,
            worker_type,
            max_concurrent_jobs,
            capabilities,
            organization_id,
        );
        
        worker.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        // Pastikan project milik organisasi yang benar
        let project = self.db.get_project_by_id(project_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        if project.organization_id != organization_id {
            return Err(SqlxError::RowNotFound); // Atau buat error khusus
        }
        
        self.db.create_worker(worker).await
    }
    
    pub async fn get_worker_by_id(&self, worker_id: Uuid) -> Result<Option<Worker>, SqlxError> {
        self.db.get_worker_by_id(worker_id).await
    }
    
    pub async fn get_workers_by_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<Worker>, SqlxError> {
        self.db.get_workers_by_project(project_id).await
    }
    
    pub async fn get_workers_by_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<Worker>, SqlxError> {
        self.db.get_workers_by_organization(organization_id).await
    }
    
    pub async fn update_worker(&self, worker: Worker) -> Result<Worker, SqlxError> {
        worker.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        // Pastikan worker milik organisasi yang benar
        if !self.authz_service.can_access_worker(worker.id, worker.organization_id).await? {
            return Err(SqlxError::RowNotFound); // Atau buat error khusus
        }
        
        self.db.update_worker(worker).await
    }
    
    pub async fn update_worker_status(
        &self,
        worker_id: Uuid,
        status: WorkerStatus,
    ) -> Result<(), SqlxError> {
        let mut worker = self.db.get_worker_by_id(worker_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan worker milik organisasi yang benar
        if !self.authz_service.can_access_worker(worker_id, worker.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        worker.status = status;
        worker.updated_at = Utc::now();
        
        self.db.update_worker(worker).await?;
        Ok(())
    }
    
    pub async fn update_worker_heartbeat(
        &self,
        worker_id: Uuid,
    ) -> Result<(), SqlxError> {
        let mut worker = self.db.get_worker_by_id(worker_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan worker milik organisasi yang benar
        if !self.authz_service.can_access_worker(worker_id, worker.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        worker.last_heartbeat = Some(Utc::now());
        worker.updated_at = Utc::now();
        
        // Jika statusnya offline dan worker mengirim heartbeat, perbarui status
        if matches!(worker.status, WorkerStatus::Offline) {
            worker.status = WorkerStatus::Online;
        }
        
        self.db.update_worker(worker).await?;
        Ok(())
    }
    
    pub async fn enter_drain_mode(&self, worker_id: Uuid) -> Result<(), SqlxError> {
        let mut worker = self.db.get_worker_by_id(worker_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan worker milik organisasi yang benar
        if !self.authz_service.can_access_worker(worker_id, worker.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        worker.drain_mode = true;
        worker.status = WorkerStatus::Draining;
        worker.updated_at = Utc::now();
        
        self.db.update_worker(worker).await?;
        Ok(())
    }
    
    pub async fn exit_drain_mode(&self, worker_id: Uuid) -> Result<(), SqlxError> {
        let mut worker = self.db.get_worker_by_id(worker_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan worker milik organisasi yang benar
        if !self.authz_service.can_access_worker(worker_id, worker.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        worker.drain_mode = false;
        worker.status = WorkerStatus::Online;
        worker.updated_at = Utc::now();
        
        self.db.update_worker(worker).await?;
        Ok(())
    }
    
    pub async fn deregister_worker(&self, worker_id: Uuid) -> Result<(), SqlxError> {
        let worker = self.db.get_worker_by_id(worker_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan worker milik organisasi yang benar
        if !self.authz_service.can_access_worker(worker_id, worker.organization_id).await? {
            return Err(SqlxError::RowNotFound);
        }
        
        // Pastikan worker tidak sedang memproses job
        let active_jobs = self.db.get_active_jobs_for_worker(worker_id).await?;
        if active_jobs > 0 {
            return Err(SqlxError::Decode("Cannot deregister worker with active jobs".into()));
        }
        
        self.db.delete_worker(worker_id).await?;
        Ok(())
    }
    
    pub async fn get_worker_stats(&self, worker_id: Uuid) -> Result<WorkerStats, SqlxError> {
        let worker = self.db.get_worker_by_id(worker_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let stats = self.db.get_worker_statistics(worker_id).await?;
        
        Ok(WorkerStats {
            id: worker.id,
            name: worker.name,
            worker_type: worker.worker_type,
            status: worker.status,
            drain_mode: worker.drain_mode,
            max_concurrent_jobs: worker.max_concurrent_jobs,
            active_jobs: stats.active_jobs,
            completed_jobs: stats.completed_jobs,
            failed_jobs: stats.failed_jobs,
            avg_processing_time_ms: stats.avg_processing_time_ms,
            p95_processing_time_ms: stats.p95_processing_time_ms,
            p99_processing_time_ms: stats.p99_processing_time_ms,
            last_heartbeat: worker.last_heartbeat,
            created_at: worker.created_at,
            updated_at: worker.updated_at,
        })
    }
    
    pub async fn get_worker_health(&self, worker_id: Uuid) -> Result<WorkerHealth, SqlxError> {
        let worker = self.db.get_worker_by_id(worker_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let now = Utc::now();
        let heartbeat_timeout = Duration::seconds(30); // 30 detik
        
        let is_healthy = match worker.last_heartbeat {
            Some(last_hb) => now - last_hb < heartbeat_timeout,
            None => false,
        };
        
        let stats = self.get_worker_stats(worker_id).await?;
        
        Ok(WorkerHealth {
            worker_id: worker.id,
            is_healthy,
            status: if is_healthy { 
                WorkerHealthStatus::Healthy 
            } else { 
                WorkerHealthStatus::Unhealthy 
            },
            stats,
            last_communication: worker.last_heartbeat,
            heartbeat_interval_seconds: 10, // Interval yang diharapkan
        })
    }
    
    pub async fn get_workers_by_queue(
        &self,
        queue_id: Uuid,
    ) -> Result<Vec<Worker>, SqlxError> {
        self.db.get_workers_assigned_to_queue(queue_id).await
    }
    
    pub async fn assign_worker_to_queue(
        &self,
        worker_id: Uuid,
        queue_id: Uuid,
        weight: i32,
    ) -> Result<(), SqlxError> {
        let worker = self.db.get_worker_by_id(worker_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let queue = self.db.get_job_queue_by_id(queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Pastikan worker dan queue milik organisasi yang sama
        if worker.organization_id != queue.organization_id {
            return Err(SqlxError::RowNotFound);
        }
        
        self.db.assign_worker_to_queue(worker_id, queue_id, weight).await?;
        Ok(())
    }
    
    pub async fn unassign_worker_from_queue(
        &self,
        worker_id: Uuid,
        queue_id: Uuid,
    ) -> Result<(), SqlxError> {
        self.db.unassign_worker_from_queue(worker_id, queue_id).await?;
        Ok(())
    }
    
    pub async fn get_available_workers(
        &self,
        queue_id: Uuid,
    ) -> Result<Vec<AvailableWorker>, SqlxError> {
        let queue = self.db.get_job_queue_by_id(queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let assigned_workers = self.db.get_workers_assigned_to_queue(queue_id).await?;
        let mut available_workers = Vec::new();
        
        for worker in assigned_workers {
            if worker.is_available() && worker.can_process_queue(&queue) {
                let current_load = self.db.get_worker_active_job_count(worker.id).await?;
                let is_under_capacity = current_load < worker.max_concurrent_jobs as i64;
                
                if is_under_capacity {
                    available_workers.push(AvailableWorker {
                        worker,
                        current_load: current_load as u32,
                        available_capacity: (worker.max_concurrent_jobs - current_load as u32).max(0),
                    });
                }
            }
        }
        
        Ok(available_workers)
    }
    
    pub async fn get_next_available_worker(
        &self,
        queue_id: Uuid,
    ) -> Result<Option<Worker>, SqlxError> {
        let available_workers = self.get_available_workers(queue_id).await?;
        
        if available_workers.is_empty() {
            return Ok(None);
        }
        
        // Gunakan weighted round-robin untuk memilih worker
        let mut total_weight = 0;
        for worker in &available_workers {
            total_weight += worker.weight; // Asumsikan weight adalah field di assignment
        }
        
        if total_weight == 0 {
            return Ok(available_workers.first().map(|w| w.worker.clone()));
        }
        
        // Dalam implementasi nyata, kita akan menggunakan algoritma load balancing yang lebih canggih
        // Misalnya, berdasarkan beban saat ini, jenis job, dll.
        
        // Untuk sementara, kita pilih worker dengan beban terendah
        let mut min_load = u32::MAX;
        let mut selected_worker = None;
        
        for available_worker in available_workers {
            if available_worker.current_load < min_load {
                min_load = available_worker.current_load;
                selected_worker = Some(available_worker.worker);
            }
        }
        
        Ok(selected_worker)
    }
    
    pub async fn can_access_worker(
        &self,
        user_id: Uuid,
        worker_id: Uuid,
    ) -> Result<bool, SqlxError> {
        if let Some(worker) = self.db.get_worker_by_id(worker_id).await? {
            // Dapatkan project dari worker
            if let Some(project) = self.db.get_project_by_id(worker.project_id).await? {
                // Periksa apakah user adalah bagian dari organisasi yang sama
                self.authz_service.user_belongs_to_organization(user_id, project.organization_id).await
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }
    
    pub async fn get_worker_performance_metrics(
        &self,
        worker_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<WorkerPerformanceMetrics, SqlxError> {
        let metrics = self.db.get_worker_performance_metrics_in_period(worker_id, start_time, end_time).await?;
        
        Ok(WorkerPerformanceMetrics {
            worker_id,
            period_start: start_time,
            period_end: end_time,
            jobs_processed: metrics.jobs_processed,
            jobs_failed: metrics.jobs_failed,
            avg_processing_time_ms: metrics.avg_processing_time_ms,
            p95_processing_time_ms: metrics.p95_processing_time_ms,
            p99_processing_time_ms: metrics.p99_processing_time_ms,
            success_rate_percent: metrics.success_rate_percent,
            cpu_usage_percent: metrics.cpu_usage_percent,
            memory_usage_percent: metrics.memory_usage_percent,
            network_io_mb: metrics.network_io_mb,
        })
    }
    
    pub async fn get_worker_capabilities(&self, worker_id: Uuid) -> Result<WorkerCapabilities, SqlxError> {
        let worker = self.db.get_worker_by_id(worker_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Parsing capabilities dari JSON
        let capabilities: WorkerCapabilities = serde_json::from_value(worker.capabilities)
            .unwrap_or_else(|_| WorkerCapabilities::default());
        
        Ok(capabilities)
    }
}

pub struct WorkerStats {
    pub id: Uuid,
    pub name: String,
    pub worker_type: WorkerType,
    pub status: WorkerStatus,
    pub drain_mode: bool,
    pub max_concurrent_jobs: u32,
    pub active_jobs: u32,
    pub completed_jobs: u64,
    pub failed_jobs: u64,
    pub avg_processing_time_ms: f64,
    pub p95_processing_time_ms: f64,
    pub p99_processing_time_ms: f64,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct WorkerHealth {
    pub worker_id: Uuid,
    pub is_healthy: bool,
    pub status: WorkerHealthStatus,
    pub stats: WorkerStats,
    pub last_communication: Option<DateTime<Utc>>,
    pub heartbeat_interval_seconds: i64,
}

#[derive(Debug, Clone)]
pub enum WorkerHealthStatus {
    Healthy,
    Warning,
    Unhealthy,
    Offline,
}

pub struct AvailableWorker {
    pub worker: Worker,
    pub current_load: u32,
    pub available_capacity: u32,
    pub weight: i32, // Weight dari assignment queue-worker
}

pub struct WorkerPerformanceMetrics {
    pub worker_id: Uuid,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub jobs_processed: u64,
    pub jobs_failed: u64,
    pub avg_processing_time_ms: f64,
    pub p95_processing_time_ms: f64,
    pub p99_processing_time_ms: f64,
    pub success_rate_percent: f64,
    pub cpu_usage_percent: f64,
    pub memory_usage_percent: f64,
    pub network_io_mb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerCapabilities {
    pub supported_job_types: Vec<String>,
    pub cpu_architecture: String,
    pub memory_gb: f64,
    pub specialized_hardware: Option<String>, // "gpu", "fpga", dll.
    pub processing_power_units: f64, // Abstraction untuk kekuatan pemrosesan
    pub network_bandwidth_mbps: f64,
    pub storage_type: String, // "ssd", "hdd", "nvme"
    pub storage_capacity_gb: f64,
    pub os: String,
    pub runtime_environments: Vec<String>, // "rust", "python", "nodejs", dll.
    pub custom_capabilities: serde_json::Value,
}

impl Default for WorkerCapabilities {
    fn default() -> Self {
        Self {
            supported_job_types: vec!["general".to_string()],
            cpu_architecture: "x86_64".to_string(),
            memory_gb: 8.0,
            specialized_hardware: None,
            processing_power_units: 1.0,
            network_bandwidth_mbps: 100.0,
            storage_type: "ssd".to_string(),
            storage_capacity_gb: 100.0,
            os: "linux".to_string(),
            runtime_environments: vec!["general".to_string()],
            custom_capabilities: serde_json::json!({}),
        }
    }
}

impl WorkerCapabilities {
    pub fn supports_job_type(&self, job_type: &str) -> bool {
        self.supported_job_types.iter().any(|jt| jt == job_type)
    }
    
    pub fn can_handle_job(&self, job_requirements: &JobRequirements) -> bool {
        // Periksa apakah worker memiliki kemampuan yang diperlukan untuk job
        if let Some(required_job_types) = &job_requirements.required_job_types {
            if !required_job_types.iter().all(|jt| self.supports_job_type(jt)) {
                return false;
            }
        }
        
        if job_requirements.min_memory_gb > self.memory_gb {
            return false;
        }
        
        if let Some(ref hardware_req) = job_requirements.specialized_hardware {
            if self.specialized_hardware.as_ref() != Some(hardware_req) {
                return false;
            }
        }
        
        if let Some(ref runtime_req) = job_requirements.runtime_environment {
            if !self.runtime_environments.contains(runtime_req) {
                return false;
            }
        }
        
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequirements {
    pub required_job_types: Option<Vec<String>>,
    pub min_memory_gb: f64,
    pub specialized_hardware: Option<String>,
    pub runtime_environment: Option<String>,
    pub max_processing_time_seconds: Option<u64>,
    pub requires_gpu: bool,
    pub requires_high_network_bandwidth: bool,
    pub requires_large_storage: bool,
}

impl JobRequirements {
    pub fn new() -> Self {
        Self {
            required_job_types: None,
            min_memory_gb: 0.0,
            specialized_hardware: None,
            runtime_environment: None,
            max_processing_time_seconds: None,
            requires_gpu: false,
            requires_high_network_bandwidth: false,
            requires_large_storage: false,
        }
    }
    
    pub fn with_job_types(mut self, job_types: Vec<String>) -> Self {
        self.required_job_types = Some(job_types);
        self
    }
    
    pub fn with_min_memory_gb(mut self, memory_gb: f64) -> Self {
        self.min_memory_gb = memory_gb;
        self
    }
    
    pub fn requiring_gpu(mut self) -> Self {
        self.requires_gpu = true;
        self
    }
    
    pub fn requiring_specialized_hardware(mut self, hardware: String) -> Self {
        self.specialized_hardware = Some(hardware);
        self
    }
    
    pub fn with_runtime_environment(mut self, runtime: String) -> Self {
        self.runtime_environment = Some(runtime);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Worker, WorkerType, WorkerStatus};
    use uuid::Uuid;
    
    #[tokio::test]
    async fn test_create_worker() {
        // Dalam implementasi nyata, kita akan menggunakan mock database
        // Untuk sekarang, kita hanya menguji logika validasi
        
        let worker = Worker::new(
            Uuid::new_v4(), // project_id
            "test_worker".to_string(),
            WorkerType::General,
            5, // max_concurrent_jobs
            serde_json::json!({"capability": "general"}),
            Uuid::new_v4(), // organization_id
        );
        
        assert_eq!(worker.name, "test_worker");
        assert_eq!(worker.worker_type, WorkerType::General);
        assert_eq!(worker.max_concurrent_jobs, 5);
        assert_eq!(worker.status, WorkerStatus::Offline); // Status default
    }
    
    #[test]
    fn test_worker_capabilities() {
        let capabilities = WorkerCapabilities {
            supported_job_types: vec!["email".to_string(), "image_processing".to_string()],
            memory_gb: 16.0,
            specialized_hardware: Some("gpu".to_string()),
            runtime_environments: vec!["rust".to_string(), "python".to_string()],
            ..Default::default()
        };
        
        let job_reqs = JobRequirements::new()
            .with_job_types(vec!["email".to_string()])
            .with_min_memory_gb(8.0)
            .requiring_specialized_hardware("gpu".to_string());
        
        assert!(capabilities.can_handle_job(&job_reqs));
        
        // Test dengan requirement yang tidak terpenuhi
        let job_reqs_too_much_memory = JobRequirements::new()
            .with_min_memory_gb(32.0); // Lebih dari yang tersedia
        
        assert!(!capabilities.can_handle_job(&job_reqs_too_much_memory));
    }
}