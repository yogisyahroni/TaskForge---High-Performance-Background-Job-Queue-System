# Sistem Metrics (Prometheus) untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan sistem metrics dan monitoring berbasis Prometheus dalam aplikasi TaskForge. Sistem ini bertanggung jawab untuk mengumpulkan, menyajikan, dan mengekspor metrik kunci yang memungkinkan pengawasan kinerja aplikasi secara real-time dan historis.

## 2. Konsep Dasar Metrics dan Monitoring

### 2.1. Jenis-jenis Metrik Prometheus

Prometheus mendukung empat jenis metrik utama:

1. **Counter**: Metrik yang hanya bisa bertambah atau direset ke nol
2. **Gauge**: Metrik yang bisa bertambah atau berkurang
3. **Histogram**: Metrik yang menghitung distribusi nilai observasi
4. **Summary**: Mirip histogram tetapi menghitung quantiles secara server-side

### 2.2. Metrik Inti untuk TaskForge

TaskForge akan mengekspos metrik-metrik berikut:

- **Job Metrics**: Jumlah job yang diproses, gagal, berhasil, dalam antrian
- **Queue Metrics**: Panjang antrian, throughput, latency
- **Worker Metrics**: Jumlah worker aktif, beban kerja, tingkat keberhasilan
- **System Metrics**: Penggunaan CPU, memori, disk, koneksi database
- **API Metrics**: Jumlah request, response time, error rate

## 3. Implementasi Metrics Core

### 3.1. Model Metrics

```rust
// File: src/models/metrics.rs
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Metric {
    pub id: Uuid,
    pub name: String,           // Nama metrik (misal: "job_processed_total")
    pub labels: serde_json::Value, // Label dalam format JSON (misal: {"queue": "default", "job_type": "email"})
    pub value: f64,            // Nilai metrik
    pub metric_type: MetricType, // Jenis metrik (counter, gauge, histogram, summary)
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Summary,
}

impl Metric {
    pub fn new_counter(name: &str, labels: serde_json::Value, value: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            labels,
            value,
            metric_type: MetricType::Counter,
            timestamp: Utc::now(),
        }
    }
    
    pub fn new_gauge(name: &str, labels: serde_json::Value, value: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            labels,
            value,
            metric_type: MetricType::Gauge,
            timestamp: Utc::now(),
        }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 200 {
            return Err("Metric name must be between 1 and 200 characters".to_string());
        }
        
        // Validasi nama metrik sesuai konvensi Prometheus
        if !self.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':') {
            return Err("Metric name can only contain alphanumeric characters, underscores, and colons".to_string());
        }
        
        Ok(())
    }
}
```

### 3.2. Registry Metrics

