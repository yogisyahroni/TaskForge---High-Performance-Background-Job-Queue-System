use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "organization_tier", rename_all = "lowercase")]
pub enum OrganizationTier {
    Starter,
    Pro,
    Enterprise,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub billing_email: String,
    pub tier: OrganizationTier,
    pub metadata: Value,              // JSONB field untuk metadata tambahan
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Organization {
    pub fn new(name: String, billing_email: String, tier: OrganizationTier) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            billing_email,
            tier,
            metadata: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.len() > 100 {
            return Err("Organization name must be between 1 and 100 characters".to_string());
        }
        
        if !self.billing_email.contains('@') || self.billing_email.len() > 254 {
            return Err("Invalid billing email format".to_string());
        }
        
        Ok(())
    }
    
    pub fn get_usage_limits(&self) -> UsageLimits {
        match self.tier {
            OrganizationTier::Starter => UsageLimits {
                max_projects: 1,
                max_workers: 5,
                max_queues: 10,
                max_jobs_per_month: 10_000,
                max_api_calls_per_month: 100_000,
                max_storage_gb: 10.0,
                max_data_transfer_gb: 100.0,
            },
            OrganizationTier::Pro => UsageLimits {
                max_projects: 5,
                max_workers: 20,
                max_queues: 50,
                max_jobs_per_month: 100_000,
                max_api_calls_per_month: 1_000_000,
                max_storage_gb: 100.0,
                max_data_transfer_gb: 1000.0,
            },
            OrganizationTier::Enterprise => UsageLimits {
                max_projects: 100,
                max_workers: 100,
                max_queues: 200,
                max_jobs_per_month: 1_000_000,
                max_api_calls_per_month: 10_000,
                max_storage_gb: 1000.0,
                max_data_transfer_gb: 10_000.0,
            },
            OrganizationTier::Custom => {
                // Dalam implementasi nyata, ini akan diambil dari konfigurasi khusus
                UsageLimits {
                    max_projects: 1000,
                    max_workers: 1000,
                    max_queues: 1000,
                    max_jobs_per_month: 10_000,
                    max_api_calls_per_month: 100_000,
                    max_storage_gb: 10_000.0,
                    max_data_transfer_gb: 100_000.0,
                }
            },
        }
    }
    
    pub fn can_create_project(&self, current_project_count: u32) -> bool {
        let limits = self.get_usage_limits();
        current_project_count < limits.max_projects
    }
    
    pub fn can_add_worker(&self, current_worker_count: u32) -> bool {
        let limits = self.get_usage_limits();
        current_worker_count < limits.max_workers
    }
    
    pub fn can_add_queue(&self, current_queue_count: u32) -> bool {
        let limits = self.get_usage_limits();
        current_queue_count < limits.max_queues
    }
    
    pub fn is_usage_within_limits(&self, usage: &CurrentUsage) -> bool {
        let limits = self.get_usage_limits();
        
        usage.projects <= limits.max_projects &&
        usage.workers <= limits.max_workers &&
        usage.queues <= limits.max_queues &&
        usage.jobs_this_month <= limits.max_jobs_per_month &&
        usage.api_calls_this_month <= limits.max_api_calls_per_month
    }
    
    pub fn get_remaining_quota(&self, current_usage: &CurrentUsage) -> RemainingQuota {
        let limits = self.get_usage_limits();
        
        RemainingQuota {
            remaining_projects: limits.max_projects.saturating_sub(current_usage.projects),
            remaining_workers: limits.max_workers.saturating_sub(current_usage.workers),
            remaining_queues: limits.max_queues.saturating_sub(current_usage.queues),
            remaining_jobs_this_month: limits.max_jobs_per_month.saturating_sub(current_usage.jobs_this_month),
            remaining_api_calls_this_month: limits.max_api_calls_per_month.saturating_sub(current_usage.api_calls_this_month),
        }
    }
}

