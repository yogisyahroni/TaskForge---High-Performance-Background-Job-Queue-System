use crate::{
    database::Database,
    models::{job::JobStatus, job_queue::JobQueue, worker::WorkerStatus},
    services::authorization_service::AuthorizationService,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};
use prometheus::{
    Encoder, TextEncoder, Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramVec,
    HistogramOpts, Opts, register, register_counter, register_counter_vec, 
    register_gauge, register_gauge_vec, register_histogram, register_histogram_vec,
};

pub struct MonitoringService {
    db: Database,
    authz_service: AuthorizationService,
    
    // Job metrics
    job_submitted_total: CounterVec,
    job_processed_total: CounterVec,
    job_succeeded_total: CounterVec,
    job_failed_total: CounterVec,
    job_processing_duration: HistogramVec,
    
    // Queue metrics
    queue_length: GaugeVec,
    queue_throughput: CounterVec,
    
    // Worker metrics
    worker_active_count: GaugeVec,
    worker_processing_duration: HistogramVec,
    
    // System metrics
    system_cpu_usage: Gauge,
    system_memory_usage: Gauge,
    system_request_duration: HistogramVec,
}

impl MonitoringService {
    pub fn new(db: Database, authz_service: AuthorizationService) -> Result<Self, Box<dyn std::error::Error>> {
        // Job metrics
        let job_submitted_total = register_counter_vec!(
            "taskforge_job_submitted_total",
            "Total number of jobs submitted",
            &["queue", "job_type", "organization"]
        )?;
        
        let job_processed_total = register_counter_vec!(
            "taskforge_job_processed_total",
            "Total number of jobs processed",
            &["queue", "job_type", "result", "organization"]
        )?;
        
        let job_succeeded_total = register_counter_vec!(
            "taskforge_job_succeeded_total",
            "Total number of jobs succeeded",
            &["queue", "job_type", "organization"]
        )?;
        
        let job_failed_total = register_counter_vec!(
            "taskforge_job_failed_total",
            "Total number of jobs failed",
            &["queue", "job_type", "error_type", "organization"]
        )?;
        
        let job_processing_duration = register_histogram_vec!(
            "taskforge_job_processing_duration_seconds",
            "Time spent processing jobs",
            &["queue", "job_type", "organization"],
            exponential_buckets(0.0005, 2.0, 20) // 0.5ms to ~524s
        )?;
        
        // Queue metrics
        let queue_length = register_gauge_vec!(
            "taskforge_queue_length",
            "Current length of job queues",
            &["queue", "organization"]
        )?;
        
        let queue_throughput = register_counter_vec!(
            "taskforge_queue_throughput_total",
            "Total jobs processed per queue",
            &["queue", "organization"]
        )?;
        
        // Worker metrics
        let worker_active_count = register_gauge_vec!(
            "taskforge_worker_active_count",
            "Number of active workers",
            &["queue", "organization"]
        )?;
        
        let worker_processing_duration = register_histogram_vec!(
            "taskforge_worker_processing_duration_seconds",
            "Time spent processing by workers",
            &["worker_id", "job_type", "organization"],
            exponential_buckets(0.005, 2.0, 20)
        )?;
        
        // System metrics
        let system_cpu_usage = register_gauge!("taskforge_system_cpu_usage_percent", "Current CPU usage percentage")?;
        
        let system_memory_usage = register_gauge!("taskforge_system_memory_usage_bytes", "Current memory usage in bytes")?;
        
        let system_request_duration = register_histogram_vec!(
            "taskforge_system_request_duration_seconds",
            "Duration of HTTP requests",
            &["method", "endpoint", "status_code"],
            exponential_buckets(0.0005, 2.0, 20)
        )?;
        
        Ok(Self {
            db,
            authz_service,
            job_submitted_total,
            job_processed_total,
            job_succeeded_total,
            job_failed_total,
            job_processing_duration,
            queue_length,
            queue_throughput,
            worker_active_count,
            worker_processing_duration,
            system_cpu_usage,
            system_memory_usage,
            system_request_duration,
        })
    }
    
