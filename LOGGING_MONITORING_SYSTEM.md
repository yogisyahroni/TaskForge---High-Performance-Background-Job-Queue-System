# Sistem Logging dan Monitoring untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan sistem logging dan monitoring dalam aplikasi TaskForge. Sistem ini bertanggung jawab untuk mencatat aktivitas aplikasi, menyediakan visibilitas terhadap kesehatan sistem, serta menyediakan alat untuk troubleshooting dan analisis kinerja.

## 2. Arsitektur Sistem Logging

### 2.1. Konsep Dasar Logging

TaskForge menerapkan pendekatan structured logging menggunakan format JSON untuk memudahkan parsing dan analisis. Sistem logging dirancang untuk:

- Menyediakan visibilitas penuh terhadap aktivitas sistem
- Mendukung debugging dan troubleshooting
- Menyediakan audit trail untuk keamanan
- Mendukung analisis kinerja dan penggunaan

### 2.2. Model Log

```rust
// File: src/models/log_entry.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "log_level", rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LogEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub service: String,              // Nama service (e.g., "queue_service", "worker_service")
    pub module: String,               // Nama modul atau komponen
    pub operation: String,            // Operasi yang sedang dilakukan
    pub message: String,              // Pesan log utama
    pub trace_id: Option<String>,     // ID untuk melacak request secara keseluruhan
    pub span_id: Option<String>,      // ID untuk melacak span dalam request
    pub correlation_id: Option<String>, // ID untuk menghubungkan log terkait
    pub user_id: Option<Uuid>,        // ID pengguna (jika ada)
    pub organization_id: Option<Uuid>, // ID organisasi (untuk multi-tenant)
    pub metadata: Value,              // Data tambahan dalam format JSON
    pub error_details: Option<Value>, // Detail error jika level Error atau Critical
}

impl LogEntry {
    pub fn new(
        level: LogLevel,
        service: String,
        module: String,
        operation: String,
        message: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            level,
            service,
            module,
            operation,
            message,
            trace_id: None,
            span_id: None,
            correlation_id: None,
            user_id: None,
            organization_id: None,
            metadata: serde_json::json!({}),
            error_details: None,
        }
    }
    
    pub fn with_trace(mut self, trace_id: String) -> Self {
        self.trace_id = Some(trace_id);
        self
    }
    
    pub fn with_correlation(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }
    
    pub fn with_user(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }
    
    pub fn with_organization(mut self, organization_id: Uuid) -> Self {
        self.organization_id = Some(organization_id);
        self
    }
    
    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = metadata;
        self
    }
    
    pub fn with_error_details(mut self, error_details: Value) -> Self {
        self.error_details = Some(error_details);
        self
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.service.is_empty() || self.service.len() > 100 {
            return Err("Service name must be between 1 and 100 characters".to_string());
        }
        
        if self.module.is_empty() || self.module.len() > 100 {
            return Err("Module name must be between 1 and 100 characters".to_string());
        }
        
        if self.operation.is_empty() || self.operation.len() > 100 {
            return Err("Operation name must be between 1 and 100 characters".to_string());
        }
        
        if self.message.is_empty() || self.message.len() > 10000 {
            return Err("Message must be between 1 and 10000 characters".to_string());
        }
        
        Ok(())
    }
}
```

## 3. Sistem Logging Terdistribusi

### 3.1. Layanan Logging

