# Sistem Pembayaran dan Metering untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan sistem pembayaran dan metering dalam aplikasi TaskForge. Sistem ini bertanggung jawab untuk menghitung penggunaan layanan oleh organisasi, mengelola berbagai tier langganan, dan memproses pembayaran sesuai dengan kebijakan penggunaan.

## 2. Arsitektur Sistem Metering

### 2.1. Model Penggunaan dan Billing

```rust
// File: src/models/usage.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "usage_type", rename_all = "lowercase")]
pub enum UsageType {
    JobExecution,
    JobProcessingTime,
    QueueStorage,
    WorkerHours,
    ApiCall,
    DataTransfer,
    Storage,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UsageRecord {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub usage_type: UsageType,
    pub quantity: f64,              // Jumlah unit yang digunakan
    pub unit_price: f64,            // Harga per unit
    pub total_cost: f64,            // Jumlah biaya (quantity * unit_price)
    pub timestamp: DateTime<Utc>,
    pub metadata: Value,            // Informasi tambahan tentang penggunaan
    pub period_start: DateTime<Utc>, // Awal periode penagihan
    pub period_end: DateTime<Utc>,   // Akhir periode penagihan
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "billing_cycle", rename_all = "lowercase")]
pub enum BillingCycle {
    Monthly,
    Quarterly,
    Annually,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BillingRecord {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub invoice_number: String,
    pub amount: f64,
    pub currency: String,
    pub status: BillingStatus,
    pub billing_cycle: BillingCycle,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub issued_at: DateTime<Utc>,
    pub due_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    pub line_items: Value,          // Detail tagihan dalam format JSON
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "billing_status", rename_all = "lowercase")]
pub enum BillingStatus {
    Draft,
    Sent,
    Paid,
    Overdue,
    Cancelled,
    Refunded,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Subscription {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub tier: SubscriptionTier,
    pub limits: UsageLimits,
    pub price_per_month: f64,
    pub billing_cycle: BillingCycle,
    pub auto_renew: bool,
    pub current_period_start: DateTime<Utc>,
    pub current_period_end: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "subscription_tier", rename_all = "lowercase")]
pub enum SubscriptionTier {
    Starter,
    Pro,
    Enterprise,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UsageLimits {
    pub id: Uuid,
    pub tier: SubscriptionTier,
    pub max_jobs_per_month: Option<i64>,
    pub max_workers: Option<i32>,
    pub max_queues: Option<i32>,
    pub max_storage_gb: Option<f64>,
    pub max_api_calls_per_month: Option<i64>,
    pub max_data_transfer_gb: Option<f64>,
    pub priority_support: bool,
    pub custom_domains: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl UsageRecord {
    pub fn new(
        organization_id: Uuid,
        usage_type: UsageType,
        quantity: f64,
        unit_price: f64,
        metadata: Value,
        period_start: DateTime<Utc>,
        period_end: DateTime<Utc>,
    ) -> Self {
        let now = Utc::now();
        let total_cost = quantity * unit_price;
        
        Self {
            id: Uuid::new_v4(),
            organization_id,
            usage_type,
            quantity,
            unit_price,
            total_cost,
            timestamp: now,
            metadata,
            period_start,
            period_end,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.quantity < 0.0 {
            return Err("Quantity cannot be negative".to_string());
        }
        
        if self.unit_price < 0.0 {
            return Err("Unit price cannot be negative".to_string());
        }
        
        Ok(())
    }
}

impl Subscription {
    pub fn new(
        organization_id: Uuid,
        tier: SubscriptionTier,
        billing_cycle: BillingCycle,
        auto_renew: bool,
    ) -> Self {
        let now = Utc::now();
        let period_end = match billing_cycle {
            BillingCycle::Monthly => now + chrono::Duration::days(30),
            BillingCycle::Quarterly => now + chrono::Duration::days(90),
            BillingCycle::Annually => now + chrono::Duration::days(365),
        };
        
        let limits = get_usage_limits_for_tier(&tier);
        let price_per_month = get_price_for_tier(&tier);
        
        Self {
            id: Uuid::new_v4(),
            organization_id,
            tier,
            limits,
            price_per_month,
            billing_cycle,
            auto_renew,
            current_period_start: now,
            current_period_end: period_end,
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn is_active(&self) -> bool {
        self.current_period_end > Utc::now()
    }
    
    pub fn is_over_usage(&self, current_usage: &CurrentUsage) -> bool {
        if let Some(max_jobs) = self.limits.max_jobs_per_month {
            if current_usage.jobs_executed > max_jobs {
                return true;
            }
        }
        
        if let Some(max_workers) = self.limits.max_workers {
            if current_usage.active_workers > max_workers {
                return true;
            }
        }
        
        if let Some(max_queues) = self.limits.max_queues {
            if current_usage.queues_created > max_queues {
                return true;
            }
        }
        
        if let Some(max_storage) = self.limits.max_storage_gb {
            if current_usage.storage_used_gb > max_storage {
                return true;
            }
        }
        
        false
    }
}

pub struct CurrentUsage {
    pub jobs_executed: i64,
    pub active_workers: i32,
    pub queues_created: i32,
    pub storage_used_gb: f64,
    pub api_calls_made: i64,
    pub data_transferred_gb: f64,
}

fn get_usage_limits_for_tier(tier: &SubscriptionTier) -> UsageLimits {
    match tier {
        SubscriptionTier::Starter => UsageLimits {
            id: Uuid::new_v4(),
            tier: tier.clone(),
            max_jobs_per_month: Some(10_000),
            max_workers: Some(5),
            max_queues: Some(10),
            max_storage_gb: Some(10.0),
            max_api_calls_per_month: Some(100_000),
            max_data_transfer_gb: Some(100.0),
            priority_support: false,
            custom_domains: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        SubscriptionTier::Pro => UsageLimits {
            id: Uuid::new_v4(),
            tier: tier.clone(),
            max_jobs_per_month: Some(100_000),
            max_workers: Some(20),
            max_queues: Some(50),
            max_storage_gb: Some(100.0),
            max_api_calls_per_month: Some(1_000_000),
            max_data_transfer_gb: Some(1000.0),
            priority_support: true,
            custom_domains: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        SubscriptionTier::Enterprise => UsageLimits {
            id: Uuid::new_v4(),
            tier: tier.clone(),
            max_jobs_per_month: Some(1_000_000),
            max_workers: Some(100),
            max_queues: Some(200),
            max_storage_gb: Some(1000.0),
            max_api_calls_per_month: Some(10_000),
            max_data_transfer_gb: Some(10_000.0),
            priority_support: true,
            custom_domains: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        SubscriptionTier::Custom => UsageLimits {
            id: Uuid::new_v4(),
            tier: tier.clone(),
            max_jobs_per_month: None,
            max_workers: None,
            max_queues: None,
            max_storage_gb: None,
            max_api_calls_per_month: None,
            max_data_transfer_gb: None,
            priority_support: true,
            custom_domains: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
    }
}

fn get_price_for_tier(tier: &SubscriptionTier) -> f64 {
    match tier {
        SubscriptionTier::Starter => 29.0,
        SubscriptionTier::Pro => 99.0,
        SubscriptionTier::Enterprise => 299.0,
        SubscriptionTier::Custom => 0.0, // Ditentukan secara kustom
    }
}
```