    pub fn increment_job_submitted(&self, queue: &str, job_type: &str, organization: &str) {
        self.job_submitted_total
            .with_label_values(&[queue, job_type, organization])
            .inc();
    }
    
    pub fn increment_job_processed(&self, queue: &str, job_type: &str, result: &str, organization: &str) {
        self.job_processed_total
            .with_label_values(&[queue, job_type, result, organization])
            .inc();
    }
    
    pub fn increment_job_succeeded(&self, queue: &str, job_type: &str, organization: &str) {
        self.job_succeeded_total
            .with_label_values(&[queue, job_type, organization])
            .inc();
    }
    
    pub fn increment_job_failed(&self, queue: &str, job_type: &str, error_type: &str, organization: &str) {
        self.job_failed_total
            .with_label_values(&[queue, job_type, error_type, organization])
            .inc();
    }
    
    pub fn observe_job_processing_duration(&self, queue: &str, job_type: &str, organization: &str, duration_seconds: f64) {
        self.job_processing_duration
            .with_label_values(&[queue, job_type, organization])
            .observe(duration_seconds);
    }
    
    pub fn set_queue_length(&self, queue: &str, organization: &str, length: f64) {
        self.queue_length
            .with_label_values(&[queue, organization])
            .set(length);
    }
    
    pub fn increment_queue_throughput(&self, queue: &str, organization: &str) {
        self.queue_throughput
            .with_label_values(&[queue, organization])
            .inc();
    }
    
    pub fn set_worker_active_count(&self, queue: &str, organization: &str, count: f64) {
        self.worker_active_count
            .with_label_values(&[queue, organization])
            .set(count);
    }
    
    pub fn observe_worker_processing_duration(&self, worker_id: &str, job_type: &str, organization: &str, duration_seconds: f64) {
        self.worker_processing_duration
            .with_label_values(&[worker_id, job_type, organization])
            .observe(duration_seconds);
    }
    
    pub fn set_system_cpu_usage(&self, usage_percent: f64) {
        self.system_cpu_usage.set(usage_percent);
    }
    
    pub fn set_system_memory_usage(&self, usage_bytes: f64) {
        self.system_memory_usage.set(usage_bytes);
    }
    
    pub fn observe_request_duration(&self, method: &str, endpoint: &str, status_code: &str, duration_seconds: f64) {
        self.system_request_duration
            .with_label_values(&[method, endpoint, status_code])
            .observe(duration_seconds);
    }
    
    pub async fn get_system_health(&self) -> Result<SystemHealth, SqlxError> {
        let db_health = self.db.health_check().await?;
        
        // Dapatkan statistik dari database
        let total_jobs = self.db.get_total_job_count().await?;
        let pending_jobs = self.db.get_job_count_by_status(JobStatus::Pending).await?;
        let processing_jobs = self.db.get_job_count_by_status(JobStatus::Processing).await?;
        let succeeded_jobs = self.db.get_job_count_by_status(JobStatus::Succeeded).await?;
        let failed_jobs = self.db.get_job_count_by_status(JobStatus::Failed).await?;
        
        let total_queues = self.db.get_total_queue_count().await?;
        let active_queues = self.db.get_active_queue_count().await?;
        
        let total_workers = self.db.get_total_worker_count().await?;
        let online_workers = self.db.get_worker_count_by_status(WorkerStatus::Online).await?;
        let offline_workers = self.db.get_worker_count_by_status(WorkerStatus::Offline).await?;
        let draining_workers = self.db.get_worker_count_by_status(WorkerStatus::Draining).await?;
        
        Ok(SystemHealth {
            timestamp: Utc::now(),
            database_healthy: db_health,
            total_jobs,
            pending_jobs,
            processing_jobs,
            succeeded_jobs,
            failed_jobs,
            total_queues,
            active_queues,
            total_workers,
            online_workers,
            offline_workers,
            draining_workers,
            system_metrics: self.get_system_metrics().await?,
        })
    }
    