```rust
// File: src/services/logging_service.rs
use crate::{
    database::Database,
    models::{LogEntry, LogLevel},
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct LoggingService {
    db: Database,
    log_buffer: Arc<RwLock<Vec<LogEntry>>>,
    buffer_size: usize,
    flush_interval: tokio::time::Duration,
}

impl LoggingService {
    pub fn new(db: Database, buffer_size: usize, flush_interval_seconds: u64) -> Self {
        let service = Self {
            db,
            log_buffer: Arc::new(RwLock::new(Vec::new())),
            buffer_size,
            flush_interval: tokio::time::Duration::from_secs(flush_interval_seconds),
        };
        
        // Mulai background task untuk flush log
        service.start_flush_task();
        
        service
    }
    
    fn start_flush_task(&self) {
        let log_buffer = Arc::clone(&self.log_buffer);
        let db = self.db.clone();
        let buffer_size = self.buffer_size;
        
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                
                let mut buffer = log_buffer.write().await;
                if buffer.len() >= buffer_size {
                    let logs_to_flush = buffer.split_off(0);
                    drop(buffer); // Lepaskan lock sebelum flush ke database
                    
                    if let Err(e) = Self::flush_logs_to_db(&db, logs_to_flush).await {
                        eprintln!("Failed to flush logs to database: {}", e);
                        // Dalam implementasi nyata, mungkin perlu mencoba menyimpan ke file atau sistem backup
                    }
                } else {
                    drop(buffer);
                }
            }
        });
    }
    
    async fn flush_logs_to_db(db: &Database, logs: Vec<LogEntry>) -> Result<(), SqlxError> {
        if logs.is_empty() {
            return Ok(());
        }
        
        // Gunakan batch insert untuk efisiensi
        for log in logs {
            db.insert_log_entry(log).await?;
        }
        
        Ok(())
    }
    
    pub async fn log(
        &self,
        level: LogLevel,
        service: &str,
        module: &str,
        operation: &str,
        message: &str,
    ) -> Result<(), SqlxError> {
        let log_entry = LogEntry::new(
            level,
            service.to_string(),
            module.to_string(),
            operation.to_string(),
            message.to_string(),
        );
        
        self.write_log(log_entry).await
    }
    
    pub async fn log_with_context(
        &self,
        level: LogLevel,
        service: &str,
        module: &str,
        operation: &str,
        message: &str,
        metadata: Value,
    ) -> Result<(), SqlxError> {
        let mut log_entry = LogEntry::new(
            level,
            service.to_string(),
            module.to_string(),
            operation.to_string(),
            message.to_string(),
        );
        log_entry.metadata = metadata;
        
        self.write_log(log_entry).await
    }
    
    pub async fn log_error_with_details(
        &self,
        service: &str,
        module: &str,
        operation: &str,
        message: &str,
        error_details: Value,
    ) -> Result<(), SqlxError> {
        let mut log_entry = LogEntry::new(
            LogLevel::Error,
            service.to_string(),
            module.to_string(),
            operation.to_string(),
            message.to_string(),
        );
        log_entry.error_details = Some(error_details);
        
        self.write_log(log_entry).await
    }
    
    async fn write_log(&self, log_entry: LogEntry) -> Result<(), SqlxError> {
        log_entry.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        // Tambahkan ke buffer
        {
            let mut buffer = self.log_buffer.write().await;
            buffer.push(log_entry);
        }
        
        Ok(())
    }
    
    pub async fn query_logs(
        &self,
        service: Option<&str>,
        level: Option<LogLevel>,
        start_time: Option<chrono::DateTime<Utc>>,
        end_time: Option<chrono::DateTime<Utc>>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<LogEntry>, SqlxError> {
        self.db.query_logs(service, level, start_time, end_time, limit, offset).await
    }
    
    pub async fn get_log_stats(
        &self,
        service: &str,
        start_time: chrono::DateTime<Utc>,
        end_time: chrono::DateTime<Utc>,
    ) -> Result<LogStats, SqlxError> {
        let total_logs = self.db.get_log_count(service, start_time, end_time).await?;
        let error_logs = self.db.get_log_count_by_level(service, LogLevel::Error, start_time, end_time).await?;
        let warn_logs = self.db.get_log_count_by_level(service, LogLevel::Warn, start_time, end_time).await?;
        
        Ok(LogStats {
            service: service.to_string(),
            total_logs: total_logs as u64,
            error_logs: error_logs as u64,
            warn_logs: warn_logs as u64,
            start_time,
            end_time,
        })
    }
}

pub struct LogStats {
    pub service: String,
    pub total_logs: u64,
    pub error_logs: u64,
    pub warn_logs: u64,
    pub start_time: chrono::DateTime<Utc>,
    pub end_time: chrono::DateTime<Utc>,
}
```

### 3.2. Tracing dan Correlation