pub struct UsageLimits {
    pub max_projects: u32,
    pub max_workers: u32,
    pub max_queues: u32,
    pub max_jobs_per_month: u64,
    pub max_api_calls_per_month: u64,
    pub max_storage_gb: f64,
    pub max_data_transfer_gb: f64,
}

pub struct CurrentUsage {
    pub projects: u32,
    pub workers: u32,
    pub queues: u32,
    pub jobs_this_month: u64,
    pub api_calls_this_month: u64,
    pub storage_used_gb: f64,
    pub data_transfer_used_gb: f64,
}

pub struct RemainingQuota {
    pub remaining_projects: u32,
    pub remaining_workers: u32,
    pub remaining_queues: u32,
    pub remaining_jobs_this_month: u64,
    pub remaining_api_calls_this_month: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrganizationUsage {
    pub organization_id: Uuid,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_jobs_processed: u64,
    pub total_compute_time_seconds: f64,
    pub total_data_processed_mb: f64,
    pub total_api_calls: u64,
    pub storage_used_gb: f64,
    pub bandwidth_used_gb: f64,
    pub created_at: DateTime<Utc>,
}

impl OrganizationUsage {
    pub fn new(organization_id: Uuid) -> Self {
        let now = Utc::now();
        let month_start = now.with_day(1).unwrap().with_hour(0).unwrap().with_minute(0).unwrap().with_second(0).unwrap();
        let month_end = month_start + chrono::Duration::days(30);
        
        Self {
            organization_id,
            period_start: month_start,
            period_end: month_end,
            total_jobs_processed: 0,
            total_compute_time_seconds: 0.0,
            total_data_processed_mb: 0.0,
            total_api_calls: 0,
            storage_used_gb: 0.0,
            bandwidth_used_gb: 0.0,
            created_at: now,
        }
    }
    
    pub fn add_job_processed(&mut self, compute_time_seconds: f64, data_processed_mb: f64) {
        self.total_jobs_processed += 1;
        self.total_compute_time_seconds += compute_time_seconds;
        self.total_data_processed_mb += data_processed_mb;
        self.created_at = Utc::now();
    }
    
    pub fn add_api_call(&mut self) {
        self.total_api_calls += 1;
        self.created_at = Utc::now();
    }
    
    pub fn add_storage_usage(&mut self, storage_gb: f64) {
        self.storage_used_gb += storage_gb;
        self.created_at = Utc::now();
    }
    
    pub fn add_bandwidth_usage(&mut self, bandwidth_gb: f64) {
        self.bandwidth_used_gb += bandwidth_gb;
        self.created_at = Utc::now();
    }
    
    pub fn get_usage_percentage(&self, limits: &UsageLimits) -> UsagePercentage {
            projects: 0.0, // Dihitung secara terpisah
            workers: 0.0,  // Dihitung secara terpisah
            queues: 0.0,   // Dihitung secara terpisah
            jobs: (self.total_jobs_processed as f64 / limits.max_jobs_per_month as f64) * 100.0,
            api_calls: (self.total_api_calls as f64 / limits.max_api_calls_per_month as f64) * 100.0,
            storage: (self.storage_used_gb / limits.max_storage_gb) * 100.0,
            bandwidth: (self.bandwidth_used_gb / limits.max_data_transfer_gb) * 100.0,
        }
    }
}

pub struct UsagePercentage {
    pub projects: f64,
    pub workers: f64,
    pub queues: f64,
    pub jobs: f64,
    pub api_calls: f64,
    pub storage: f64,
    pub bandwidth: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Subscription {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub tier: OrganizationTier,
    pub billing_cycle: BillingCycle,
    pub next_billing_date: DateTime<Utc>,
    pub auto_renew: bool,
    pub payment_method_id: Option<Uuid>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "billing_cycle", rename_all = "lowercase")]
pub enum BillingCycle {
    Monthly,
    Quarterly,
    Annually,
}