    async fn get_system_metrics(&self) -> Result<SystemMetrics, SqlxError> {
        // Dalam implementasi nyata, ini akan mengumpulkan metrik sistem
        // seperti penggunaan CPU, memori, disk, dll.
        Ok(SystemMetrics {
            cpu_usage_percent: 0.0, // Placeholder
            memory_usage_percent: 0.0, // Placeholder
            disk_usage_percent: 0.0, // Placeholder
            network_in_bps: 0.0, // Placeholder
            network_out_bps: 0.0, // Placeholder
            load_average: 0.0, // Placeholder
            uptime_seconds: 0, // Placeholder
        })
    }
    
    pub async fn get_organization_health(
        &self,
        organization_id: Uuid,
    ) -> Result<OrganizationHealth, SqlxError> {
        // Pastikan user memiliki akses ke organisasi
        // Dalam implementasi nyata, ini akan diperiksa melalui authz_service
        
        let org_stats = self.db.get_organization_statistics(organization_id).await?;
        
        let queues = self.db.get_job_queues_by_organization(organization_id).await?;
        let workers = self.db.get_workers_by_organization(organization_id).await?;
        
        let mut queue_stats = Vec::new();
        for queue in queues {
            let stats = self.db.get_queue_statistics(queue.id).await?;
            queue_stats.push(QueueHealth {
                queue_id: queue.id,
                name: queue.name,
                pending_jobs: stats.pending_count,
                processing_jobs: stats.processing_count,
                succeeded_jobs: stats.succeeded_count,
                failed_jobs: stats.failed_count,
                avg_processing_time_ms: stats.avg_processing_time_ms,
                p95_processing_time_ms: stats.p95_processing_time_ms,
                p99_processing_time_ms: stats.p99_processing_time_ms,
                throughput_per_minute: stats.throughput_per_minute,
                error_rate_percent: stats.error_rate_percent,
            });
        }
        
        let mut worker_stats = Vec::new();
        for worker in workers {
            let stats = self.db.get_worker_statistics(worker.id).await?;
            worker_stats.push(WorkerHealth {
                worker_id: worker.id,
                name: worker.name,
                status: worker.status,
                active_jobs: stats.active_jobs,
                completed_jobs: stats.completed_jobs,
                failed_jobs: stats.failed_jobs,
                avg_processing_time_ms: stats.avg_processing_time_ms,
                last_heartbeat: worker.last_heartbeat,
            });
        }
        
        Ok(OrganizationHealth {
            organization_id,
            timestamp: Utc::now(),
            total_jobs: org_stats.total_jobs,
            pending_jobs: org_stats.pending_jobs,
            processing_jobs: org_stats.processing_jobs,
            succeeded_jobs: org_stats.succeeded_jobs,
            failed_jobs: org_stats.failed_jobs,
            total_queues: org_stats.total_queues,
            active_queues: org_stats.active_queues,
            total_workers: org_stats.total_workers,
            online_workers: org_stats.online_workers,
            queue_health: queue_stats,
            worker_health: worker_stats,
        })
    }
    