### 2.2. Model Payment dan Invoicing

```rust
// File: src/models/payment.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "payment_method", rename_all = "lowercase")]
pub enum PaymentMethod {
    CreditCard,
    BankTransfer,
    PayPal,
    Stripe,
    WireTransfer,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "payment_status", rename_all = "lowercase")]
pub enum PaymentStatus {
    Pending,
    Processing,
    Succeeded,
    Failed,
    Refunded,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Payment {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub billing_record_id: Uuid,
    pub payment_method: PaymentMethod,
    pub amount: f64,
    pub currency: String,
    pub status: PaymentStatus,
    pub transaction_id: Option<String>, // ID transaksi dari gateway pembayaran
    pub gateway_data: serde_json::Value, // Data tambahan dari gateway pembayaran
    pub processed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Invoice {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub billing_record_id: Option<Uuid>,
    pub invoice_number: String,
    pub issue_date: DateTime<Utc>,
    pub due_date: DateTime<Utc>,
    pub subtotal: f64,
    pub tax_amount: f64,
    pub total_amount: f64,
    pub currency: String,
    pub status: InvoiceStatus,
    pub pdf_url: Option<String>,
    pub line_items: serde_json::Value, // Daftar item tagihan
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "invoice_status", rename_all = "lowercase")]
pub enum InvoiceStatus {
    Draft,
    Sent,
    Viewed,
    Paid,
    Overdue,
    Cancelled,
    Void,
}

impl Payment {
    pub fn new(
        organization_id: Uuid,
        billing_record_id: Uuid,
        payment_method: PaymentMethod,
        amount: f64,
        currency: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            organization_id,
            billing_record_id,
            payment_method,
            amount,
            currency,
            status: PaymentStatus::Pending,
            transaction_id: None,
            gateway_data: serde_json::json!({}),
            processed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
    
    pub fn process_payment(&mut self) {
        self.status = PaymentStatus::Processing;
        self.updated_at = Utc::now();
    }
    
    pub fn complete_payment(&mut self, transaction_id: String, gateway_data: serde_json::Value) {
        self.status = PaymentStatus::Succeeded;
        self.transaction_id = Some(transaction_id);
        self.gateway_data = gateway_data;
        self.processed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn fail_payment(&mut self) {
        self.status = PaymentStatus::Failed;
        self.updated_at = Utc::now();
    }
}

impl Invoice {
    pub fn new(
        organization_id: Uuid,
        invoice_number: String,
        issue_date: DateTime<Utc>,
        due_date: DateTime<Utc>,
        subtotal: f64,
        tax_amount: f64,
        currency: String,
    ) -> Self {
        let total_amount = subtotal + tax_amount;
        
        Self {
            id: Uuid::new_v4(),
            organization_id,
            billing_record_id: None,
            invoice_number,
            issue_date,
            due_date,
            subtotal,
            tax_amount,
            total_amount,
            currency,
            status: InvoiceStatus::Draft,
            pdf_url: None,
            line_items: serde_json::json!([]),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
    
    pub fn add_line_item(&mut self, item: InvoiceLineItem) {
        if let serde_json::Value::Array(ref mut items) = self.line_items {
            items.push(serde_json::json!(item));
        }
        self.subtotal += item.amount;
        self.total_amount = self.subtotal + self.tax_amount;
    }
    
    pub fn send(&mut self) {
        self.status = InvoiceStatus::Sent;
        self.updated_at = Utc::now();
    }
    
    pub fn mark_as_paid(&mut self) {
        self.status = InvoiceStatus::Paid;
        self.updated_at = Utc::now();
    }
    
    pub fn is_overdue(&self) -> bool {
        self.due_date < Utc::now() && self.status == InvoiceStatus::Sent
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct InvoiceLineItem {
    pub id: Uuid,
    pub description: String,
    pub quantity: f64,
    pub unit_price: f64,
    pub amount: f64,
    pub created_at: DateTime<Utc>,
}
```

## 3. Layanan Metering

### 3.1. Layanan Metering Utama

