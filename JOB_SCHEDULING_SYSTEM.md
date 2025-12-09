# Sistem Penjadwalan Job untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan sistem penjadwalan job dalam aplikasi TaskForge. Sistem ini memungkinkan pengguna untuk menjadwalkan job untuk dieksekusi di masa depan, baik sekali maupun berulang (cron-like), dengan fleksibilitas dan skalabilitas tinggi.

## 2. Arsitektur Sistem Penjadwalan

### 2.1. Komponen Utama

Sistem penjadwalan terdiri dari beberapa komponen utama:

1. **Job Scheduler**: Komponen yang bertanggung jawab untuk mengelola dan mengeksekusi job yang dijadwalkan
2. **Cron Parser**: Parser untuk ekspresi cron-style
3. **Time-based Trigger**: Mekanisme untuk memicu eksekusi job berdasarkan waktu
4. **Persistence Layer**: Penyimpanan untuk job yang dijadwalkan
5. **Scheduler Engine**: Mesin inti yang menentukan kapan job harus dieksekusi

### 2.2. Model Penjadwalan Job

```rust
// File: src/models/scheduled_job.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "schedule_type", rename_all = "lowercase")]
pub enum ScheduleType {
    Once,      // Sekali eksekusi
    Recurring, // Berulang
    Cron,      // Berdasarkan ekspresi cron
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "schedule_status", rename_all = "lowercase")]
pub enum ScheduleStatus {
    Active,
    Paused,
    Completed, // Hanya untuk job sekali
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ScheduledJob {
    pub id: Uuid,
    pub job_id: Uuid,              // ID job yang dijadwalkan
    pub queue_id: Uuid,            // Queue tujuan
    pub name: String,              // Nama deskriptif jadwal
    pub schedule_type: ScheduleType,
    pub schedule_expression: String, // Ekspresi cron atau durasi
    pub payload: Value,            // Payload job
    pub job_type: String,          // Tipe job
    pub priority: i32,             // Prioritas job
    pub max_attempts: i32,         // Jumlah maksimum percobaan
    pub next_run_at: DateTime<Utc>, // Waktu eksekusi berikutnya
    pub last_run_at: Option<DateTime<Utc>>, // Waktu eksekusi terakhir
    pub status: ScheduleStatus,
    pub created_by: Uuid,          // ID pengguna yang membuat
    pub organization_id: Uuid,     // ID organisasi (untuk multi-tenant)
    pub metadata: Value,           // Metadata tambahan
    pub max_runs: Option<i32>,     // Jumlah maksimum eksekusi (None = tak terbatas)
    pub runs_count: i32,           // Jumlah eksekusi yang telah dilakukan
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl ScheduledJob {
    pub fn new(
        job_id: Uuid,
        queue_id: Uuid,
        name: String,
        schedule_type: ScheduleType,
        schedule_expression: String,
        payload: Value,
        job_type: String,
        created_by: Uuid,
        organization_id: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            job_id,
            queue_id,
            name,
            schedule_type,
            schedule_expression,
            payload,
            job_type,
            priority: 0,
            max_attempts: 3,
            next_run_at: calculate_next_run(&schedule_type, &schedule_expression, now),
            last_run_at: None,
            status: ScheduleStatus::Active,
            created_by,
            organization_id,
            metadata: serde_json::json!({}),
            max_runs: None,
            runs_count: 0,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("Schedule name must be between 1 and 100 characters".to_string());
        }
        
        if self.job_type.is_empty() || self.job_type.len() > 100 {
            return Err("Job type must be between 1 and 100 characters".to_string());
        }
        
        match self.schedule_type {
            ScheduleType::Cron => {
                // Validasi ekspresi cron
                validate_cron_expression(&self.schedule_expression)
                    .map_err(|e| format!("Invalid cron expression: {}", e))?;
            },
            ScheduleType::Once | ScheduleType::Recurring => {
                // Validasi format durasi atau timestamp
                validate_time_expression(&self.schedule_expression)
                    .map_err(|e| format!("Invalid time expression: {}", e))?;
            }
        }
        
        if self.priority < 0 || self.priority > 9 {
            return Err("Priority must be between 0 and 9".to_string());
        }
        
        Ok(())
    }
    
    pub fn is_due(&self) -> bool {
        self.next_run_at <= Utc::now() && matches!(self.status, ScheduleStatus::Active)
    }
    
    pub fn can_run_again(&self) -> bool {
        if let Some(max_runs) = self.max_runs {
            self.runs_count < max_runs
        } else {
            true // Tak terbatas
        }
    }
    
    pub fn update_next_run(&mut self) {
        if !self.can_run_again() {
            self.status = ScheduleStatus::Completed;
            return;
        }
        
        self.next_run_at = calculate_next_run(
            &self.schedule_type,
            &self.schedule_expression,
            self.next_run_at,
        );
    }
    
    pub fn mark_as_run(&mut self) {
        self.last_run_at = Some(Utc::now());
        self.runs_count += 1;
        self.updated_at = Utc::now();
        
        // Update next run time
        self.update_next_run();
        
        // Jika ini job sekali dan sudah dijalankan, tandai sebagai selesai
        if matches!(self.schedule_type, ScheduleType::Once) && self.runs_count >= 1 {
            self.status = ScheduleStatus::Completed;
        }
    }
}

// Fungsi bantu untuk validasi
fn validate_cron_expression(expr: &str) -> Result<(), String> {
    // Dalam implementasi nyata, gunakan library seperti `cron` untuk validasi
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 && parts.len() != 6 {
        return Err("Cron expression must have 5 or 6 parts".to_string());
    }
    
    Ok(())
}

fn validate_time_expression(expr: &str) -> Result<(), String> {
    // Validasi format timestamp ISO8601 atau durasi
    if let Ok(_datetime) = chrono::DateTime::parse_from_rfc3339(expr) {
        return Ok(());
    }
    
    // Validasi format durasi (misalnya: "1h", "30m", "2d")
    if !expr.chars().all(|c| c.is_alphanumeric() || c == 'h' || c == 'm' || c == 's' || c == 'd') {
        return Err("Invalid time expression format".to_string());
    }
    
    Ok(())
}

fn calculate_next_run(
    schedule_type: &ScheduleType,
    expression: &str,
    from_time: DateTime<Utc>,
) -> DateTime<Utc> {
    match schedule_type {
        ScheduleType::Once => {
            // Parse timestamp untuk eksekusi sekali
            chrono::DateTime::parse_from_rfc3339(expression)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| from_time + chrono::Duration::seconds(1))
        },
        ScheduleType::Recurring => {
            // Parse durasi untuk eksekusi berulang
            parse_duration(expression)
                .map(|duration| from_time + duration)
                .unwrap_or_else(|_| from_time + chrono::Duration::minutes(1))
        },
        ScheduleType::Cron => {
            // Dalam implementasi nyata, gunakan library cron untuk menghitung waktu berikutnya
            from_time + chrono::Duration::minutes(1) // Placeholder
        }
    }
}

fn parse_duration(duration_str: &str) -> Result<chrono::Duration, String> {
    // Parse durasi seperti "1h", "30m", "2d", "10s"
    let (num, unit) = duration_str.split_at(duration_str.len() - 1);
    let num: i64 = num.parse().map_err(|_| "Invalid duration number".to_string())?;
    
    let duration = match unit {
        "s" => chrono::Duration::seconds(num),
        "m" => chrono::Duration::minutes(num),
        "h" => chrono::Duration::hours(num),
        "d" => chrono::Duration::days(num),
        _ => return Err("Invalid duration unit. Use s, m, h, or d".to_string()),
    };
    
    Ok(duration)
}
```