```rust
// File: src/services/tracing_service.rs
use uuid::Uuid;
use std::collections::HashMap;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};
use serde_json::Value;

pub struct TracingService {
    active_traces: RwLock<HashMap<String, TraceInfo>>,
}

#[derive(Debug, Clone)]
pub struct TraceInfo {
    pub trace_id: String,
    pub start_time: DateTime<Utc>,
    pub spans: Vec<SpanInfo>,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
pub struct SpanInfo {
    pub span_id: String,
    pub operation: String,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub metadata: Value,
}

impl TracingService {
    pub fn new() -> Self {
        Self {
            active_traces: RwLock::new(HashMap::new()),
        }
    }
    
    pub async fn start_trace(&self, operation: &str, metadata: Value) -> String {
        let trace_id = uuid::Uuid::new_v4().to_string();
        let span_id = uuid::Uuid::new_v4().to_string();
        
        let trace_info = TraceInfo {
            trace_id: trace_id.clone(),
            start_time: Utc::now(),
            spans: vec![SpanInfo {
                span_id: span_id.clone(),
                operation: operation.to_string(),
                start_time: Utc::now(),
                end_time: None,
                metadata,
            }],
            metadata: serde_json::json!({}),
        };
        
        let mut traces = self.active_traces.write().await;
        traces.insert(trace_id.clone(), trace_info);
        
        trace_id
    }
    
    pub async fn start_span(&self, trace_id: &str, operation: &str, metadata: Value) -> Option<String> {
        let span_id = uuid::Uuid::new_v4().to_string();
        
        let mut traces = self.active_traces.write().await;
        if let Some(trace_info) = traces.get_mut(trace_id) {
            trace_info.spans.push(SpanInfo {
                span_id: span_id.clone(),
                operation: operation.to_string(),
                start_time: Utc::now(),
                end_time: None,
                metadata,
            });
            Some(span_id)
        } else {
            None
        }
    }
    
    pub async fn end_span(&self, trace_id: &str, span_id: &str) -> bool {
        let mut traces = self.active_traces.write().await;
        if let Some(trace_info) = traces.get_mut(trace_id) {
            for span in &mut trace_info.spans {
                if span.span_id == span_id && span.end_time.is_none() {
                    span.end_time = Some(Utc::now());
                    return true;
                }
            }
        }
        false
    }
    
    pub async fn end_trace(&self, trace_id: &str) -> Option<TraceInfo> {
        let mut traces = self.active_traces.write().await;
        traces.remove(trace_id)
    }
    
    pub async fn get_trace(&self, trace_id: &str) -> Option<TraceInfo> {
        let traces = self.active_traces.read().await;
        traces.get(trace_id).cloned()
    }
    
    pub async fn get_active_traces(&self) -> Vec<TraceInfo> {
        let traces = self.active_traces.read().await;
        traces.values().cloned().collect()
    }
}
```

## 4. Sistem Monitoring Aplikasi

### 4.1. Layanan Monitoring