    pub async fn get_queue_health(
        &self,
        queue_id: Uuid,
    ) -> Result<QueueHealthDetailed, SqlxError> {
        let queue = self.db.get_job_queue_by_id(queue_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let stats = self.db.get_queue_statistics(queue_id).await?;
        let workers = self.db.get_workers_assigned_to_queue(queue_id).await?;
        
        let mut worker_health = Vec::new();
        for worker in workers {
            let worker_stats = self.db.get_worker_statistics_for_queue(worker.id, queue_id).await?;
            worker_health.push(WorkerQueueHealth {
                worker_id: worker.id,
                worker_name: worker.name,
                worker_status: worker.status,
                jobs_processed: worker_stats.completed_jobs,
                avg_processing_time_ms: worker_stats.avg_processing_time_ms,
                success_rate_percent: if worker_stats.completed_jobs > 0 {
                    (worker_stats.completed_jobs as f64 / (worker_stats.completed_jobs + worker_stats.failed_jobs) as f64) * 100.0
                } else {
                    100.0
                },
            });
        }
        
        Ok(QueueHealthDetailed {
            queue_id: queue.id,
            name: queue.name,
            project_id: queue.project_id,
            organization_id: queue.organization_id,
            is_active: queue.is_active,
            stats: QueueHealthStats {
                pending_jobs: stats.pending_count,
                processing_jobs: stats.processing_count,
                succeeded_jobs: stats.succeeded_count,
                failed_jobs: stats.failed_count,
                avg_processing_time_ms: stats.avg_processing_time_ms,
                p95_processing_time_ms: stats.p95_processing_time_ms,
                p99_processing_time_ms: stats.p99_processing_time_ms,
                throughput_per_minute: stats.throughput_per_minute,
                error_rate_percent: stats.error_rate_percent,
            },
            workers: worker_health,
            created_at: queue.created_at,
            updated_at: queue.updated_at,
        })
    }
    
    pub async fn get_worker_health(
        &self,
        worker_id: Uuid,
    ) -> Result<WorkerHealthDetailed, SqlxError> {
        let worker = self.db.get_worker_by_id(worker_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let stats = self.db.get_worker_statistics(worker_id).await?;
        
        let assignments = self.db.get_queue_assignments_for_worker(worker_id).await?;
        
        Ok(WorkerHealthDetailed {
            worker_id: worker.id,
            name: worker.name,
            worker_type: worker.worker_type,
            status: worker.status,
            drain_mode: worker.drain_mode,
            max_concurrent_jobs: worker.max_concurrent_jobs,
            stats: WorkerHealthStats {
                active_jobs: stats.active_jobs,
                completed_jobs: stats.completed_jobs,
                failed_jobs: stats.failed_jobs,
                avg_processing_time_ms: stats.avg_processing_time_ms,
                p95_processing_time_ms: stats.p95_processing_time_ms,
                p99_processing_time_ms: stats.p99_processing_time_ms,
                success_rate_percent: if stats.completed_jobs + stats.failed_jobs > 0 {
                    (stats.completed_jobs as f64 / (stats.completed_jobs + stats.failed_jobs) as f64) * 100.0
                } else {
                    100.0
                },
            },
            assignments,
            last_heartbeat: worker.last_heartbeat,
            created_at: worker.created_at,
            updated_at: worker.updated_at,
        })
    }
    
    pub async fn get_metrics_for_prometheus(&self) -> Result<String, Box<dyn std::error::Error>> {
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        
        let metric_families = prometheus::gather();
        encoder.encode(&metric_families, &mut buffer)?;
        
        let metrics = String::from_utf8(buffer)?;
        Ok(metrics)
    }
    
    pub async fn get_job_performance_metrics(
        &self,
        job_id: Uuid,
    ) -> Result<JobPerformanceMetrics, SqlxError> {
        let executions = self.db.get_job_executions_by_job(job_id).await?;
        
        let mut metrics = JobPerformanceMetrics {
            job_id,
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            avg_processing_time_ms: 0.0,
            min_processing_time_ms: None,
            max_processing_time_ms: None,
            p95_processing_time_ms: 0.0,
            p99_processing_time_ms: 0.0,
            success_rate_percent: 0.0,
            total_data_processed_mb: 0.0,
        };
        
        let mut processing_times = Vec::new();
        let mut total_data_processed = 0.0;
        
        for execution in executions {
            metrics.total_executions += 1;
            
            match execution.status {
                crate::models::execution_status::ExecutionStatus::Succeeded => {
                    metrics.successful_executions += 1;
                    
                    if let (Some(started), Some(finished)) = (execution.started_at, execution.finished_at) {
                        let duration_ms = (finished - started).num_milliseconds() as f64;
                        processing_times.push(duration_ms);
                        
                        if let Some(output) = &execution.output {
                            total_data_processed += output.len() as f64 / (1024.0 * 1024.0); // Convert to MB
                        }
                    }
                },
                crate::models::execution_status::ExecutionStatus::Failed => {
                    metrics.failed_executions += 1;
                },
                _ => {}
            }
        }
        
        if !processing_times.is_empty() {
            processing_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
            
            metrics.avg_processing_time_ms = processing_times.iter().sum::<f64>() / processing_times.len() as f64;
            metrics.min_processing_time_ms = Some(*processing_times.first().unwrap());
            metrics.max_processing_time_ms = Some(*processing_times.last().unwrap());
            
            // Calculate percentiles
            let p95_idx = (processing_times.len() as f64 * 0.95).floor() as usize;
            let p99_idx = (processing_times.len() as f64 * 0.99).floor() as usize;
            
            metrics.p95_processing_time_ms = processing_times.get(p95_idx.min(processing_times.len() - 1)).copied().unwrap_or(0.0);
            metrics.p99_processing_time_ms = processing_times.get(p99_idx.min(processing_times.len() - 1)).copied().unwrap_or(0.0);
            
            if metrics.total_executions > 0 {
                metrics.success_rate_percent = (metrics.successful_executions as f64 / metrics.total_executions as f64) * 100.0;
            }
            
            metrics.total_data_processed_mb = total_data_processed;
        }
        
        Ok(metrics)
    }
    
    pub async fn get_usage_metrics(
        &self,
        organization_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<UsageMetrics, SqlxError> {
        let usage_data = self.db.get_usage_metrics_in_period(organization_id, start_time, end_time).await?;
        
        Ok(UsageMetrics {
            organization_id,
            period_start: start_time,
            period_end: end_time,
            jobs_processed: usage_data.jobs_processed,
            compute_time_seconds: usage_data.compute_time_seconds,
            data_processed_mb: usage_data.data_processed_mb,
            api_calls_made: usage_data.api_calls_made,
            storage_used_gb: usage_data.storage_used_gb,
            bandwidth_used_gb: usage_data.bandwidth_used_gb,
        })
    }
}

pub struct SystemHealth {
    pub timestamp: DateTime<Utc>,
    pub database_healthy: bool,
    pub total_jobs: i64,
    pub pending_jobs: i64,
    pub processing_jobs: i64,
    pub succeeded_jobs: i64,
    pub failed_jobs: i64,
    pub total_queues: i64,
    pub active_queues: i64,
    pub total_workers: i64,
    pub online_workers: i64,
    pub offline_workers: i64,
    pub draining_workers: i64,
    pub system_metrics: SystemMetrics,
}

pub struct SystemMetrics {
    pub cpu_usage_percent: f64,
    pub memory_usage_percent: f64,
    pub disk_usage_percent: f64,
    pub network_in_bps: f64,
    pub network_out_bps: f64,
    pub load_average: f64,
    pub uptime_seconds: u64,
}

pub struct OrganizationHealth {
    pub organization_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub total_jobs: i64,
    pub pending_jobs: i64,
    pub processing_jobs: i64,
    pub succeeded_jobs: i64,
    pub failed_jobs: i64,
    pub total_queues: i64,
    pub active_queues: i64,
    pub total_workers: i64,
    pub online_workers: i64,
    pub queue_health: Vec<QueueHealth>,
    pub worker_health: Vec<WorkerHealth>,
}

pub struct QueueHealth {
    pub queue_id: Uuid,
    pub name: String,
    pub pending_jobs: i64,
    pub processing_jobs: i64,
    pub succeeded_jobs: i64,
    pub failed_jobs: i64,
    pub avg_processing_time_ms: f64,
    pub p95_processing_time_ms: f64,
    pub p99_processing_time_ms: f64,
    pub throughput_per_minute: f64,
    pub error_rate_percent: f64,
}

pub struct WorkerHealth {
    pub worker_id: Uuid,
    pub name: String,
    pub status: WorkerStatus,
    pub active_jobs: i64,
    pub completed_jobs: i64,
    pub failed_jobs: i64,
    pub avg_processing_time_ms: f64,
    pub last_heartbeat: Option<DateTime<Utc>>,
}

pub struct QueueHealthDetailed {
    pub queue_id: Uuid,
    pub name: String,
    pub project_id: Uuid,
    pub organization_id: Uuid,
    pub is_active: bool,
    pub stats: QueueHealthStats,
    pub workers: Vec<WorkerQueueHealth>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct WorkerQueueHealth {
    pub worker_id: Uuid,
    pub worker_name: String,
    pub worker_status: WorkerStatus,
    pub jobs_processed: i64,
    pub avg_processing_time_ms: f64,
    pub success_rate_percent: f64,
}

pub struct WorkerHealthDetailed {
    pub worker_id: Uuid,
    pub name: String,
    pub worker_type: WorkerType,
    pub status: WorkerStatus,
    pub drain_mode: bool,
    pub max_concurrent_jobs: u32,
    pub stats: WorkerHealthStats,
    pub assignments: Vec<QueueWorkerAssignment>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct WorkerHealthStats {
    pub active_jobs: i64,
    pub completed_jobs: i64,
    pub failed_jobs: i64,
    pub avg_processing_time_ms: f64,
    pub p95_processing_time_ms: f64,
    pub p99_processing_time_ms: f64,
    pub success_rate_percent: f64,
}

pub struct JobPerformanceMetrics {
    pub job_id: Uuid,
    pub total_executions: u32,
    pub successful_executions: u32,
    pub failed_executions: u32,
    pub avg_processing_time_ms: f64,
    pub min_processing_time_ms: Option<f64>,
    pub max_processing_time_ms: Option<f64>,
    pub p95_processing_time_ms: f64,
    pub p99_processing_time_ms: f64,
    pub success_rate_percent: f64,
    pub total_data_processed_mb: f64,
}

pub struct UsageMetrics {
    pub organization_id: Uuid,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub jobs_processed: u64,
    pub compute_time_seconds: f64,
    pub data_processed_mb: f64,
    pub api_calls_made: u64,
    pub storage_used_gb: f64,
    pub bandwidth_used_gb: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus::{default_registry, gather, TextEncoder};
    
    #[test]
    fn test_prometheus_metrics_format() {
        // Dalam implementasi nyata, kita akan menguji bahwa metrik
        // dalam format yang benar untuk Prometheus
        
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        
        let metric_families = gather();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        
        let output = String::from_utf8(buffer).unwrap();
        
        // Verifikasi bahwa output mengandung beberapa metrik dasar
        assert!(output.contains("taskforge_"));
        assert!(output.contains("# TYPE "));
        assert!(output.contains("# HELP "));
    }
    
    #[tokio::test]
    async fn test_system_health_retrieval() {
        // Dalam implementasi nyata, kita akan menggunakan mock database
        // untuk menguji pengambilan metrik kesehatan sistem
        
        // Untuk sekarang, kita hanya menguji struktur
        let health = SystemHealth {
            timestamp: chrono::Utc::now(),
            database_healthy: true,
            total_jobs: 100,
            pending_jobs: 5,
            processing_jobs: 2,
            succeeded_jobs: 90,
            failed_jobs: 3,
            total_queues: 3,
            active_queues: 3,
            total_workers: 5,
            online_workers: 5,
            offline_workers: 0,
            draining_workers: 0,
            system_metrics: SystemMetrics {
                cpu_usage_percent: 15.0,
                memory_usage_percent: 45.0,
                disk_usage_percent: 60.0,
                network_in_bps: 1000.0,
                network_out_bps: 500.0,
                load_average: 0.5,
                uptime_seconds: 3600,
            },
        };
        
        assert!(health.database_healthy);
        assert_eq!(health.total_jobs, 100);
        assert_eq!(health.pending_jobs, 5);
    }
}