```rust
// File: src/services/metrics_registry.rs
use crate::models::metrics::Metric;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use prometheus::{
    Encoder, TextEncoder, Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramVec,
    HistogramOpts, Opts, register_counter, register_counter_vec, register_gauge, 
    register_gauge_vec, register_histogram_vec,
};

pub struct MetricsRegistry {
    // Job metrics
    pub job_submitted_total: CounterVec,
    pub job_processed_total: CounterVec,
    pub job_failed_total: CounterVec,
    pub job_processing_duration: HistogramVec,
    
    // Queue metrics
    pub queue_length: GaugeVec,
    pub queue_throughput: CounterVec,
    
    // Worker metrics
    pub worker_active_count: GaugeVec,
    pub worker_processing_duration: HistogramVec,
    
    // System metrics
    pub system_cpu_usage: Gauge,
    pub system_memory_usage: Gauge,
    pub system_request_duration: HistogramVec,
    
    // Custom metrics storage
    custom_metrics: Arc<RwLock<HashMap<String, Metric>>>,
}

impl MetricsRegistry {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Job metrics
        let job_submitted_total = register_counter_vec!(
            "taskforge_job_submitted_total",
            "Total number of jobs submitted",
            &["queue", "job_type"]
        )?;
        
        let job_processed_total = register_counter_vec!(
            "taskforge_job_processed_total",
            "Total number of jobs processed",
            &["queue", "job_type", "result"]
        )?;
        
        let job_failed_total = register_counter_vec!(
            "taskforge_job_failed_total",
            "Total number of jobs failed",
            &["queue", "job_type", "error_type"]
        )?;
        
        let job_processing_duration = register_histogram_vec!(
            "taskforge_job_processing_duration_seconds",
            "Time spent processing jobs",
            &["queue", "job_type"],
            exponential_buckets(0.0005, 2.0, 20) // 0.5ms to ~524s
        )?;
        
        // Queue metrics
        let queue_length = register_gauge_vec!(
            "taskforge_queue_length",
            "Current length of job queues",
            &["queue"]
        )?;
        
        let queue_throughput = register_counter_vec!(
            "taskforge_queue_throughput_total",
            "Total jobs processed per queue",
            &["queue"]
        )?;
        
        // Worker metrics
        let worker_active_count = register_gauge_vec!(
            "taskforge_worker_active_count",
            "Number of active workers",
            &["queue"]
        )?;
        
        let worker_processing_duration = register_histogram_vec!(
            "taskforge_worker_processing_duration_seconds",
            "Time spent processing by workers",
            &["worker_id", "job_type"],
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
            job_submitted_total,
            job_processed_total,
            job_failed_total,
            job_processing_duration,
            queue_length,
            queue_throughput,
            worker_active_count,
            worker_processing_duration,
            system_cpu_usage,
            system_memory_usage,
            system_request_duration,
            custom_metrics: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    pub fn encode_metrics(&self) -> Result<String, Box<dyn std::error::Error>> {
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        
        let metric_families = prometheus::gather();
        encoder.encode(&metric_families, &mut buffer)?;
        
        Ok(String::from_utf8(buffer)?)
    }
    
    pub async fn add_custom_metric(&self, metric: Metric) -> Result<(), String> {
        metric.validate()?;
        
        let mut custom_metrics = self.custom_metrics.write().await;
        custom_metrics.insert(metric.name.clone(), metric);
        
        Ok(())
    }
    
    pub async fn get_custom_metric(&self, name: &str) -> Option<Metric> {
        let custom_metrics = self.custom_metrics.read().await;
        custom_metrics.get(name).cloned()
    }
    
    pub async fn get_all_custom_metrics(&self) -> Vec<Metric> {
        let custom_metrics = self.custom_metrics.read().await;
        custom_metrics.values().cloned().collect()
    }
}

// Fungsi pembantu untuk membuat buckets eksponensial
fn exponential_buckets(start: f64, factor: f64, count: usize) -> Vec<f64> {
    let mut buckets = Vec::with_capacity(count);
    let mut current = start;
    
    for _ in 0..count {
        buckets.push(current);
        current *= factor;
    }
    
    buckets
}
```

## 4. Layanan Metrics

### 4.1. Layanan Metrics Utama