```rust
// File: src/services/monitoring_service.rs
use crate::database::Database;
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct MonitoringService {
    db: Database,
    metrics: RwLock<HashMap<String, MetricValue>>,
}

#[derive(Debug, Clone)]
pub enum MetricValue {
    Counter(u64),
    Gauge(f64),
    Histogram(Vec<u64>),
    Timer(Vec<chrono::Duration>),
}

impl MetricValue {
    pub fn increment_counter(&mut self) {
        if let MetricValue::Counter(ref mut value) = self {
            *value += 1;
        }
    }
    
    pub fn update_gauge(&mut self, value: f64) {
        if let MetricValue::Gauge(ref mut current) = self {
            *current = value;
        }
    }
    
    pub fn record_histogram(&mut self, value: u64) {
        if let MetricValue::Histogram(ref mut values) = self {
            values.push(value);
        }
    }
    
    pub fn record_timer(&mut self, duration: chrono::Duration) {
        if let MetricValue::Timer(ref mut values) = self {
            values.push(duration);
        }
    }
}

impl MonitoringService {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            metrics: RwLock::new(HashMap::new()),
        }
    }
    
    pub async fn increment_counter(&self, metric_name: &str) {
        let mut metrics = self.metrics.write().await;
        match metrics.get_mut(metric_name) {
            Some(MetricValue::Counter(ref mut value)) => *value += 1,
            _ => {
                metrics.insert(metric_name.to_string(), MetricValue::Counter(1));
            }
        }
    }
    
    pub async fn update_gauge(&self, metric_name: &str, value: f64) {
        let mut metrics = self.metrics.write().await;
        metrics.insert(metric_name.to_string(), MetricValue::Gauge(value));
    }
    
    pub async fn record_histogram(&self, metric_name: &str, value: u64) {
        let mut metrics = self.metrics.write().await;
        match metrics.get_mut(metric_name) {
            Some(MetricValue::Histogram(ref mut values)) => values.push(value),
            _ => {
                metrics.insert(metric_name.to_string(), MetricValue::Histogram(vec![value]));
            }
        }
    }
    
    pub async fn record_timer(&self, metric_name: &str, duration: chrono::Duration) {
        let mut metrics = self.metrics.write().await;
        match metrics.get_mut(metric_name) {
            Some(MetricValue::Timer(ref mut values)) => values.push(duration),
            _ => {
                metrics.insert(metric_name.to_string(), MetricValue::Timer(vec![duration]));
            }
        }
    }
    
    pub async fn get_metric(&self, metric_name: &str) -> Option<MetricValue> {
        let metrics = self.metrics.read().await;
        metrics.get(metric_name).cloned()
    }
    
    pub async fn get_all_metrics(&self) -> HashMap<String, MetricValue> {
        let metrics = self.metrics.read().await;
        metrics.clone()
    }
    
    pub async fn get_system_health(&self) -> Result<SystemHealth, SqlxError> {
        // Cek koneksi database
        let db_healthy = self.db.test_connection().await.is_ok();
        
        // Dapatkan metrik sistem
        let queue_stats = self.db.get_queue_statistics().await?;
        let worker_stats = self.db.get_worker_statistics().await?;
        let job_stats = self.db.get_job_statistics().await?;
        
        Ok(SystemHealth {
            timestamp: Utc::now(),
            database_healthy: db_healthy,
            queue_stats,
            worker_stats,
            job_stats,
        })
    }
    
    pub async fn get_service_metrics(
        &self,
        service_name: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<ServiceMetrics, SqlxError> {
        let request_count = self.db.get_request_count(service_name, start_time, end_time).await?;
        let error_count = self.db.get_error_count(service_name, start_time, end_time).await?;
        let avg_response_time = self.db.get_avg_response_time(service_name, start_time, end_time).await?;
        let p95_response_time = self.db.get_p95_response_time(service_name, start_time, end_time).await?;
        let p99_response_time = self.db.get_p99_response_time(service_name, start_time, end_time).await?;
        
        let error_rate = if request_count > 0 {
            (error_count as f64 / request_count as f64) * 100.0
        } else {
            0.0
        };
        
        Ok(ServiceMetrics {
            service_name: service_name.to_string(),
            request_count: request_count as u64,
            error_count: error_count as u64,
            error_rate,
            avg_response_time,
            p95_response_time,
            p99_response_time,
            period_start: start_time,
            period_end: end_time,
        })
    }
}

pub struct SystemHealth {
    pub timestamp: DateTime<Utc>,
    pub database_healthy: bool,
    pub queue_stats: QueueStatistics,
    pub worker_stats: WorkerStatistics,
    pub job_stats: JobStatistics,
}

pub struct QueueStatistics {
    pub total_queues: u64,
    pub active_queues: u64,
    pub paused_queues: u64,
}

pub struct WorkerStatistics {
    pub total_workers: u64,
    pub online_workers: u64,
    pub offline_workers: u64,
    pub draining_workers: u64,
}

pub struct JobStatistics {
    pub total_jobs: u64,
    pub pending_jobs: u64,
    pub processing_jobs: u64,
    pub succeeded_jobs: u64,
    pub failed_jobs: u64,
}

pub struct ServiceMetrics {
    pub service_name: String,
    pub request_count: u64,
    pub error_count: u64,
    pub error_rate: f64,  // dalam persen
    pub avg_response_time: f64,  // dalam milidetik
    pub p95_response_time: f64,  // dalam milidetik
    pub p99_response_time: f64,  // dalam milidetik
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}
```

### 4.2. Health Check Service