```rust
// File: src/services/metering_service.rs
use crate::{
    models::{usage::{UsageRecord, UsageType, CurrentUsage, Subscription}, payment::Invoice},
    database::Database,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc, Duration};

pub struct MeteringService {
    db: Database,
}

impl MeteringService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn record_usage(
        &self,
        organization_id: Uuid,
        usage_type: UsageType,
        quantity: f64,
        unit_price: f64,
        metadata: serde_json::Value,
    ) -> Result<UsageRecord, SqlxError> {
        let now = Utc::now();
        let current_period = self.get_current_billing_period(organization_id).await?;
        
        let usage_record = UsageRecord::new(
            organization_id,
            usage_type,
            quantity,
            unit_price,
            metadata,
            current_period.0,
            current_period.1,
        );
        
        usage_record.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        self.db.create_usage_record(usage_record).await
    }
    
    pub async fn get_current_usage(
        &self,
        organization_id: Uuid,
    ) -> Result<CurrentUsage, SqlxError> {
        let now = Utc::now();
        let current_period = self.get_current_billing_period(organization_id).await?;
        
        let jobs_executed = self.db.get_jobs_executed_in_period(
            organization_id,
            current_period.0,
            current_period.1,
        ).await?;
        
        let active_workers = self.db.get_active_workers_count(organization_id).await? as i32;
        
        let queues_created = self.db.get_queues_created_in_period(
            organization_id,
            current_period.0,
            current_period.1,
        ).await? as i32;
        
        let storage_used_gb = self.db.get_storage_used_gb(organization_id).await?;
        
        let api_calls_made = self.db.get_api_calls_in_period(
            organization_id,
            current_period.0,
            current_period.1,
        ).await?;
        
        let data_transferred_gb = self.db.get_data_transferred_gb_in_period(
            organization_id,
            current_period.0,
            current_period.1,
        ).await?;
        
        Ok(CurrentUsage {
            jobs_executed,
            active_workers,
            queues_created,
            storage_used_gb,
            api_calls_made,
            data_transferred_gb,
        })
    }
    
    pub async fn get_usage_by_type(
        &self,
        organization_id: Uuid,
        usage_type: UsageType,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<f64, SqlxError> {
        self.db.get_usage_by_type_in_period(organization_id, usage_type, start_time, end_time).await
    }
    
    pub async fn get_usage_records(
        &self,
        organization_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<UsageRecord>, SqlxError> {
        self.db.get_usage_records_in_period(organization_id, start_time, end_time, limit, offset).await
    }
    
    pub async fn get_usage_summary(
        &self,
        organization_id: Uuid,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<UsageSummary, SqlxError> {
        let usage_records = self.get_usage_records(organization_id, start_time, end_time, None, None).await?;
        
        let mut summary = UsageSummary::new();
        
        for record in usage_records {
            summary.total_cost += record.total_cost;
            summary.total_quantity += record.quantity;
            
            match record.usage_type {
                UsageType::JobExecution => {
                    summary.job_execution_cost += record.total_cost;
                    summary.job_execution_quantity += record.quantity;
                },
                UsageType::JobProcessingTime => {
                    summary.job_processing_time_cost += record.total_cost;
                    summary.job_processing_time_quantity += record.quantity;
                },
                UsageType::QueueStorage => {
                    summary.queue_storage_cost += record.total_cost;
                    summary.queue_storage_quantity += record.quantity;
                },
                UsageType::WorkerHours => {
                    summary.worker_hours_cost += record.total_cost;
                    summary.worker_hours_quantity += record.quantity;
                },
                UsageType::ApiCall => {
                    summary.api_call_cost += record.total_cost;
                    summary.api_call_quantity += record.quantity;
                },
                UsageType::DataTransfer => {
                    summary.data_transfer_cost += record.total_cost;
                    summary.data_transfer_quantity += record.quantity;
                },
            }
        }
        
        Ok(summary)
    }
    
    pub async fn check_usage_limits(
        &self,
        organization_id: Uuid,
    ) -> Result<UsageLimitCheck, SqlxError> {
        let subscription = self.db.get_organization_subscription(organization_id).await?;
        if subscription.is_none() {
            return Ok(UsageLimitCheck {
                is_within_limits: false,
                exceeded_limits: vec!["No subscription found".to_string()],
                current_usage: CurrentUsage {
                    jobs_executed: 0,
                    active_workers: 0,
                    queues_created: 0,
                    storage_used_gb: 0.0,
                    api_calls_made: 0,
                    data_transferred_gb: 0.0,
                },
                limits: Default::default(),
            });
        }
        
        let subscription = subscription.unwrap();
        let current_usage = self.get_current_usage(organization_id).await?;
        
        let mut exceeded_limits = Vec::new();
        
        if let Some(max_jobs) = subscription.limits.max_jobs_per_month {
            if current_usage.jobs_executed > max_jobs {
                exceeded_limits.push(format!("Jobs executed ({}) exceeds limit ({})", 
                    current_usage.jobs_executed, max_jobs));
            }
        }
        
        if let Some(max_workers) = subscription.limits.max_workers {
            if current_usage.active_workers > max_workers {
                exceeded_limits.push(format!("Active workers ({}) exceeds limit ({})", 
                    current_usage.active_workers, max_workers));
            }
        }
        
        if let Some(max_queues) = subscription.limits.max_queues {
            if current_usage.queues_created > max_queues {
                exceeded_limits.push(format!("Queues created ({}) exceeds limit ({})", 
                    current_usage.queues_created, max_queues));
            }
        }
        
        if let Some(max_storage) = subscription.limits.max_storage_gb {
            if current_usage.storage_used_gb > max_storage {
                exceeded_limits.push(format!("Storage used ({:.2}GB) exceeds limit ({:.2}GB)", 
                    current_usage.storage_used_gb, max_storage));
            }
        }
        
        if let Some(max_api_calls) = subscription.limits.max_api_calls_per_month {
            if current_usage.api_calls_made > max_api_calls {
                exceeded_limits.push(format!("API calls made ({}) exceeds limit ({})", 
                    current_usage.api_calls_made, max_api_calls));
            }
        }
        
        if let Some(max_data_transfer) = subscription.limits.max_data_transfer_gb {
            if current_usage.data_transferred_gb > max_data_transfer {
                exceeded_limits.push(format!("Data transferred ({:.2}GB) exceeds limit ({:.2}GB)", 
                    current_usage.data_transferred_gb, max_data_transfer));
            }
        }
        
        Ok(UsageLimitCheck {
            is_within_limits: exceeded_limits.is_empty(),
            exceeded_limits,
            current_usage,
            limits: subscription.limits,
        })
    }
    
    async fn get_current_billing_period(&self, organization_id: Uuid) -> Result<(DateTime<Utc>, DateTime<Utc>), SqlxError> {
        // Dapatkan periode penagihan saat ini berdasarkan subscription
        let subscription = self.db.get_organization_subscription(organization_id).await?;
        
        if let Some(sub) = subscription {
            Ok((sub.current_period_start, sub.current_period_end))
        } else {
            // Jika tidak ada subscription, gunakan periode bulan ini
            let now = Utc::now();
            let start = now.with_day(1).unwrap().with_hour(0).unwrap().with_minute(0).unwrap().with_second(0).unwrap();
            let end = start + Duration::days(30);
            Ok((start, end))
        }
    }
    
    pub async fn generate_invoice(
        &self,
        organization_id: Uuid,
    ) -> Result<Invoice, SqlxError> {
        let subscription = self.db.get_organization_subscription(organization_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        let usage_summary = self.get_usage_summary(
            organization_id,
            subscription.current_period_start,
            subscription.current_period_end,
        ).await?;
        
        let mut invoice = Invoice::new(
            organization_id,
            format!("INV-{:06}", organization_id.to_string()[..6].to_uppercase()),
            Utc::now(),
            Utc::now() + Duration::days(30), // Jatuh tempo dalam 30 hari
            usage_summary.total_cost,
            usage_summary.total_cost * 0.1, // PPN 10%
            "USD".to_string(),
        );
        
        // Tambahkan item berdasarkan jenis penggunaan
        if usage_summary.job_execution_quantity > 0.0 {
            invoice.add_line_item(InvoiceLineItem {
                id: Uuid::new_v4(),
                description: "Job Executions".to_string(),
                quantity: usage_summary.job_execution_quantity,
                unit_price: 0.001, // $0.001 per job
                amount: usage_summary.job_execution_cost,
                created_at: Utc::now(),
            });
        }
        
        if usage_summary.api_call_quantity > 0.0 {
            invoice.add_line_item(InvoiceLineItem {
                id: Uuid::new_v4(),
                description: "API Calls".to_string(),
                quantity: usage_summary.api_call_quantity,
                unit_price: 0.00001, // $0.00001 per API call
                amount: usage_summary.api_call_cost,
                created_at: Utc::now(),
            });
        }
        
        // Tambahkan biaya bulanan berdasarkan tier
        invoice.add_line_item(InvoiceLineItem {
            id: Uuid::new_v4(),
            description: format!("{} Plan Monthly Fee", subscription.tier),
            quantity: 1.0,
            unit_price: subscription.price_per_month,
            amount: subscription.price_per_month,
            created_at: Utc::now(),
        });
        
        Ok(invoice)
    }
}

pub struct UsageSummary {
    pub total_cost: f64,
    pub total_quantity: f64,
    pub job_execution_cost: f64,
    pub job_execution_quantity: f64,
    pub job_processing_time_cost: f64,
    pub job_processing_time_quantity: f64,
    pub queue_storage_cost: f64,
    pub queue_storage_quantity: f64,
    pub worker_hours_cost: f64,
    pub worker_hours_quantity: f64,
    pub api_call_cost: f64,
    pub api_call_quantity: f64,
    pub data_transfer_cost: f64,
    pub data_transfer_quantity: f64,
}

impl UsageSummary {
    pub fn new() -> Self {
        Self {
            total_cost: 0.0,
            total_quantity: 0.0,
            job_execution_cost: 0.0,
            job_execution_quantity: 0.0,
            job_processing_time_cost: 0.0,
            job_processing_time_quantity: 0.0,
            queue_storage_cost: 0.0,
            queue_storage_quantity: 0.0,
            worker_hours_cost: 0.0,
            worker_hours_quantity: 0.0,
            api_call_cost: 0.0,
            api_call_quantity: 0.0,
            data_transfer_cost: 0.0,
            data_transfer_quantity: 0.0,
        }
    }
}

pub struct UsageLimitCheck {
    pub is_within_limits: bool,
    pub exceeded_limits: Vec<String>,
    pub current_usage: CurrentUsage,
    pub limits: crate::models::usage::UsageLimits,
}
```