```rust
// File: src/services/metrics_service.rs
use crate::{
    models::metrics::Metric,
    services::metrics_registry::MetricsRegistry,
};
use uuid::Uuid;
use std::sync::Arc;

pub struct MetricsService {
    registry: Arc<MetricsRegistry>,
}

impl MetricsService {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        Self { registry }
    }
    
    // Metrik Job
    pub fn increment_job_submitted(&self, queue: &str, job_type: &str) {
        self.registry.job_submitted_total
            .with_label_values(&[queue, job_type])
            .inc();
    }
    
    pub fn increment_job_processed(&self, queue: &str, job_type: &str, result: &str) {
        self.registry.job_processed_total
            .with_label_values(&[queue, job_type, result])
            .inc();
    }
    
    pub fn increment_job_failed(&self, queue: &str, job_type: &str, error_type: &str) {
        self.registry.job_failed_total
            .with_label_values(&[queue, job_type, error_type])
            .inc();
    }
    
    pub fn observe_job_processing_duration(&self, queue: &str, job_type: &str, duration_seconds: f64) {
        self.registry.job_processing_duration
            .with_label_values(&[queue, job_type])
            .observe(duration_seconds);
    }
    
    // Metrik Queue
    pub fn set_queue_length(&self, queue: &str, length: f64) {
        self.registry.queue_length
            .with_label_values(&[queue])
            .set(length);
    }
    
    pub fn increment_queue_throughput(&self, queue: &str) {
        self.registry.queue_throughput
            .with_label_values(&[queue])
            .inc();
    }
    
    // Metrik Worker
    pub fn set_worker_active_count(&self, queue: &str, count: f64) {
        self.registry.worker_active_count
            .with_label_values(&[queue])
            .set(count);
    }
    
    pub fn observe_worker_processing_duration(&self, worker_id: &str, job_type: &str, duration_seconds: f64) {
        self.registry.worker_processing_duration
            .with_label_values(&[worker_id, job_type])
            .observe(duration_seconds);
    }
    
    // Metrik System
    pub fn set_cpu_usage(&self, usage_percent: f64) {
        self.registry.system_cpu_usage.set(usage_percent);
    }
    
    pub fn set_memory_usage(&self, usage_bytes: f64) {
        self.registry.system_memory_usage.set(usage_bytes);
    }
    
    pub fn observe_request_duration(&self, method: &str, endpoint: &str, status_code: &str, duration_seconds: f64) {
        self.registry.system_request_duration
            .with_label_values(&[method, endpoint, status_code])
            .observe(duration_seconds);
    }
    
    pub async fn add_custom_metric(&self, metric: Metric) -> Result<(), String> {
        self.registry.add_custom_metric(metric).await
    }
    
    pub fn encode_metrics(&self) -> Result<String, Box<dyn std::error::Error>> {
        self.registry.encode_metrics()
    }
    
    pub async fn get_system_metrics(&self) -> SystemMetrics {
        SystemMetrics {
            cpu_usage: self.registry.system_cpu_usage.get(),
            memory_usage: self.registry.system_memory_usage.get(),
            timestamp: chrono::Utc::now(),
        }
    
    pub async fn get_queue_metrics(&self, queue: &str) -> QueueMetrics {
        QueueMetrics {
            length: self.registry.queue_length.with_label_values(&[queue]).get(),
            throughput: self.registry.queue_throughput.with_label_values(&[queue]).get(),
            timestamp: chrono::Utc::now(),
        }
    }
    
    pub async fn get_job_metrics(&self, queue: &str, job_type: &str) -> JobMetrics {
        let processed = self.registry.job_processed_total
            .with_label_values(&[queue, job_type, "success"])
            .get();
        
        let failed = self.registry.job_failed_total
            .with_label_values(&[queue, job_type, "error"])
            .get();
        
        let submitted = self.registry.job_submitted_total
            .with_label_values(&[queue, job_type])
            .get();
        
        JobMetrics {
            submitted: submitted as u64,
            processed: processed as u64,
            failed: failed as u64,
            success_rate: if submitted > 0 { (processed / submitted) * 10.0 } else { 100.0 },
            timestamp: chrono::Utc::now(),
        }
    }
}

pub struct SystemMetrics {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct QueueMetrics {
    pub length: f64,
    pub throughput: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct JobMetrics {
    pub submitted: u64,
    pub processed: u64,
    pub failed: u64,
    pub success_rate: f64, // dalam persen
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
```

### 4.2. Middleware Metrics untuk API

```rust
// File: src/middleware/metrics_middleware.rs
use axum::{
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use crate::services::metrics_service::MetricsService;

pub async fn metrics_middleware(
    State(metrics_service): State<Arc<MetricsService>>,
    method: Method,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let start_time = std::time::Instant::now();
    let path = request.uri().path().to_string();
    let method_str = method.as_str().to_string();
    
    let response = next.run(request).await;
    let duration = start_time.elapsed().as_secs_f64();
    
    let status_code = response.status().as_str().to_string();
    
    // Catat durasi request
    metrics_service
        .observe_request_duration(&method_str, &path, &status_code, duration)
        .await;
    
    Ok(response)
}
```

## 5. Ekspor Metrik ke Prometheus

### 5.1. Endpoint Metrics

```rust
// File: src/handlers/metrics_handler.rs
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use crate::services::metrics_service::MetricsService;

pub async fn metrics_handler(
    State(metrics_service): State<Arc<MetricsService>>,
) -> Result<impl IntoResponse, StatusCode> {
    match metrics_service.encode_metrics().await {
        Ok(metrics_text) => {
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/plain; version=0.0.4")
                .body(metrics_text)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
```