```rust
// File: src/services/health_check_service.rs
use crate::database::Database;
use sqlx::Error as SqlxError;
use std::collections::HashMap;

pub struct HealthCheckService {
    db: Database,
    checks: HashMap<String, Box<dyn HealthCheck>>,
}

#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync {
    async fn check(&self) -> HealthStatus;
}

pub struct HealthStatus {
    pub name: String,
    pub status: HealthState,
    pub details: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub enum HealthState {
    Healthy,
    Unhealthy,
    Degraded,
}

impl HealthCheckService {
    pub fn new(db: Database) -> Self {
        let mut service = Self {
            db,
            checks: HashMap::new(),
        };
        
        // Register default health checks
        service.register_check("database", Box::new(DatabaseHealthCheck::new(db)));
        service.register_check("memory", Box::new(MemoryHealthCheck::new()));
        service.register_check("disk", Box::new(DiskHealthCheck::new()));
        
        service
    }
    
    pub fn register_check(&mut self, name: &str, check: Box<dyn HealthCheck>) {
        self.checks.insert(name.to_string(), check);
    }
    
    pub async fn run_all_checks(&self) -> Vec<HealthStatus> {
        let mut results = Vec::new();
        
        for (name, check) in &self.checks {
            let mut status = check.check().await;
            status.name = name.clone();
            results.push(status);
        }
        
        results
    }
    
    pub async fn get_overall_health(&self) -> HealthStatus {
        let results = self.run_all_checks().await;
        
        let mut overall_status = HealthState::Healthy;
        let mut details = Vec::new();
        
        for result in results {
            match result.status {
                HealthState::Unhealthy => {
                    overall_status = HealthState::Unhealthy;
                    if let Some(detail) = result.details {
                        details.push(detail);
                    }
                },
                HealthState::Degraded if overall_status == HealthState::Healthy => {
                    overall_status = HealthState::Degraded;
                    if let Some(detail) = result.details {
                        details.push(detail);
                    }
                },
                _ => {}
            }
        }
        
        HealthStatus {
            name: "overall".to_string(),
            status: overall_status,
            details: Some(details.join(", ")),
            timestamp: chrono::Utc::now(),
        }
    }
}

// Implementasi health check spesifik
pub struct DatabaseHealthCheck {
    db: Database,
}

impl DatabaseHealthCheck {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl HealthCheck for DatabaseHealthCheck {
    async fn check(&self) -> HealthStatus {
        match self.db.test_connection().await {
            Ok(_) => HealthStatus {
                name: "database".to_string(),
                status: HealthState::Healthy,
                details: Some("Database connection successful".to_string()),
                timestamp: chrono::Utc::now(),
            },
            Err(e) => HealthStatus {
                name: "database".to_string(),
                status: HealthState::Unhealthy,
                details: Some(format!("Database connection failed: {}", e)),
                timestamp: chrono::Utc::now(),
            },
        }
    }
}

pub struct MemoryHealthCheck;

impl MemoryHealthCheck {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl HealthCheck for MemoryHealthCheck {
    async fn check(&self) -> HealthStatus {
        // Dalam implementasi nyata, ini akan memeriksa penggunaan memori sistem
        // Untuk contoh, kita asumsikan selalu healthy
        HealthStatus {
            name: "memory".to_string(),
            status: HealthState::Healthy,
            details: Some("Memory usage within normal limits".to_string()),
            timestamp: chrono::Utc::now(),
        }
    }
}

pub struct DiskHealthCheck;

impl DiskHealthCheck {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl HealthCheck for DiskHealthCheck {
    async fn check(&self) -> HealthStatus {
        // Dalam implementasi nyata, ini akan memeriksa penggunaan disk
        // Untuk contoh, kita asumsikan selalu healthy
        HealthStatus {
            name: "disk".to_string(),
            status: HealthState::Healthy,
            details: Some("Disk usage within normal limits".to_string()),
            timestamp: chrono::Utc::now(),
        }
    }
}
```

## 5. Integrasi dengan Sistem Eksternal

### 5.1. Exporter untuk Prometheus