### 3.2. Layanan Subscription dan Billing

```rust
// File: src/services/subscription_service.rs
use crate::{
    models::{
        usage::{Subscription, SubscriptionTier, BillingCycle, UsageLimits},
        payment::{Payment, PaymentMethod, PaymentStatus, Invoice, InvoiceStatus},
    },
    database::Database,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc, Duration};

pub struct SubscriptionService {
    db: Database,
    metering_service: MeteringService,
}

impl SubscriptionService {
    pub fn new(db: Database, metering_service: MeteringService) -> Self {
        Self { db, metering_service }
    }
    
    pub async fn create_subscription(
        &self,
        organization_id: Uuid,
        tier: SubscriptionTier,
        billing_cycle: BillingCycle,
        auto_renew: bool,
    ) -> Result<Subscription, SqlxError> {
        let subscription = Subscription::new(organization_id, tier, billing_cycle, auto_renew);
        self.db.create_subscription(subscription).await
    }
    
    pub async fn get_organization_subscription(
        &self,
        organization_id: Uuid,
    ) -> Result<Option<Subscription>, SqlxError> {
        self.db.get_organization_subscription(organization_id).await
    }
    
    pub async fn update_subscription_tier(
        &self,
        organization_id: Uuid,
        new_tier: SubscriptionTier,
    ) -> Result<(), SqlxError> {
        let mut subscription = self.db.get_organization_subscription(organization_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Hitung biaya upgrade/downgrade
        let price_difference = get_price_for_tier(&new_tier) - subscription.price_per_month;
        
        // Update tier dan harga
        subscription.tier = new_tier;
        subscription.limits = get_usage_limits_for_tier(&new_tier);
        subscription.price_per_month = get_price_for_tier(&new_tier);
        subscription.updated_at = Utc::now();
        
        // Jika ada perbedaan harga, buat invoice tambahan
        if price_difference != 0.0 {
            self.create_proration_invoice(organization_id, price_difference).await?;
        }
        
        self.db.update_subscription(subscription).await?;
        Ok(())
    }
    
    async fn create_proration_invoice(&self, organization_id: Uuid, amount: f64) -> Result<(), SqlxError> {
        let invoice = Invoice::new(
            organization_id,
            format!("PROR-{:06}", organization_id.to_string()[..6].to_uppercase()),
            Utc::now(),
            Utc::now() + Duration::days(7), // Jatuh tempo dalam 7 hari untuk prorated charges
            amount.abs(),
            if amount > 0.0 { amount * 0.1 } else { 0.0 }, // PPN hanya untuk charge, bukan credit
            "USD".to_string(),
        );
        
        let mut invoice_with_items = invoice;
        invoice_with_items.add_line_item(InvoiceLineItem {
            id: Uuid::new_v4(),
            description: if amount > 0.0 { "Plan Upgrade Proration".to_string() } else { "Plan Downgrade Credit".to_string() },
            quantity: 1.0,
            unit_price: amount,
            amount,
            created_at: Utc::now(),
        });
        
        self.db.create_invoice(invoice_with_items).await?;
        Ok(())
    }
    
    pub async fn process_billing_cycle(&self) -> Result<(), SqlxError> {
        let now = Utc::now();
        
        // Ambil semua subscription yang periode penagihannya habis
        let expired_subscriptions = self.db.get_expired_subscriptions(now).await?;
        
        for subscription in expired_subscriptions {
            // Generate invoice untuk periode baru
            let invoice = self.metering_service.generate_invoice(subscription.organization_id).await?;
            
            // Update periode penagihan subscription
            let mut updated_subscription = subscription;
            let cycle_duration = match updated_subscription.billing_cycle {
                BillingCycle::Monthly => Duration::days(30),
                BillingCycle::Quarterly => Duration::days(90),
                BillingCycle::Annually => Duration::days(365),
            };
            
            updated_subscription.current_period_start = updated_subscription.current_period_end;
            updated_subscription.current_period_end = updated_subscription.current_period_start + cycle_duration;
            updated_subscription.updated_at = now;
            
            // Simpan invoice dan update subscription
            self.db.create_invoice(invoice).await?;
            self.db.update_subscription(updated_subscription).await?;
        }
        
        Ok(())
    }
    
    pub async fn process_payments(&self) -> Result<(), SqlxError> {
        // Ambil semua invoice yang belum dibayar dan jatuh tempo
        let due_invoices = self.db.get_due_invoices(Utc::now()).await?;
        
        for invoice in due_invoices {
            // Dalam implementasi nyata, ini akan memicu pembayaran otomatis
            // berdasarkan metode pembayaran yang tersimpan
            
            // Untuk simulasi, kita buat pembayaran otomatis
            let mut payment = Payment::new(
                invoice.organization_id,
                invoice.id, // Dalam implementasi nyata, ini mungkin berbeda
                PaymentMethod::CreditCard, // Metode default
                invoice.total_amount,
                invoice.currency,
            );
            
            // Proses pembayaran (dalam implementasi nyata, ini akan terhubung ke payment gateway)
            payment.complete_payment(
                format!("txn_{}", Uuid::new_v4()),
                serde_json::json!({"gateway": "stripe", "method": "card"}),
            );
            
            // Update status invoice
            let mut updated_invoice = invoice;
            updated_invoice.mark_as_paid();
            
            self.db.update_invoice(updated_invoice).await?;
            self.db.create_payment(payment).await?;
        }
        
        Ok(())
    }
    
    pub async fn check_over_usage_and_notify(&self) -> Result<(), SqlxError> {
        // Ambil semua organisasi
        let organizations = self.db.get_all_organizations().await?;
        
        for org in organizations {
            let usage_check = self.metering_service.check_usage_limits(org.id).await?;
            
            if !usage_check.is_within_limits {
                // Kirim notifikasi kepada organisasi tentang penggunaan berlebih
                self.send_over_usage_notification(&org, &usage_check).await?;
            }
        }
        
        Ok(())
    }
    
    async fn send_over_usage_notification(
        &self,
        organization: &crate::models::Organization, // Asumsikan model Organization ada
        usage_check: &UsageLimitCheck,
    ) -> Result<(), SqlxError> {
        // Dalam implementasi nyata, ini akan mengirim email atau notifikasi push
        println!("Over-usage notification for organization {}: {:?}", organization.name, usage_check.exceeded_limits);
        
        // Simpan notifikasi ke database
        self.db.create_usage_notification(
            organization.id,
            "over_usage".to_string(),
            serde_json::json!(usage_check.exceeded_limits),
        ).await?;
        
        Ok(())
    }
    
    pub async fn get_billing_summary(
        &self,
        organization_id: Uuid,
    ) -> Result<BillingSummary, SqlxError> {
        let subscription = self.db.get_organization_subscription(organization_id).await?;
        let current_usage = self.metering_service.get_current_usage(organization_id).await?;
        let usage_limit_check = self.metering_service.check_usage_limits(organization_id).await?;
        
        let upcoming_invoice = self.metering_service.generate_invoice(organization_id).await.ok();
        
        Ok(BillingSummary {
            subscription: subscription,
            current_usage,
            usage_limit_check,
            upcoming_invoice_amount: upcoming_invoice.as_ref().map(|inv| inv.total_amount),
            next_billing_date: subscription.as_ref().map(|sub| sub.current_period_end),
        })
    }
}

pub struct BillingSummary {
    pub subscription: Option<Subscription>,
    pub current_usage: CurrentUsage,
    pub usage_limit_check: UsageLimitCheck,
    pub upcoming_invoice_amount: Option<f64>,
    pub next_billing_date: Option<DateTime<Utc>>,
}
```