### 5.2. Background Metrics Collection

```rust
// File: src/services/background_metrics_collector.rs
use crate::services::metrics_service::MetricsService;
use std::sync::Arc;
use sysinfo::{System, SystemExt, CpuExt, NetworkExt, DiskExt};
use tokio::time::{sleep, Duration};

pub struct BackgroundMetricsCollector {
    metrics_service: Arc<MetricsService>,
    system: System,
}

impl BackgroundMetricsCollector {
    pub fn new(metrics_service: Arc<MetricsService>) -> Self {
        Self {
            metrics_service,
            system: System::new_all(),
        }
    }
    
    pub async fn start_collection(&self) {
        loop {
            self.collect_system_metrics().await;
            self.collect_application_metrics().await;
            
            // Tidur selama 10 detik sebelum pengumpulan berikutnya
            sleep(Duration::from_secs(10)).await;
        }
    }
    
    async fn collect_system_metrics(&mut self) {
        self.system.refresh_all();
        
        // Kumpulkan penggunaan CPU
        let cpu_usage: f64 = self.system.cpus().iter()
            .map(|cpu| cpu.cpu_usage() as f64)
            .sum::<f64>() / self.system.cpus().len() as f64;
        
        self.metrics_service.set_cpu_usage(cpu_usage).await;
        
        // Kumpulkan penggunaan memori
        let memory_usage = self.system.used_memory() as f64;
        self.metrics_service.set_memory_usage(memory_usage).await;
        
        // Kumpulkan penggunaan disk
        for disk in self.system.disks() {
            let disk_usage = (disk.total_space() - disk.available_space()) as f64;
            // Dalam implementasi nyata, kita akan memiliki metrik khusus untuk disk
        }
        
        // Kumpulkan penggunaan jaringan
        for (_, network) in self.system.networks() {
            // Dalam implementasi nyata, kita akan melacak penggunaan jaringan
        }
    }
    
    async fn collect_application_metrics(&self) {
        // Dalam implementasi nyata, ini akan mengumpulkan metrik aplikasi spesifik
        // seperti jumlah koneksi database aktif, jumlah request pending, dll.
    }
}
```

## 6. Integrasi dengan Layanan Lain

### 6.1. Auto-scaling Metrics

```rust
// File: src/services/autoscaling_metrics_service.rs
use crate::services::metrics_service::MetricsService;
use std::sync::Arc;

pub struct AutoscalingMetricsService {
    metrics_service: Arc<MetricsService>,
}

impl AutoscalingMetricsService {
    pub fn new(metrics_service: Arc<MetricsService>) -> Self {
        Self { metrics_service }
    }
    
    pub async fn get_queue_backlog_metrics(&self, queue: &str) -> QueueBacklogMetrics {
        let queue_metrics = self.metrics_service.get_queue_metrics(queue).await;
        
        QueueBacklogMetrics {
            queue: queue.to_string(),
            current_length: queue_metrics.length,
            throughput: queue_metrics.throughput,
            // Hitung backlog berdasarkan perbedaan antara job yang masuk dan yang diproses
            estimated_backlog_time: if queue_metrics.throughput > 0.0 {
                queue_metrics.length / queue_metrics.throughput
            } else {
                f64::INFINITY
            },
            timestamp: chrono::Utc::now(),
        }
    }
    
    pub async fn get_worker_efficiency_metrics(&self, queue: &str) -> WorkerEfficiencyMetrics {
        let job_metrics = self.metrics_service.get_job_metrics(queue, "all").await;
        
        WorkerEfficiencyMetrics {
            queue: queue.to_string(),
            success_rate: job_metrics.success_rate,
            average_processing_time: self.get_average_processing_time(queue).await,
            worker_utilization: self.get_worker_utilization(queue).await,
            timestamp: chrono::Utc::now(),
        }
    }
    
    async fn get_average_processing_time(&self, queue: &str) -> f64 {
        // Dalam implementasi nyata, ini akan menghitung rata-rata waktu pemrosesan
        // dari histogram job_processing_duration
        0.0
    }
    
    async fn get_worker_utilization(&self, queue: &str) -> f64 {
        // Dalam implementasi nyata, ini akan menghitung utilisasi worker
        0.0
    }
}

pub struct QueueBacklogMetrics {
    pub queue: String,
    pub current_length: f64,
    pub throughput: f64,
    pub estimated_backlog_time: f64, // dalam detik
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub struct WorkerEfficiencyMetrics {
    pub queue: String,
    pub success_rate: f64, // dalam persen
    pub average_processing_time: f64, // dalam detik
    pub worker_utilization: f64, // dalam persen
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
```