## 3. Cron Expression Parser

### 3.1. Implementasi Parser Cron

```rust
// File: src/services/cron_parser.rs
use chrono::{DateTime, Utc, Datelike, Timelike};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct CronExpression {
    pub minute: CronField,
    pub hour: CronField,
    pub day_of_month: CronField,
    pub month: CronField,
    pub day_of_week: CronField,
}

#[derive(Debug, Clone)]
pub enum CronField {
    All,                    // *
    Value(i32),            // 5
    Range(i32, i32),       // 5-10
    Step(i32, Option<i32>), // */2 atau 5/10
    List(Vec<i32>),        // 1,5,10
}

impl CronExpression {
    pub fn parse(expression: &str) -> Result<Self, String> {
        let parts: Vec<&str> = expression.split_whitespace().collect();
        if parts.len() < 5 {
            return Err("Cron expression must have at least 5 parts".to_string());
        }
        
        let minute = Self::parse_field(parts[0], 0, 59)?;
        let hour = Self::parse_field(parts[1], 0, 23)?;
        let day_of_month = Self::parse_field(parts[2], 1, 31)?;
        let month = Self::parse_field(parts[3], 1, 12)?;
        let day_of_week = Self::parse_field(parts[4], 0, 6)?; // 0 = Minggu, 6 = Sabtu
        
        Ok(CronExpression {
            minute,
            hour,
            day_of_month,
            month,
            day_of_week,
        })
    }
    
    fn parse_field(field: &str, min: i32, max: i32) -> Result<CronField, String> {
        if field == "*" {
            return Ok(CronField::All);
        }
        
        if field.contains('/') {
            let parts: Vec<&str> = field.split('/').collect();
            if parts[0] == "*" {
                let step = parts[1].parse::<i32>().map_err(|_| "Invalid step value")?;
                return Ok(CronField::Step(step, None));
            } else {
                let range_part = parts[0];
                let step = parts[1].parse::<i32>().map_err(|_| "Invalid step value")?;
                
                if range_part.contains('-') {
                    let range_parts: Vec<&str> = range_part.split('-').collect();
                    let start = range_parts[0].parse::<i32>().map_err(|_| "Invalid range start")?;
                    let end = range_parts[1].parse::<i32>().map_err(|_| "Invalid range end")?;
                    return Ok(CronField::Step(step, Some(start * 1000 + end))); // Encoding range as start*1000+end
                }
            }
        }
        
        if field.contains('-') {
            let parts: Vec<&str> = field.split('-').collect();
            let start = parts[0].parse::<i32>().map_err(|_| "Invalid range start")?;
            let end = parts[1].parse::<i32>().map_err(|_| "Invalid range end")?;
            
            if start < min || end > max || start > end {
                return Err(format!("Range {}-{} is out of bounds ({}-{})", start, end, min, max));
            }
            
            return Ok(CronField::Range(start, end));
        }
        
        if field.contains(',') {
            let values: Result<Vec<i32>, _> = field.split(',')
                .map(|v| v.parse::<i32>())
                .collect();
            
            let values = values.map_err(|_| "Invalid list value")?;
            
            for &val in &values {
                if val < min || val > max {
                    return Err(format!("Value {} is out of bounds ({}-{})", val, min, max));
                }
            }
            
            return Ok(CronField::List(values));
        }
        
        let value = field.parse::<i32>().map_err(|_| "Invalid value")?;
        if value < min || value > max {
            return Err(format!("Value {} is out of bounds ({}-{})", value, min, max));
        }
        
        Ok(CronField::Value(value))
    }
    
    pub fn next_occurrence(&self, from: DateTime<Utc>) -> DateTime<Utc> {
        let mut next_time = from.with_nanosecond(0).unwrap();
        
        // Tambahkan 1 detik untuk memastikan kita tidak menghitung waktu saat ini
        next_time = next_time.with_second(0).unwrap() + chrono::Duration::minutes(1);
        
        loop {
            // Periksa bulan
            if !self.month.matches(next_time.month() as i32) {
                next_time = next_time.with_day0(0).unwrap()
                    .with_hour(0).unwrap()
                    .with_minute(0).unwrap()
                    .with_second(0).unwrap()
                    + chrono::Duration::months(1);
                continue;
            }
            
            // Periksa tanggal
            if !self.day_of_month.matches(next_time.day() as i32) {
                next_time = next_time.with_hour(0).unwrap()
                    .with_minute(0).unwrap()
                    .with_second(0).unwrap()
                    + chrono::Duration::days(1);
                continue;
            }
            
            // Periksa hari dalam seminggu
            let day_of_week = next_time.weekday().number_from_sunday() as i32;
            if !self.day_of_week.matches(day_of_week) {
                next_time = next_time.with_hour(0).unwrap()
                    .with_minute(0).unwrap()
                    .with_second(0).unwrap()
                    + chrono::Duration::days(1);
                continue;
            }
            
            // Periksa jam
            if !self.hour.matches(next_time.hour() as i32) {
                next_time = next_time.with_minute(0).unwrap()
                    .with_second(0).unwrap()
                    + chrono::Duration::hours(1);
                continue;
            }
            
            // Periksa menit
            if !self.minute.matches(next_time.minute() as i32) {
                next_time = next_time.with_second(0).unwrap()
                    + chrono::Duration::minutes(1);
                continue;
            }
            
            // Semua kondisi terpenuhi
            break;
        }
        
        next_time
    }
}

impl CronField {
    fn matches(&self, value: i32) -> bool {
        match self {
            CronField::All => true,
            CronField::Value(v) => *v == value,
            CronField::Range(start, end) => *start <= value && value <= *end,
            CronField::Step(step, range) => {
                match range {
                    Some(range_val) => {
                        let start = range_val / 100;
                        let end = range_val % 1000;
                        if value >= start && value <= end {
                            (value - start) % step == 0
                        } else {
                            false
                        }
                    },
                    None => value % step == 0,
                }
            },
            CronField::List(values) => values.contains(&value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_cron_expression() {
        let expr = CronExpression::parse("0 2 * * 1-5").unwrap();
        assert!(matches!(expr.hour, CronField::Value(2)));
        assert!(matches!(expr.minute, CronField::Value(0)));
        assert!(matches!(expr.day_of_week, CronField::Range(1, 5)));
    }
}
```