impl Subscription {
    pub fn new(
        organization_id: Uuid,
        tier: OrganizationTier,
        billing_cycle: BillingCycle,
        auto_renew: bool,
    ) -> Self {
        let now = Utc::now();
        let next_billing_date = match billing_cycle {
            BillingCycle::Monthly => now + chrono::Duration::days(30),
            BillingCycle::Quarterly => now + chrono::Duration::days(90),
            BillingCycle::Annually => now + chrono::Duration::days(365),
        };
        
        Self {
            id: Uuid::new_v4(),
            organization_id,
            tier,
            billing_cycle,
            next_billing_date,
            auto_renew,
            payment_method_id: None,
            metadata: serde_json::json!({}),
            created_at: now,
            updated_at: now,
        }
    }
    
    pub fn is_active(&self) -> bool {
        self.next_billing_date > Utc::now()
    }
    
    pub fn is_near_expiry(&self, days_before_warning: i64) -> bool {
        let warning_date = Utc::now() + chrono::Duration::days(days_before_warning);
        self.next_billing_date < warning_date
    }
    
    pub fn get_remaining_days(&self) -> i64 {
        let today = Utc::now();
        let diff = self.next_billing_date - today;
        diff.num_days()
    }
    
    pub fn calculate_usage_fee(&self, usage: &OrganizationUsage) -> f64 {
        // Dalam implementasi nyata, ini akan menghitung biaya berdasarkan penggunaan
        // dan tier organisasi
        match self.tier {
            OrganizationTier::Starter => 29.00,
            OrganizationTier::Pro => 99.00,
            OrganizationTier::Enterprise => 299.00,
            OrganizationTier::Custom => {
                // Dalam tier custom, biaya dihitung berdasarkan penggunaan aktual
                self.calculate_custom_tier_fee(usage)
            }
        }
    }
    
    fn calculate_custom_tier_fee(&self, usage: &OrganizationUsage) -> f64 {
        // Dalam implementasi nyata, ini akan menghitung biaya berdasarkan
        // kontrak khusus organisasi
        0.0
    }
    