```rust
// File: src/services/prometheus_exporter.rs
use crate::services::monitoring_service::MonitoringService;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct PrometheusExporter {
    monitoring_service: Arc<MonitoringService>,
    metrics_cache: Arc<RwLock<HashMap<String, String>>>,
}

impl PrometheusExporter {
    pub fn new(monitoring_service: Arc<MonitoringService>) -> Self {
        Self {
            monitoring_service,
            metrics_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub async fn generate_metrics(&self) -> String {
        let mut metrics = String::new();
        
        // Tambahkan metrik sistem
        metrics.push_str("# HELP taskforge_queue_count Total number of queues\n");
        metrics.push_str("# TYPE taskforge_queue_count gauge\n");
        // Dalam implementasi nyata, ini akan mendapatkan nilai dari monitoring service
        
        metrics.push_str("# HELP taskforge_job_count Total number of jobs\n");
        metrics.push_str("# TYPE taskforge_job_count gauge\n");
        // Dalam implementasi nyata, ini akan mendapatkan nilai dari monitoring service
        
        metrics.push_str("# HELP taskforge_worker_count Total number of workers\n");
        metrics.push_str("# TYPE taskforge_worker_count gauge\n");
        // Dalam implementasi nyata, ini akan mendapatkan nilai dari monitoring service
        
        // Tambahkan metrik kustom berdasarkan yang ada di monitoring service
        let all_metrics = self.monitoring_service.get_all_metrics().await;
        for (name, value) in all_metrics {
            match value {
                crate::services::monitoring_service::MetricValue::Counter(count) => {
                    metrics.push_str(&format!("taskforge_{}_total {}\n", name, count));
                },
                crate::services::monitoring_service::MetricValue::Gauge(gauge) => {
                    metrics.push_str(&format!("taskforge_{}_gauge {}\n", name, gauge));
                },
                _ => {} // Histogram dan Timer perlu penanganan khusus
            }
        }
        
        metrics
    }
    
    pub async fn get_metrics_for_prometheus(&self) -> String {
        self.generate_metrics().await
    }
}
```

### 5.2. Sink Logging untuk Sistem Eksternal

```rust
// File: src/services/logging_sink_service.rs
use crate::models::LogEntry;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct LoggingSinkService {
    log_tx: mpsc::UnboundedSender<LogEntry>,
    sinks: Vec<Arc<dyn LogSink>>,
}

#[async_trait::async_trait]
pub trait LogSink: Send + Sync {
    async fn write_log(&self, log_entry: &LogEntry) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

impl LoggingSinkService {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<LogEntry>) {
        let (log_tx, log_rx) = mpsc::unbounded_channel();
        
        let service = Self {
            log_tx,
            sinks: Vec::new(),
        };
        
        (service, log_rx)
    }
    
    pub fn add_sink(&mut self, sink: Arc<dyn LogSink>) {
        self.sinks.push(sink);
    }
    
    pub fn log(&self, log_entry: LogEntry) -> Result<(), mpsc::error::SendError<LogEntry>> {
        self.log_tx.send(log_entry)
    }
    
    pub async fn start_processing(&self, mut log_rx: mpsc::UnboundedReceiver<LogEntry>) {
        while let Some(log_entry) = log_rx.recv().await {
            for sink in &self.sinks {
                if let Err(e) = sink.write_log(&log_entry).await {
                    eprintln!("Failed to write log to sink: {}", e);
                }
            }
        }
    }
}

// Contoh implementasi sink untuk file
pub struct FileLogSink {
    file_path: String,
}

impl FileLogSink {
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl LogSink for FileLogSink {
    async fn write_log(&self, log_entry: &LogEntry) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use tokio::fs::OpenOptions;
        use tokio::io::{AsyncWriteExt, BufWriter};
        
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)
            .await?;
        
        let mut writer = BufWriter::new(file);
        let log_json = serde_json::to_string(log_entry)?;
        writer.write_all((log_json + "\n").as_bytes()).await?;
        writer.flush().await?;
        
        Ok(())
    }
}

// Contoh implementasi sink untuk sistem logging eksternal (seperti ELK stack)
pub struct ExternalLogSink {
    endpoint: String,
    api_key: String,
}

impl ExternalLogSink {
    pub fn new(endpoint: &str, api_key: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            api_key: api_key.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl LogSink for ExternalLogSink {
    async fn write_log(&self, log_entry: &LogEntry) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = reqwest::Client::new();
        let response = client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(log_entry)
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(format!("Failed to send log: {}", response.status()).into());
        }
        
        Ok(())
    }
}
```

## 6. Alerting dan Notifikasi

### 6.1. Sistem Alert