## 4. Scheduler Engine

### 4.1. Layanan Scheduler Utama

```rust
// File: src/services/scheduler_service.rs
use crate::{
    models::scheduled_job::{ScheduledJob, ScheduleStatus},
    services::job_service::JobService,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::time::{interval, Duration};

pub struct SchedulerService {
    job_service: Arc<JobService>,
    db: Database, // Gantilah dengan tipe database Anda
}

impl SchedulerService {
    pub fn new(job_service: Arc<JobService>, db: Database) -> Self {
        Self { job_service, db }
    }
    
    pub async fn start_scheduler(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut interval = interval(Duration::from_secs(30)); // Periksa setiap 30 detik
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.process_scheduled_jobs().await {
                eprintln!("Error processing scheduled jobs: {}", e);
            }
        }
    }
    
    pub async fn process_scheduled_jobs(&self) -> Result<(), SqlxError> {
        let now = Utc::now();
        
        // Ambil semua job yang jatuh tempo
        let scheduled_jobs = self.db.get_due_scheduled_jobs(now).await?;
        
        for mut scheduled_job in scheduled_jobs {
            if scheduled_job.is_due() && scheduled_job.can_run_again() {
                // Buat job baru untuk dieksekusi
                let new_job = crate::models::Job::new(
                    scheduled_job.queue_id,
                    scheduled_job.job_type.clone(),
                    scheduled_job.payload.clone(),
                    scheduled_job.priority,
                    Some(scheduled_job.max_attempts),
                );
                
                match self.job_service.create_job(new_job).await {
                    Ok(created_job) => {
                        // Update status scheduled job
                        scheduled_job.mark_as_run();
                        scheduled_job.job_id = created_job.id; // Update dengan ID job baru
                        
                        self.db.update_scheduled_job(scheduled_job).await?;
                        
                        // Log keberhasilan
                        println!("Successfully scheduled job {} at {}", created_job.id, now);
                    },
                    Err(e) => {
                        eprintln!("Failed to create job from scheduled job {}: {}", scheduled_job.id, e);
                        
                        // Dalam implementasi nyata, mungkin ingin mencatat error ini
                        // dan mungkin menonaktifkan scheduled job jika terjadi error berulang
                    }
                }
            }
        }
        
        Ok(())
    }
    
    pub async fn create_scheduled_job(
        &self,
        queue_id: Uuid,
        name: String,
        schedule_type: crate::models::scheduled_job::ScheduleType,
        schedule_expression: String,
        payload: serde_json::Value,
        job_type: String,
        created_by: Uuid,
        organization_id: Uuid,
    ) -> Result<ScheduledJob, SqlxError> {
        let scheduled_job = ScheduledJob::new(
            Uuid::new_v4(), // Job ID akan diisi saat eksekusi pertama
            queue_id,
            name,
            schedule_type,
            schedule_expression,
            payload,
            job_type,
            created_by,
            organization_id,
        );
        
        scheduled_job.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        self.db.create_scheduled_job(scheduled_job).await
    }
    
    pub async fn get_scheduled_job_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<ScheduledJob>, SqlxError> {
        self.db.get_scheduled_job_by_id(id).await
    }
    
    pub async fn get_scheduled_jobs_by_organization(
        &self,
        organization_id: Uuid,
    ) -> Result<Vec<ScheduledJob>, SqlxError> {
        self.db.get_scheduled_jobs_by_organization(organization_id).await
    }
    
    pub async fn update_scheduled_job_status(
        &self,
        id: Uuid,
        status: ScheduleStatus,
    ) -> Result<(), SqlxError> {
        self.db.update_scheduled_job_status(id, status).await
    }
    
    pub async fn pause_scheduled_job(&self, id: Uuid) -> Result<(), SqlxError> {
        self.update_scheduled_job_status(id, ScheduleStatus::Paused).await
    }
    
    pub async fn resume_scheduled_job(&self, id: Uuid) -> Result<(), SqlxError> {
        self.update_scheduled_job_status(id, ScheduleStatus::Active).await
    }
    
    pub async fn delete_scheduled_job(&self, id: Uuid) -> Result<(), SqlxError> {
        self.db.delete_scheduled_job(id).await
    }
    
    pub async fn get_scheduled_jobs_summary(
        &self,
        organization_id: Uuid,
    ) -> Result<ScheduledJobsSummary, SqlxError> {
        let all_jobs = self.get_scheduled_jobs_by_organization(organization_id).await?;
        
        let mut summary = ScheduledJobsSummary {
            total: 0,
            active: 0,
            paused: 0,
            completed: 0,
            failed: 0,
            due_now: 0,
        };
        
        for job in all_jobs {
            summary.total += 1;
            
            match job.status {
                ScheduleStatus::Active => summary.active += 1,
                ScheduleStatus::Paused => summary.paused += 1,
                ScheduleStatus::Completed => summary.completed += 1,
                ScheduleStatus::Failed => summary.failed += 1,
            }
            
            if job.is_due() {
                summary.due_now += 1;
            }
        }
        
        Ok(summary)
    }
}

pub struct ScheduledJobsSummary {
    pub total: u32,
    pub active: u32,
    pub paused: u32,
    pub completed: u32,
    pub failed: u32,
    pub due_now: u32,
}
```