    pub fn can_change_tier(&self, new_tier: OrganizationTier) -> bool {
        // Dalam implementasi nyata, ini akan memeriksa apakah organisasi
        // dapat mengganti tier berdasarkan kebijakan penagihan
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_organization_creation() {
        let org = Organization::new(
            "Test Organization".to_string(),
            "billing@test.com".to_string(),
            OrganizationTier::Pro,
        );
        
        assert!(!org.id.is_nil());
        assert_eq!(org.name, "Test Organization");
        assert_eq!(org.billing_email, "billing@test.com");
        assert_eq!(org.tier, OrganizationTier::Pro);
        assert!(org.created_at <= Utc::now());
    }
    
    #[test]
    fn test_organization_validation_success() {
        let org = Organization::new(
            "Valid Org Name".to_string(),
            "valid@email.com".to_string(),
            OrganizationTier::Starter,
        );
        
        assert!(org.validate().is_ok());
    }
    
    #[test]
    fn test_organization_validation_invalid_email() {
        let org = Organization::new(
            "Valid Org Name".to_string(),
            "invalid-email".to_string(), // Email tidak valid
            OrganizationTier::Starter,
        );
        
        assert!(org.validate().is_err());
    }
    
    #[test]
    fn test_organization_validation_empty_name() {
        let org = Organization::new(
            "".to_string(), // Nama kosong
            "valid@email.com".to_string(),
            OrganizationTier::Starter,
        );
        
        assert!(org.validate().is_err());
    }
    
    #[test]
    fn test_usage_limits_by_tier() {
        let starter_org = Organization::new(
            "Starter Org".to_string(),
            "starter@test.com".to_string(),
            OrganizationTier::Starter,
        );
        
        let pro_org = Organization::new(
            "Pro Org".to_string(),
            "pro@test.com".to_string(),
            OrganizationTier::Pro,
        );
        
        let enterprise_org = Organization::new(
            "Enterprise Org".to_string(),
            "enterprise@test.com".to_string(),
            OrganizationTier::Enterprise,
        );
        
        let starter_limits = starter_org.get_usage_limits();
        assert_eq!(starter_limits.max_projects, 1);
        assert_eq!(starter_limits.max_workers, 5);
        assert_eq!(starter_limits.max_jobs_per_month, 10_000);
        
        let pro_limits = pro_org.get_usage_limits();
        assert_eq!(pro_limits.max_projects, 5);
        assert_eq!(pro_limits.max_workers, 20);
        assert_eq!(pro_limits.max_jobs_per_month, 100_000);
        
        let enterprise_limits = enterprise_org.get_usage_limits();
        assert_eq!(enterprise_limits.max_projects, 100);
        assert_eq!(enterprise_limits.max_workers, 100);
        assert_eq!(enterprise_limits.max_jobs_per_month, 1_000_000);
    }
    
    #[test]
    fn test_organization_quota_check() {
        let org = Organization::new(
            "Test Org".to_string(),
            "test@example.com".to_string(),
            OrganizationTier::Pro,
        );
        
        let limits = org.get_usage_limits();
        let mut usage = CurrentUsage {
            projects: 0,
            workers: 0,
            queues: 0,
            jobs_this_month: 0,
            api_calls_this_month: 0,
            storage_used_gb: 0.0,
            data_transfer_used_gb: 0.0,
        };
        
        // Tambahkan penggunaan yang masih dalam batas
        usage.projects = 3; // Dalam batas (max 5)
        usage.jobs_this_month = 50_000; // Dalam batas (max 100_000)
        
        assert!(org.is_usage_within_limits(&usage));
        
        // Tambahkan penggunaan yang melebihi batas
        usage.jobs_this_month = 150_000; // Melebihi batas (max 100_000)
        
        assert!(!org.is_usage_within_limits(&usage));
    }
    
    #[test]
    fn test_remaining_quota_calculation() {
        let org = Organization::new(
            "Test Org".to_string(),
            "test@example.com".to_string(),
            OrganizationTier::Pro,
        );
        
        let limits = org.get_usage_limits();
        let usage = CurrentUsage {
            projects: 2,
            workers: 10,
            queues: 25,
            jobs_this_month: 50_000,
            api_calls_this_month: 500_000,
            storage_used_gb: 50.0,
            data_transfer_used_gb: 500.0,
        };
        
        let remaining = org.get_remaining_quota(&usage);
        
        assert_eq!(remaining.remaining_projects, limits.max_projects - usage.projects);
        assert_eq!(remaining.remaining_workers, limits.max_workers - usage.workers);
        assert_eq!(remaining.remaining_queues, limits.max_queues - usage.queues);
        assert_eq!(remaining.remaining_jobs_this_month, limits.max_jobs_per_month - usage.jobs_this_month);
        assert_eq!(remaining.remaining_api_calls_this_month, limits.max_api_calls_per_month - usage.api_calls_this_month);
    }
    
    #[test]
    fn test_subscription_creation() {
        let sub = Subscription::new(
            Uuid::new_v4(),
            OrganizationTier::Enterprise,
            BillingCycle::Monthly,
            true,
        );
        
        assert!(!sub.id.is_nil());
        assert_eq!(sub.tier, OrganizationTier::Enterprise);
        assert_eq!(sub.billing_cycle, BillingCycle::Monthly);
        assert!(sub.is_active()); // Karena baru dibuat, seharusnya aktif
        assert!(sub.auto_renew);
    }
    
    #[test]
    fn test_subscription_expiry() {
        let mut sub = Subscription::new(
            Uuid::new_v4(),
            OrganizationTier::Pro,
            BillingCycle::Monthly,
            true,
        );
        
        // Set next billing date ke masa lalu
        sub.next_billing_date = Utc::now() - chrono::Duration::days(1);
        
        assert!(!sub.is_active());
    }
}