### 6.2. Business Metrics

```rust
// File: src/services/business_metrics_service.rs
use crate::services::metrics_service::MetricsService;
use std::sync::Arc;

pub struct BusinessMetricsService {
    metrics_service: Arc<MetricsService>,
}

impl BusinessMetricsService {
    pub fn new(metrics_service: Arc<MetricsService>) -> Self {
        Self { metrics_service }
    }
    
    pub async fn track_job_completion(
        &self,
        queue: &str,
        job_type: &str,
        duration_seconds: f64,
        success: bool,
    ) {
        if success {
            self.metrics_service.increment_job_processed(queue, job_type, "success").await;
        } else {
            self.metrics_service.increment_job_failed(queue, job_type, "error").await;
        }
        
        self.metrics_service.observe_job_processing_duration(queue, job_type, duration_seconds).await;
    }
    
    pub async fn track_billing_metrics(
        &self,
        organization_id: &str,
        job_type: &str,
        resource_usage: f64, // dalam unit yang bisa ditagih
    ) {
        // Dalam implementasi nyata, ini akan melacak metrik untuk keperluan penagihan
        // Misalnya: jumlah job yang diproses, durasi pemrosesan, sumber daya yang digunakan
        let _ = self.metrics_service.add_custom_metric(crate::models::metrics::Metric::new_gauge(
            &format!("taskforge_billing_usage_total"),
            serde_json::json!({"organization_id": organization_id, "job_type": job_type}),
            resource_usage,
        )).await;
    }
    
    pub async fn get_organization_metrics(
        &self,
        organization_id: &str,
    ) -> OrganizationMetrics {
        // Dalam implementasi nyata, ini akan mengumpulkan metrik organisasi
        // dari berbagai sumber dan mengembalikannya dalam format yang bisa digunakan
        OrganizationMetrics {
            organization_id: organization_id.to_string(),
            total_jobs_processed: self.get_total_jobs_for_org(organization_id).await,
            total_job_errors: self.get_error_jobs_for_org(organization_id).await,
            average_job_duration: self.get_avg_duration_for_org(organization_id).await,
            resource_usage: self.get_resource_usage_for_org(organization_id).await,
            timestamp: chrono::Utc::now(),
        }
    }
    
    async fn get_total_jobs_for_org(&self, organization_id: &str) -> u64 {
        // Implementasi untuk mendapatkan total job untuk organisasi
        0
    }
    
    async fn get_error_jobs_for_org(&self, organization_id: &str) -> u64 {
        // Implementasi untuk mendapatkan jumlah job error untuk organisasi
        0
    }
    
    async fn get_avg_duration_for_org(&self, organization_id: &str) -> f64 {
        // Implementasi untuk mendapatkan durasi rata-rata job untuk organisasi
        0.0
    }
    
    async fn get_resource_usage_for_org(&self, organization_id: &str) -> f64 {
        // Implementasi untuk mendapatkan penggunaan sumber daya untuk organisasi
        0.0
    }
}

pub struct OrganizationMetrics {
    pub organization_id: String,
    pub total_jobs_processed: u64,
    pub total_job_errors: u64,
    pub average_job_duration: f64, // dalam detik
    pub resource_usage: f64, // dalam unit yang bisa ditagih
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
```

## 7. Dashboard dan Visualisasi

### 7.1. Konfigurasi Grafana