### 4.2. Cron Job Manager

```rust
// File: src/services/cron_job_manager.rs
use crate::{
    models::scheduled_job::{ScheduledJob, ScheduleType},
    services::{cron_parser::CronExpression, job_service::JobService},
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc};
use std::sync::Arc;

pub struct CronJobManager {
    job_service: Arc<JobService>,
    db: Database, // Gantilah dengan tipe database Anda
}

impl CronJobManager {
    pub fn new(job_service: Arc<JobService>, db: Database) -> Self {
        Self { job_service, db }
    }
    
    pub async fn process_cron_jobs(&self, now: DateTime<Utc>) -> Result<(), SqlxError> {
        // Ambil semua cron job aktif
        let cron_jobs = self.db.get_active_cron_jobs().await?;
        
        for mut cron_job in cron_jobs {
            // Parse ekspresi cron
            match CronExpression::parse(&cron_job.schedule_expression) {
                Ok(cron_expr) => {
                    // Hitung waktu eksekusi berikutnya berdasarkan waktu terakhir dijalankan
                    let last_run = cron_job.last_run_at.unwrap_or(cron_job.created_at);
                    let next_run = cron_expr.next_occurrence(last_run);
                    
                    // Periksa apakah waktunya sudah tiba
                    if next_run <= now && now < next_run + chrono::Duration::minutes(1) {
                        // Waktu eksekusi tiba
                        if cron_job.can_run_again() {
                            // Buat job baru untuk dieksekusi
                            let new_job = crate::models::Job::new(
                                cron_job.queue_id,
                                cron_job.job_type.clone(),
                                cron_job.payload.clone(),
                                cron_job.priority,
                                Some(cron_job.max_attempts),
                            );
                            
                            match self.job_service.create_job(new_job).await {
                                Ok(created_job) => {
                                    // Update cron job
                                    cron_job.mark_as_run();
                                    cron_job.job_id = created_job.id;
                                    
                                    self.db.update_scheduled_job(cron_job).await?;
                                    
                                    println!("Cron job {} executed at {}", cron_job.id, now);
                                },
                                Err(e) => {
                                    eprintln!("Failed to create job from cron job {}: {}", cron_job.id, e);
                                }
                            }
                        } else {
                            // Job telah mencapai batas maksimum eksekusi
                            self.db.update_scheduled_job_status(
                                cron_job.id,
                                crate::models::scheduled_job::ScheduleStatus::Completed
                            ).await?;
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Invalid cron expression for job {}: {}", cron_job.id, e);
                    
                    // Nonaktifkan job dengan ekspresi cron yang tidak valid
                    self.db.update_scheduled_job_status(
                        cron_job.id,
                        crate::models::scheduled_job::ScheduleStatus::Failed
                    ).await?;
                }
            }
        }
        
        Ok(())
    }
    
    pub async fn create_cron_job(
        &self,
        queue_id: Uuid,
        name: String,
        cron_expression: String,
        payload: serde_json::Value,
        job_type: String,
        created_by: Uuid,
        organization_id: Uuid,
        max_runs: Option<i32>,
    ) -> Result<ScheduledJob, SqlxError> {
        // Validasi ekspresi cron
        CronExpression::parse(&cron_expression)
            .map_err(|e| SqlxError::Decode(e.into()))?;
        
        let mut scheduled_job = ScheduledJob::new(
            Uuid::new_v4(),
            queue_id,
            name,
            ScheduleType::Cron,
            cron_expression,
            payload,
            job_type,
            created_by,
            organization_id,
        );
        
        scheduled_job.max_runs = max_runs;
        
        // Hitung waktu eksekusi pertama
        if let Ok(cron_expr) = CronExpression::parse(&scheduled_job.schedule_expression) {
            scheduled_job.next_run_at = cron_expr.next_occurrence(Utc::now());
        }
        
        scheduled_job.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        self.db.create_scheduled_job(scheduled_job).await
    }
    
    pub async fn get_next_cron_execution(
        &self,
        cron_expression: &str,
    ) -> Result<DateTime<Utc>, String> {
        let cron_expr = CronExpression::parse(cron_expression)?;
        Ok(cron_expr.next_occurrence(Utc::now()))
    }
}
```