```rust
// File: src/services/alert_service.rs
use crate::{
    models::LogEntry,
    services::{logging_service::LoggingService, monitoring_service::MonitoringService},
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::HashMap;

pub struct AlertService {
    logging_service: LoggingService,
    monitoring_service: MonitoringService,
    rules: Vec<AlertRule>,
    active_alerts: HashMap<String, Alert>,
}

#[derive(Debug, Clone)]
pub struct AlertRule {
    pub id: String,
    pub name: String,
    pub condition: AlertCondition,
    pub severity: AlertSeverity,
    pub notification_channels: Vec<NotificationChannel>,
    pub cooldown_period: chrono::Duration, // Waktu minimum antar notifikasi
}

#[derive(Debug, Clone)]
pub enum AlertCondition {
    LogBased { level: crate::models::LogLevel, count_threshold: u32, time_window: chrono::Duration },
    MetricBased { metric_name: String, threshold: f64, operator: ComparisonOperator },
    Custom { query: String }, // Query khusus untuk kondisi kompleks
}

#[derive(Debug, Clone)]
pub enum ComparisonOperator {
    GreaterThan,
    LessThan,
    Equal,
    NotEqual,
}

#[derive(Debug, Clone)]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone)]
pub enum NotificationChannel {
    Email { recipients: Vec<String> },
    Slack { webhook_url: String },
    Webhook { url: String },
    PagerDuty { integration_key: String },
}

#[derive(Debug, Clone)]
pub struct Alert {
    pub id: String,
    pub rule_id: String,
    pub timestamp: DateTime<Utc>,
    pub severity: AlertSeverity,
    pub message: String,
    pub details: Value,
    pub status: AlertStatus,
    pub last_notified: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub enum AlertStatus {
    Active,
    Resolved,
    Acknowledged,
}

impl AlertService {
    pub fn new(
        logging_service: LoggingService,
        monitoring_service: MonitoringService,
    ) -> Self {
        Self {
            logging_service,
            monitoring_service,
            rules: Vec::new(),
            active_alerts: HashMap::new(),
        }
    }
    
    pub fn add_rule(&mut self, rule: AlertRule) {
        self.rules.push(rule);
    }
    
    pub async fn evaluate_alerts(&mut self) -> Result<(), sqlx::Error> {
        for rule in &self.rules {
            if self.should_trigger_alert(rule).await? {
                self.trigger_alert(rule).await?;
            }
        }
        
        Ok(())
    }
    
    async fn should_trigger_alert(&self, rule: &AlertRule) -> Result<bool, sqlx::Error> {
        match &rule.condition {
            AlertCondition::LogBased { level, count_threshold, time_window } => {
                let start_time = Utc::now() - *time_window;
                let log_count = self.logging_service
                    .db
                    .get_log_count_by_level("all", level.clone(), start_time, Utc::now())
                    .await?;
                
                Ok(log_count >= *count_threshold as i64)
            },
            AlertCondition::MetricBased { metric_name, threshold, operator } => {
                if let Some(metric) = self.monitoring_service.get_metric(metric_name).await {
                    match metric {
                        crate::services::monitoring_service::MetricValue::Gauge(value) => {
                            match operator {
                                ComparisonOperator::GreaterThan => Ok(value > *threshold),
                                ComparisonOperator::LessThan => Ok(value < *threshold),
                                ComparisonOperator::Equal => Ok((value - *threshold).abs() < f64::EPSILON),
                                ComparisonOperator::NotEqual => Ok((value - *threshold).abs() >= f64::EPSILON),
                            }
                        },
                        crate::services::monitoring_service::MetricValue::Counter(count) => {
                            match operator {
                                ComparisonOperator::GreaterThan => Ok(count as f64 > *threshold),
                                ComparisonOperator::LessThan => Ok(count as f64 < *threshold),
                                ComparisonOperator::Equal => Ok((count as f64 - *threshold).abs() < f64::EPSILON),
                                ComparisonOperator::NotEqual => Ok((count as f64 - *threshold).abs() >= f64::EPSILON),
                            }
                        },
                        _ => Ok(false), // Tipe metrik lain belum didukung
                    }
                } else {
                    Ok(false)
                }
            },
            AlertCondition::Custom { .. } => {
                // Dalam implementasi nyata, ini akan mengevaluasi query khusus
                Ok(false)
            }
        }
    }
    
    async fn trigger_alert(&mut self, rule: &AlertRule) -> Result<(), sqlx::Error> {
        let alert_id = uuid::Uuid::new_v4().to_string();
        let alert = Alert {
            id: alert_id.clone(),
            rule_id: rule.id.clone(),
            timestamp: Utc::now(),
            severity: rule.severity.clone(),
            message: format!("Alert triggered: {}", rule.name),
            details: serde_json::json!({}),
            status: AlertStatus::Active,
            last_notified: None,
        };
        
        // Simpan alert ke database
        self.logging_service.db.insert_alert(alert.clone()).await?;
        
        // Kirim notifikasi
        self.send_notifications(&alert, &rule.notification_channels).await?;
        
        // Tambahkan ke daftar alert aktif
        self.active_alerts.insert(alert_id, alert);
        
        Ok(())
    }
    
    async fn send_notifications(
        &self,
        alert: &Alert,
        channels: &[NotificationChannel],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for channel in channels {
            match channel {
                NotificationChannel::Email { recipients } => {
                    self.send_email_notification(alert, recipients).await?;
                },
                NotificationChannel::Slack { webhook_url } => {
                    self.send_slack_notification(alert, webhook_url).await?;
                },
                NotificationChannel::Webhook { url } => {
                    self.send_webhook_notification(alert, url).await?;
                },
                NotificationChannel::PagerDuty { integration_key } => {
                    self.send_pagerduty_notification(alert, integration_key).await?;
                },
            }
        }
        
        Ok(())
    }
    
    async fn send_email_notification(
        &self,
        alert: &Alert,
        recipients: &[String],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Implementasi pengiriman email
        // Dalam implementasi nyata, ini akan menggunakan library seperti lettre
        println!("Sending email alert to: {:?}, message: {}", recipients, alert.message);
        Ok(())
    }
    
    async fn send_slack_notification(
        &self,
        alert: &Alert,
        webhook_url: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "text": format!("TaskForge Alert: {}", alert.message),
            "attachments": [{
                "color": match alert.severity {
                    AlertSeverity::Critical => "danger",
                    AlertSeverity::Error => "danger",
                    AlertSeverity::Warning => "warning",
                    AlertSeverity::Info => "good",
                },
                "fields": [
                    {
                        "title": "Severity",
                        "value": format!("{:?}", alert.severity),
                        "short": true
                    },
                    {
                        "title": "Timestamp",
                        "value": alert.timestamp.to_rfc3339(),
                        "short": true
                    }
                ]
            }]
        });
        
        client
            .post(webhook_url)
            .json(&payload)
            .send()
            .await?;
        
        Ok(())
    }
    
    async fn send_webhook_notification(
        &self,
        alert: &Alert,
        url: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = reqwest::Client::new();
        client
            .post(url)
            .json(alert)
            .send()
            .await?;
        
        Ok(())
    }
    
    async fn send_pagerduty_notification(
        &self,
        alert: &Alert,
        integration_key: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "routing_key": integration_key,
            "event_action": "trigger",
            "payload": {
                "summary": alert.message,
                "source": "taskforge",
                "severity": match alert.severity {
                    AlertSeverity::Critical => "critical",
                    AlertSeverity::Error => "error", 
                    AlertSeverity::Warning => "warning",
                    AlertSeverity::Info => "info",
                },
                "custom_details": alert.details,
            }
        });
        
        client
            .post("https://events.pagerduty.com/v2/enqueue")
            .json(&payload)
            .send()
            .await?;
        
        Ok(())
    }
}
```