## 4. Integrasi Payment Gateway

### 4.1. Payment Gateway Interface

```rust
// File: src/services/payment_gateway.rs
use crate::models::payment::{Payment, PaymentMethod, PaymentStatus};
use uuid::Uuid;
use std::collections::HashMap;

#[async_trait::async_trait]
pub trait PaymentGateway: Send + Sync {
    async fn process_payment(&self, payment: &Payment) -> Result<PaymentResult, PaymentError>;
    async fn refund_payment(&self, transaction_id: &str, amount: f64) -> Result<RefundResult, PaymentError>;
    async fn get_payment_status(&self, transaction_id: &str) -> Result<PaymentStatus, PaymentError>;
}

#[derive(Debug)]
pub struct PaymentResult {
    pub transaction_id: String,
    pub status: PaymentStatus,
    pub gateway_data: serde_json::Value,
    pub fees: f64,
}

#[derive(Debug)]
pub struct RefundResult {
    pub refund_id: String,
    pub status: PaymentStatus,
    pub refunded_amount: f64,
}

#[derive(Debug)]
pub enum PaymentError {
    GatewayError(String),
    ValidationError(String),
    NetworkError(String),
}

pub struct StripePaymentGateway {
    api_key: String,
    base_url: String,
}

impl StripePaymentGateway {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.stripe.com/v1".to_string(),
        }
    }
}

#[async_trait::async_trait]
impl PaymentGateway for StripePaymentGateway {
    async fn process_payment(&self, payment: &Payment) -> Result<PaymentResult, PaymentError> {
        // Dalam implementasi nyata, ini akan membuat permintaan ke API Stripe
        // Untuk simulasi, kita buat hasil sukses
        Ok(PaymentResult {
            transaction_id: format!("pi_{}", Uuid::new_v4()),
            status: PaymentStatus::Succeeded,
            gateway_data: serde_json::json!({
                "gateway": "stripe",
                "payment_method": "card",
                "receipt_url": "https://stripe.com/receipt"
            }),
            fees: payment.amount * 0.029 + 0.30, // Biaya Stripe 2.9% + $0.30
        })
    }
    
    async fn refund_payment(&self, transaction_id: &str, amount: f64) -> Result<RefundResult, PaymentError> {
        // Dalam implementasi nyata, ini akan membuat permintaan pengembalian dana ke API Stripe
        Ok(RefundResult {
            refund_id: format!("re_{}", Uuid::new_v4()),
            status: PaymentStatus::Succeeded,
            refunded_amount: amount,
        })
    }
    
    async fn get_payment_status(&self, transaction_id: &str) -> Result<PaymentStatus, PaymentError> {
        // Dalam implementasi nyata, ini akan memeriksa status pembayaran di API Stripe
        Ok(PaymentStatus::Succeeded)
    }
}

pub struct PaymentProcessor {
    gateways: HashMap<PaymentMethod, Box<dyn PaymentGateway>>,
}

impl PaymentProcessor {
    pub fn new() -> Self {
        let mut processor = Self {
            gateways: HashMap::new(),
        };
        
        // Register default gateways
        // Dalam implementasi nyata, ini akan dikonfigurasi dari environment
        Ok(processor)
    }
    
    pub fn add_gateway(&mut self, method: PaymentMethod, gateway: Box<dyn PaymentGateway>) {
        self.gateways.insert(method, gateway);
    }
    
    pub async fn process_payment(&self, payment: &mut Payment) -> Result<(), PaymentError> {
        let gateway = self.gateways.get(&payment.payment_method)
            .ok_or(PaymentError::ValidationError("Unsupported payment method".to_string()))?;
        
        payment.process_payment();
        
        let result = gateway.process_payment(payment).await?;
        
        if matches!(result.status, PaymentStatus::Succeeded) {
            payment.complete_payment(result.transaction_id, result.gateway_data);
        } else {
            payment.fail_payment();
        }
        
        Ok(())
    }
    
    pub async fn process_refund(&self, transaction_id: &str, amount: f64) -> Result<RefundResult, PaymentError> {
        // Dalam implementasi nyata, kita perlu mencari gateway yang digunakan untuk transaksi asli
        // Untuk simulasi, kita asumsikan pengembalian dana berhasil
        Ok(RefundResult {
            refund_id: format!("re_{}", Uuid::new_v4()),
            status: PaymentStatus::Succeeded,
            refunded_amount: amount,
        })
    }
}
```