## 5. API Endpoints untuk Penjadwalan

### 5.1. Scheduler API Handler

```rust
// File: src/api/scheduler.rs
use axum::{
    extract::{Path, Query, State, Json},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put, delete},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    models::scheduled_job::{ScheduledJob, ScheduleType, ScheduleStatus},
    services::{
        authorization_service::AuthorizationService,
        scheduler_service::SchedulerService,
    },
};

#[derive(Deserialize)]
pub struct CreateScheduledJobRequest {
    pub queue_id: Uuid,
    pub name: String,
    pub schedule_type: String, // "once", "recurring", "cron"
    pub schedule_expression: String,
    pub payload: serde_json::Value,
    pub job_type: String,
    pub priority: Option<i32>,
    pub max_attempts: Option<i32>,
    pub max_runs: Option<i32>,
}

#[derive(Deserialize)]
pub struct UpdateScheduledJobRequest {
    pub name: Option<String>,
    pub schedule_expression: Option<String>,
    pub payload: Option<serde_json::Value>,
    pub priority: Option<i32>,
    pub max_attempts: Option<i32>,
    pub max_runs: Option<i32>,
    pub status: Option<String>, // "active", "paused"
}

#[derive(Serialize)]
pub struct CreateScheduledJobResponse {
    pub id: Uuid,
    pub message: String,
}

#[derive(Serialize)]
pub struct ScheduledJobsResponse {
    pub jobs: Vec<ScheduledJob>,
    pub total: u32,
    pub page: u32,
    pub pages: u32,
}

#[derive(Serialize)]
pub struct ScheduledJobsSummaryResponse {
    pub total: u32,
    pub active: u32,
    pub paused: u32,
    pub completed: u32,
    pub failed: u32,
    pub due_now: u32,
}

pub fn create_scheduler_router(
    scheduler_service: Arc<SchedulerService>,
    authz_service: Arc<AuthorizationService>,
) -> Router {
    Router::new()
        .route("/", get(get_scheduled_jobs).post(create_scheduled_job))
        .route("/summary", get(get_scheduled_jobs_summary))
        .route("/:job_id", get(get_scheduled_job).put(update_scheduled_job).delete(delete_scheduled_job))
        .route("/:job_id/pause", post(pause_scheduled_job))
        .route("/:job_id/resume", post(resume_scheduled_job))
        .route("/:job_id/next-run", get(get_next_run_time))
        .with_state((scheduler_service, authz_service))
}

pub async fn get_scheduled_jobs(
    State((scheduler_service, authz_service)): State<(Arc<SchedulerService>, Arc<AuthorizationService>)>,
    Query(_query): Query<GetJobsQuery>, // Implementasikan query parameter
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "scheduled_job:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match scheduler_service.get_scheduled_jobs_by_organization(user.organization_id).await {
        Ok(jobs) => {
            let response = ScheduledJobsResponse {
                jobs,
                total: 0, // Implementasikan perhitungan total
                page: 1,  // Implementasikan pagination
                pages: 1, // Implementasikan pagination
            };
            Ok(Json(response))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn create_scheduled_job(
    State((scheduler_service, authz_service)): State<(Arc<SchedulerService>, Arc<AuthorizationService>)>,
    Json(request): Json<CreateScheduledJobRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "scheduled_job:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Validasi schedule type
    let schedule_type = match request.schedule_type.as_str() {
        "once" => ScheduleType::Once,
        "recurring" => ScheduleType::Recurring,
        "cron" => ScheduleType::Cron,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    
    match scheduler_service.create_scheduled_job(
        request.queue_id,
        request.name,
        schedule_type,
        request.schedule_expression,
        request.payload,
        request.job_type,
        user.id,
        user.organization_id,
    ).await {
        Ok(scheduled_job) => {
            let response = CreateScheduledJobResponse {
                id: scheduled_job.id,
                message: "Scheduled job created successfully".to_string(),
            };
            Ok(Json(response))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_scheduled_job(
    State((scheduler_service, authz_service)): State<(Arc<SchedulerService>, Arc<AuthorizationService>)>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "scheduled_job:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match scheduler_service.get_scheduled_job_by_id(job_id).await {
        Ok(Some(job)) => {
            // Pastikan job milik organisasi user
            if job.organization_id != user.organization_id {
                return Err(StatusCode::FORBIDDEN);
            }
            Ok(Json(job))
        },
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn update_scheduled_job(
    State((scheduler_service, authz_service)): State<(Arc<SchedulerService>, Arc<AuthorizationService>)>,
    Path(job_id): Path<Uuid>,
    Json(request): Json<UpdateScheduledJobRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "scheduled_job:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Ambil job yang ada
    let mut job = scheduler_service.get_scheduled_job_by_id(job_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan job milik organisasi user
    if job.organization_id != user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Update field yang disediakan
    if let Some(name) = request.name {
        job.name = name;
    }
    if let Some(schedule_expression) = request.schedule_expression {
        job.schedule_expression = schedule_expression;
        // Jika ekspresi diubah, hitung ulang waktu eksekusi berikutnya
        job.next_run_at = calculate_next_run(&job.schedule_type, &job.schedule_expression, Utc::now());
    }
    if let Some(payload) = request.payload {
        job.payload = payload;
    }
    if let Some(priority) = request.priority {
        job.priority = priority;
    }
    if let Some(max_attempts) = request.max_attempts {
        job.max_attempts = max_attempts;
    }
    if let Some(max_runs) = request.max_runs {
        job.max_runs = Some(max_runs);
    }
    if let Some(status_str) = request.status {
        job.status = match status_str.as_str() {
            "active" => ScheduleStatus::Active,
            "paused" => ScheduleStatus::Paused,
            "completed" => ScheduleStatus::Completed,
            "failed" => ScheduleStatus::Failed,
            _ => return Err(StatusCode::BAD_REQUEST),
        };
    }
    
    // Validasi ulang job
    job.validate().map_err(|_| StatusCode::BAD_REQUEST)?;
    
    match scheduler_service.db.update_scheduled_job(job).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn delete_scheduled_job(
    State((scheduler_service, authz_service)): State<(Arc<SchedulerService>, Arc<AuthorizationService>)>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "scheduled_job:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let job = scheduler_service.get_scheduled_job_by_id(job_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan job milik organisasi user
    if job.organization_id != user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match scheduler_service.delete_scheduled_job(job_id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn pause_scheduled_job(
    State((scheduler_service, authz_service)): State<(Arc<SchedulerService>, Arc<AuthorizationService>)>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "scheduled_job:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let job = scheduler_service.get_scheduled_job_by_id(job_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan job milik organisasi user
    if job.organization_id != user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match scheduler_service.pause_scheduled_job(job_id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn resume_scheduled_job(
    State((scheduler_service, authz_service)): State<(Arc<SchedulerService>, Arc<AuthorizationService>)>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "scheduled_job:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let job = scheduler_service.get_scheduled_job_by_id(job_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan job milik organisasi user
    if job.organization_id != user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match scheduler_service.resume_scheduled_job(job_id).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_scheduled_jobs_summary(
    State((scheduler_service, authz_service)): State<(Arc<SchedulerService>, Arc<AuthorizationService>)>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "scheduled_job:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match scheduler_service.get_scheduled_jobs_summary(user.organization_id).await {
        Ok(summary) => {
            let response = ScheduledJobsSummaryResponse {
                total: summary.total,
                active: summary.active,
                paused: summary.paused,
                completed: summary.completed,
                failed: summary.failed,
                due_now: summary.due_now,
            };
            Ok(Json(response))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_next_run_time(
    State((scheduler_service, authz_service)): State<(Arc<SchedulerService>, Arc<AuthorizationService>)>,
    Path(job_id): Path<Uuid>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "scheduled_job:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let job = scheduler_service.get_scheduled_job_by_id(job_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan job milik organisasi user
    if job.organization_id != user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    Ok(Json(serde_json::json!({
        "next_run_at": job.next_run_at,
        "is_due": job.is_due(),
    })))
}

#[derive(Deserialize)]
struct GetJobsQuery {
    page: Option<u32>,
    limit: Option<u32>,
    status: Option<String>,
    schedule_type: Option<String>,
}
```