```yaml
# grafana_dashboard.yml
apiVersion: 1

providers:
  - name: 'TaskForge Dashboard'
    orgId: 1
    folder: ''
    type: file
    disableDeletion: false
    updateIntervalSeconds: 10
    options:
      path: /etc/grafana/dashboards

# Contoh dashboard JSON untuk Grafana
{
  "dashboard": {
    "id": null,
    "title": "TaskForge Job Processing",
    "tags": ["taskforge", "jobs", "queue"],
    "timezone": "browser",
    "panels": [
      {
        "id": 1,
        "title": "Job Throughput",
        "type": "graph",
        "targets": [
          {
            "expr": "rate(taskforge_job_processed_total[5m])",
            "legendFormat": "{{queue}} - {{result}}"
          }
        ],
        "yAxes": [
          {
            "label": "Jobs/Second",
            "show": true
          }
        ]
      },
      {
        "id": 2,
        "title": "Queue Length",
        "type": "graph",
        "targets": [
          {
            "expr": "taskforge_queue_length",
            "legendFormat": "{{queue}}"
          }
        ],
        "yAxes": [
          {
            "label": "Jobs",
            "show": true
          }
        ]
      },
      {
        "id": 3,
        "title": "Job Processing Duration",
        "type": "graph",
        "targets": [
          {
            "expr": "histogram_quantile(0.95, rate(taskforge_job_processing_duration_seconds_bucket[5m]))",
            "legendFormat": "{{queue}} - P95"
          }
        ],
        "yAxes": [
          {
            "label": "Seconds",
            "show": true
          }
        ]
      }
    ],
    "time": {
      "from": "now-6h",
      "to": "now"
    },
    "refresh": "5s"
  }
}
```

### 7.2. Alert Rules untuk Prometheus

```yaml
# prometheus_alerts.yml
groups:
  - name: taskforge_alerts
    rules:
      - alert: QueueBacklogHigh
        expr: taskforge_queue_length > 1000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Queue backlog is high"
          description: "Queue {{ $labels.queue }} has {{ $value }} jobs waiting"
      
      - alert: JobFailureRateHigh
        expr: rate(taskforge_job_failed_total[5m]) > 10
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "High job failure rate"
          description: "Job failure rate is {{ $value }} per second"
      
      - alert: WorkerDown
        expr: taskforge_worker_active_count < 1
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "No active workers"
          description: "No active workers for queue {{ $labels.queue }}"
      
      - alert: SystemCPUHigh
        expr: taskforge_system_cpu_usage_percent > 80
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High CPU usage"
          description: "CPU usage is {{ $value }}%"
      
      - alert: SystemMemoryHigh
        expr: taskforge_system_memory_usage_bytes / 1024 / 1024 > 8192
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High memory usage"
          description: "Memory usage is {{ $value }} MB"
```

## 8. Best Practices dan Rekomendasi

### 8.1. Praktik Terbaik untuk Metrics

1. **Gunakan nama metrik yang konsisten** - mengikuti konvensi Prometheus
2. **Gunakan label yang bermakna** - untuk segmentasi data yang fleksibel
3. **Hindari terlalu banyak label kartesian** - untuk mencegah ledakan seri waktu
4. **Gunakan tipe metrik yang tepat** - counter untuk hal yang hanya bertambah, gauge untuk nilai yang bisa naik turun
5. **Gunakan unit yang konsisten** - detik untuk durasi, bytes untuk ukuran, dll.

### 8.2. Praktik Terbaik untuk Monitoring

1. **Gunakan histogram untuk durasi** - untuk analisis latency yang fleksibel
2. **Gunakan quantiles yang bermakna** - seperti P95, P99 untuk latency
3. **Gunakan alert yang bijak** - hindari alert fatigue dengan threshold yang masuk akal
4. **Gunakan dashboard yang informatif** - fokus pada metrik bisnis dan kesehatan sistem
5. **Gunakan retention policy yang sesuai** - untuk efisiensi penyimpanan

### 8.3. Skala dan Kinerja

1. **Gunakan async collection** - untuk tidak menghambat thread utama
2. **Gunakan buffering untuk metrics** - untuk mengurangi frekuensi write ke storage
3. **Gunakan sampling untuk metrics tingkat tinggi** - untuk efisiensi
4. **Gunakan service discovery** - untuk pengawasan dinamis di lingkungan container
5. **Gunakan federation untuk skenario multi-instance** - untuk agregasi metrics dari berbagai sumber