## 5. API Endpoints untuk Billing

### 5.1. Billing API Handler

```rust
// File: src/api/billing.rs
use axum::{
    extract::{Path, Query, State, Json},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    models::{
        usage::{Subscription, SubscriptionTier, BillingCycle},
        payment::{Invoice, PaymentMethod},
    },
    services::{
        authorization_service::AuthorizationService,
        metering_service::MeteringService,
        subscription_service::SubscriptionService,
    },
};

#[derive(Deserialize)]
pub struct CreateSubscriptionRequest {
    pub tier: String,           // "starter", "pro", "enterprise", "custom"
    pub billing_cycle: String,  // "monthly", "quarterly", "annually"
    pub auto_renew: bool,
}

#[derive(Deserialize)]
pub struct UpdateSubscriptionRequest {
    pub tier: String,           // "starter", "pro", "enterprise", "custom"
}

#[derive(Deserialize)]
pub struct ProcessPaymentRequest {
    pub invoice_id: Uuid,
    pub payment_method: String, // "credit_card", "paypal", "bank_transfer"
    pub payment_token: String,  // Token pembayaran dari gateway
}

#[derive(Serialize)]
pub struct CreateSubscriptionResponse {
    pub subscription_id: Uuid,
    pub message: String,
    pub next_billing_date: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct BillingSummaryResponse {
    pub subscription: Option<Subscription>,
    pub current_usage: crate::models::usage::CurrentUsage,
    pub usage_limit_check: crate::services::metering_service::UsageLimitCheck,
    pub upcoming_invoice_amount: Option<f64>,
    pub next_billing_date: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize)]
pub struct InvoiceResponse {
    pub invoices: Vec<Invoice>,
    pub total: u32,
    pub page: u32,
    pub pages: u32,
}

pub fn create_billing_router(
    metering_service: Arc<MeteringService>,
    subscription_service: Arc<SubscriptionService>,
    authz_service: Arc<AuthorizationService>,
) -> Router {
    Router::new()
        .route("/subscription", get(get_subscription).post(create_subscription).put(update_subscription))
        .route("/subscription/change-tier", post(change_subscription_tier))
        .route("/usage", get(get_current_usage))
        .route("/usage/summary", get(get_usage_summary))
        .route("/usage/records", get(get_usage_records))
        .route("/invoices", get(get_invoices))
        .route("/invoices/:invoice_id", get(get_invoice))
        .route("/billing-summary", get(get_billing_summary))
        .route("/process-payment", post(process_payment))
        .with_state((metering_service, subscription_service, authz_service))
}

pub async fn get_subscription(
    State((metering_service, subscription_service, authz_service)): State<(Arc<MeteringService>, Arc<SubscriptionService>, Arc<AuthorizationService>)>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "billing:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match subscription_service.get_organization_subscription(user.organization_id).await {
        Ok(Some(subscription)) => Ok(Json(subscription)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn create_subscription(
    State((metering_service, subscription_service, authz_service)): State<(Arc<MeteringService>, Arc<SubscriptionService>, Arc<AuthorizationService>)>,
    Json(request): Json<CreateSubscriptionRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "billing:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Validasi tier
    let tier = match request.tier.as_str() {
        "starter" => SubscriptionTier::Starter,
        "pro" => SubscriptionTier::Pro,
        "enterprise" => SubscriptionTier::Enterprise,
        "custom" => SubscriptionTier::Custom,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    
    // Validasi billing cycle
    let billing_cycle = match request.billing_cycle.as_str() {
        "monthly" => BillingCycle::Monthly,
        "quarterly" => BillingCycle::Quarterly,
        "annually" => BillingCycle::Annually,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    
    match subscription_service.create_subscription(
        user.organization_id,
        tier,
        billing_cycle,
        request.auto_renew,
    ).await {
        Ok(subscription) => {
            let response = CreateSubscriptionResponse {
                subscription_id: subscription.id,
                message: "Subscription created successfully".to_string(),
                next_billing_date: subscription.current_period_end,
            };
            Ok(Json(response))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn update_subscription(
    State((metering_service, subscription_service, authz_service)): State<(Arc<MeteringService>, Arc<SubscriptionService>, Arc<AuthorizationService>)>,
    Json(request): Json<UpdateSubscriptionRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "billing:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Validasi tier
    let tier = match request.tier.as_str() {
        "starter" => SubscriptionTier::Starter,
        "pro" => SubscriptionTier::Pro,
        "enterprise" => SubscriptionTier::Enterprise,
        "custom" => SubscriptionTier::Custom,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    
    match subscription_service.update_subscription_tier(user.organization_id, tier).await {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(sqlx::Error::RowNotFound) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn change_subscription_tier(
    State((metering_service, subscription_service, authz_service)): State<(Arc<MeteringService>, Arc<SubscriptionService>, Arc<AuthorizationService>)>,
    Json(request): Json<UpdateSubscriptionRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    // Fungsi ini sama dengan update_subscription, tetapi bisa memiliki logika tambahan
    // seperti proration, trial period, dll.
    update_subscription(
        State((metering_service, subscription_service, authz_service)),
        Json(request),
    ).await
}

pub async fn get_current_usage(
    State((metering_service, subscription_service, authz_service)): State<(Arc<MeteringService>, Arc<SubscriptionService>, Arc<AuthorizationService>)>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "billing:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match metering_service.get_current_usage(user.organization_id).await {
        Ok(usage) => Ok(Json(usage)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_usage_summary(
    State((metering_service, subscription_service, authz_service)): State<(Arc<MeteringService>, Arc<SubscriptionService>, Arc<AuthorizationService>)>,
    Query(params): Query<TimeRangeQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "billing:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let start_time = params.start_time.unwrap_or_else(|| {
        chrono::Utc::now() - chrono::Duration::days(30) // Default: 30 hari terakhir
    });
    let end_time = params.end_time.unwrap_or_else(chrono::Utc::now);
    
    match metering_service.get_usage_summary(user.organization_id, start_time, end_time).await {
        Ok(summary) => Ok(Json(summary)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_usage_records(
    State((metering_service, subscription_service, authz_service)): State<(Arc<MeteringService>, Arc<SubscriptionService>, Arc<AuthorizationService>)>,
    Query(params): Query<UsageRecordsQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "billing:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    let start_time = params.start_time.unwrap_or_else(|| {
        chrono::Utc::now() - chrono::Duration::days(30)
    });
    let end_time = params.end_time.unwrap_or_else(chrono::Utc::now);
    let limit = params.limit.or(Some(50)); // Default: 50 records
    let offset = params.offset;
    
    match metering_service.get_usage_records(user.organization_id, start_time, end_time, limit, offset).await {
        Ok(records) => Ok(Json(serde_json::json!({
            "records": records,
            "total": records.len(),
            "page": (offset.unwrap_or(0) / limit.unwrap_or(50)) + 1,
        }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_billing_summary(
    State((metering_service, subscription_service, authz_service)): State<(Arc<MeteringService>, Arc<SubscriptionService>, Arc<AuthorizationService>)>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "billing:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    match subscription_service.get_billing_summary(user.organization_id).await {
        Ok(summary) => {
            let response = BillingSummaryResponse {
                subscription: summary.subscription,
                current_usage: summary.current_usage,
                usage_limit_check: summary.usage_limit_check,
                upcoming_invoice_amount: summary.upcoming_invoice_amount,
                next_billing_date: summary.next_billing_date,
            };
            Ok(Json(response))
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn get_invoices(
    State((metering_service, subscription_service, authz_service)): State<(Arc<MeteringService>, Arc<SubscriptionService>, Arc<AuthorizationService>)>,
    Query(params): Query<InvoiceQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "billing:read").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Dalam implementasi nyata, ambil invoices dari database
    let invoices = Vec::new(); // Placeholder
    
    let response = InvoiceResponse {
        invoices,
        total: 0,
        page: params.page.unwrap_or(1),
        pages: 1,
    };
    
    Ok(Json(response))
}

pub async fn process_payment(
    State((metering_service, subscription_service, authz_service)): State<(Arc<MeteringService>, Arc<SubscriptionService>, Arc<AuthorizationService>)>,
    Json(request): Json<ProcessPaymentRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let user = authz_service.get_current_user().await
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Periksa izin
    if !authz_service.has_permission(&user, "billing:write").await {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Dalam implementasi nyata, proses pembayaran akan terjadi di sini
    // menggunakan payment gateway yang sesuai
    
    // Validasi metode pembayaran
    let payment_method = match request.payment_method.as_str() {
        "credit_card" => PaymentMethod::CreditCard,
        "paypal" => PaymentMethod::PayPal,
        "bank_transfer" => PaymentMethod::BankTransfer,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    
    // Dapatkan invoice
    let invoice = metering_service.db.get_invoice_by_id(request.invoice_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Pastikan invoice milik organisasi yang benar
    if invoice.organization_id != user.organization_id {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Buat record pembayaran
    let mut payment = crate::models::payment::Payment::new(
        user.organization_id,
        invoice.id,
        payment_method,
        invoice.total_amount,
        invoice.currency,
    );
    
    // Dalam implementasi nyata, panggil payment gateway
    // Untuk simulasi, kita anggap pembayaran berhasil
    payment.complete_payment(
        format!("txn_{}", Uuid::new_v4()),
        serde_json::json!({"gateway": "simulated", "method": request.payment_method}),
    );
    
    // Simpan pembayaran
    metering_service.db.create_payment(payment).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // Update status invoice
    let mut updated_invoice = invoice;
    updated_invoice.mark_as_paid();
    metering_service.db.update_invoice(updated_invoice).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct TimeRangeQuery {
    start_time: Option<chrono::DateTime<chrono::Utc>>,
    end_time: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Deserialize)]
struct UsageRecordsQuery {
    start_time: Option<chrono::DateTime<chrono::Utc>>,
    end_time: Option<chrono::DateTime<chrono::Utc>>,
    limit: Option<i32>,
    offset: Option<i32>,
    usage_type: Option<String>,
}

#[derive(Deserialize)]
struct InvoiceQuery {
    page: Option<u32>,
    limit: Option<u32>,
    status: Option<String>,
}
```