## 6. Fitur-fitur Lanjutan

### 6.1. Job Dependency dan Chaining

```rust
// File: src/models/job_dependency.rs
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobDependency {
    pub id: Uuid,
    pub parent_job_id: Uuid,    // Job yang harus selesai dulu
    pub child_job_id: Uuid,     // Job yang menunggu parent
    pub dependency_type: DependencyType,
    pub status: DependencyStatus,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencyType {
    Sequential,  // Child menunggu parent selesai
    Parallel,    // Child bisa mulai saat parent mulai
    Conditional, // Child mulai jika parent hasilnya tertentu
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencyStatus {
    Pending,    // Menunggu
    Resolved,   // Sudah terpenuhi
    Failed,     // Gagal dipenuhi
}

impl JobDependency {
    pub fn is_resolved(&self) -> bool {
        matches!(self.status, DependencyStatus::Resolved)
    }
    
    pub fn can_execute_child(&self, parent_status: &str) -> bool {
        match self.dependency_type {
            DependencyType::Sequential => {
                matches!(parent_status, "succeeded" | "failed") // Child bisa jalan setelah parent selesai
            },
            DependencyType::Parallel => {
                matches!(parent_status, "started") // Child bisa jalan saat parent mulai
            },
            DependencyType::Conditional => {
                matches!(parent_status, "succeeded") // Child hanya jalan jika parent sukses
            },
        }
    }
}
```