## 7. Best Practices dan Rekomendasi

### 7.1. Praktik Terbaik untuk Logging

1. **Gunakan structured logging** - dengan format JSON untuk kemudahan parsing
2. **Sertakan konteks yang relevan** - seperti trace ID, user ID, dan organisasi
3. **Gunakan level log yang tepat** - untuk membedakan jenis pesan
4. **Batasi ukuran pesan log** - untuk mencegah pemborosan penyimpanan
5. **Enkripsi log sensitif** - untuk menjaga privasi data

### 7.2. Praktik Terbaik untuk Monitoring

1. **Gunakan metrik yang bermakna** - yang mencerminkan kesehatan sistem
2. **Terapkan alerting yang bijak** - hindari alert fatigue
3. **Gunakan dashboard yang informatif** - untuk visibilitas real-time
4. **Simpan data historis** - untuk analisis tren
5. **Gunakan sampling untuk metrik tingkat tinggi** - untuk efisiensi

### 7.3. Skala dan Kinerja

1. **Gunakan buffering untuk logging** - untuk mengurangi beban I/O
2. **Gunakan asynchronous logging** - untuk tidak menghambat thread utama
3. **Gunakan indexing yang tepat** - pada tabel log untuk query yang cepat
4. **Gunakan partisi tabel** - untuk mengelola data log dengan efisien
5. **Gunakan retention policy** - untuk mengelola siklus hidup data log