## 6. Best Practices dan Rekomendasi

### 6.1. Praktik Terbaik untuk Metering

1. **Gunakan presisi tinggi untuk pengukuran** - untuk akurasi penagihan
2. **Implementasikan audit trail** - untuk keperluan akuntansi dan troubleshooting
3. **Gunakan rate limiting untuk proteksi abuse** - untuk mencegah penggunaan berlebihan
4. **Simpan data penggunaan secara redundan** - untuk perlindungan data
5. **Gunakan encryption untuk data sensitif** - untuk perlindungan privasi

### 6.2. Praktik Terbaik untuk Billing

1. **Gunakan sistem penagihan yang andal** - untuk pengalaman pengguna yang baik
2. **Implementasikan proration untuk perubahan tier** - untuk keadilan penagihan
3. **Gunakan multiple payment gateways** - untuk fleksibilitas dan keandalan
4. **Implementasikan sistem notifikasi** - untuk keeping pelanggan terinformasi
5. **Gunakan data retention policies** - untuk manajemen penyimpanan

### 6.3. Skalabilitas dan Kinerja

1. **Gunakan indexing yang tepat** - pada tabel penggunaan untuk query yang cepat
2. **Gunakan partitioning untuk data historis** - untuk efisiensi manajemen data
3. **Gunakan caching untuk informasi subscription** - untuk mengurangi beban database
4. **Gunakan batch processing untuk pengumpulan penggunaan** - untuk efisiensi
5. **Gunakan connection pooling** - untuk efisiensi koneksi database