### 6.2. Rate Limiting untuk Scheduled Jobs

```rust
// File: src/services/scheduler_rate_limiter.rs
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc, Duration};

pub struct SchedulerRateLimiter {
    limits: Arc<RwLock<HashMap<String, Vec<ExecutionRecord>>>>,
    default_jobs_per_minute: u32,
}

struct ExecutionRecord {
    timestamp: DateTime<Utc>,
    job_type: String,
}

impl SchedulerRateLimiter {
    pub fn new(default_jobs_per_minute: u32) -> Self {
        Self {
            limits: Arc::new(RwLock::new(HashMap::new())),
            default_jobs_per_minute,
        }
    }
    
    pub async fn is_allowed(
        &self,
        organization_id: &str,
        job_type: &str,
        max_jobs: Option<u32>,
    ) -> bool {
        let now = Utc::now();
        let max_jobs = max_jobs.unwrap_or(self.default_jobs_per_minute);
        
        let mut limits = self.limits.write().await;
        let key = format!("{}:{}", organization_id, job_type);
        let records = limits.entry(key).or_insert_with(Vec::new);
        
        // Hapus record lama (lebih dari 1 menit)
        records.retain(|record| {
            now - record.timestamp < Duration::minutes(1)
        });
        
        // Periksa apakah jumlah eksekusi melebihi batas
        if records.len() >= max_jobs as usize {
            return false;
        }
        
        // Tambahkan record eksekusi baru
        records.push(ExecutionRecord {
            timestamp: now,
            job_type: job_type.to_string(),
        });
        
        true
    }
    
    pub async fn get_remaining_jobs(
        &self,
        organization_id: &str,
        job_type: &str,
        max_jobs: Option<u32>,
    ) -> u32 {
        let max_jobs = max_jobs.unwrap_or(self.default_jobs_per_minute);
        let limits = self.limits.read().await;
        let key = format!("{}:{}", organization_id, job_type);
        
        if let Some(records) = limits.get(&key) {
            max_jobs.saturating_sub(records.len() as u32)
        } else {
            max_jobs
        }
    }
}
```

## 7. Best Practices dan Rekomendasi

### 7.1. Praktik Terbaik untuk Penjadwalan Job

1. **Gunakan ekspresi cron yang valid** - untuk memastikan jadwal berjalan sesuai harapan
2. **Terapkan rate limiting** - untuk mencegah eksekusi berlebihan
3. **Gunakan backoff untuk retry** - untuk penjadwalan ulang yang gagal
4. **Gunakan monitoring dan alerting** - untuk mendeteksi masalah penjadwalan
5. **Gunakan audit trail** - untuk melacak perubahan jadwal

### 7.2. Praktik Terbaik untuk Kinerja

1. **Gunakan indexing yang tepat** - pada kolom-kolom yang digunakan untuk pencarian jadwal
2. **Gunakan batching untuk eksekusi** - untuk efisiensi saat memproses banyak job
3. **Gunakan caching untuk ekspresi cron** - untuk menghindari parsing berulang
4. **Gunakan partisi tabel** - untuk mengelola data jadwal historis
5. **Gunakan connection pooling** - untuk efisiensi koneksi database

### 7.3. Skalabilitas

1. **Gunakan horizontal scaling** - untuk menangani volume jadwal yang tinggi
2. **Gunakan distributed locking** - untuk mencegah eksekusi ganda
3. **Gunakan message queue** - untuk mendistribusikan beban penjadwalan
4. **Gunakan sharding** - untuk mendistribusikan jadwal ke beberapa node
5. **Gunakan load balancing** - untuk mendistribusikan permintaan penjadwalan