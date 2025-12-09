use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String, // "json" or "plain"
    pub output: String, // "stdout", "stderr", or file path
    pub enable_tracing: bool,
    pub max_file_size_mb: u64,
    pub max_files: u32,
    pub log_slow_queries: bool,
    pub slow_query_threshold_ms: u64,
}

impl LoggingConfig {
    pub fn from_env() -> Self {
        Self {
            level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            format: env::var("LOG_FORMAT").unwrap_or_else(|_| "json".to_string()),
            output: env::var("LOG_OUTPUT").unwrap_or_else(|_| "stdout".to_string()),
            enable_tracing: env::var("ENABLE_TRACING")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            max_file_size_mb: env::var("LOG_MAX_FILE_SIZE_MB")
                .unwrap_or_else(|_| "100".to_string())
                .parse()
                .unwrap_or(100),
            max_files: env::var("LOG_MAX_FILES")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap_or(10),
            log_slow_queries: env::var("LOG_SLOW_QUERIES")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            slow_query_threshold_ms: env::var("SLOW_QUERY_THRESHOLD_MS")
                .unwrap_or_else(|_| "100".to_string())
                .parse()
                .unwrap_or(100),
        }
    }
    
    pub fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "json".to_string(),
            output: "stdout".to_string(),
            enable_tracing: true,
            max_file_size_mb: 100,
            max_files: 10,
            log_slow_queries: true,
            slow_query_threshold_ms: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub enable_metrics: bool,
    pub metrics_port: u16,
    pub enable_health_checks: bool,
    pub health_check_interval_seconds: u64,
    pub enable_prometheus: bool,
    pub prometheus_endpoint: String,
    pub enable_profiling: bool,
    pub profile_output_path: String,
    pub enable_apm: bool, // Application Performance Monitoring
    pub apm_endpoint: Option<String>,
    pub apm_token: Option<String>,
}

impl MonitoringConfig {
    pub fn from_env() -> Self {
        Self {
            enable_metrics: env::var("METRICS_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            metrics_port: env::var("METRICS_PORT")
                .unwrap_or_else(|_| "9000".to_string())
                .parse()
                .unwrap_or(9000),
            enable_health_checks: env::var("HEALTH_CHECKS_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            health_check_interval_seconds: env::var("HEALTH_CHECK_INTERVAL_SECONDS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            enable_prometheus: env::var("PROMETHEUS_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            prometheus_endpoint: env::var("PROMETHEUS_ENDPOINT")
                .unwrap_or_else(|_| "/metrics".to_string()),
            enable_profiling: env::var("PROFILING_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            profile_output_path: env::var("PROFILE_OUTPUT_PATH")
                .unwrap_or_else(|_| "./profiles".to_string()),
            enable_apm: env::var("APM_ENABLED")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            apm_endpoint: env::var("APM_ENDPOINT").ok(),
            apm_token: env::var("APM_TOKEN").ok(),
        }
    }
    
    pub fn default() -> Self {
        Self {
            enable_metrics: true,
            metrics_port: 9000,
            enable_health_checks: true,
            health_check_interval_seconds: 30,
            enable_prometheus: true,
            prometheus_endpoint: "/metrics".to_string(),
            enable_profiling: false,
            profile_output_path: "./profiles".to_string(),
            enable_apm: false,
            apm_endpoint: None,
            apm_token: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityLoggingConfig {
    pub log_authentication_events: bool,
    pub log_authorization_events: bool,
    pub log_data_access: bool,
    pub log_configuration_changes: bool,
    pub enable_audit_logging: bool,
    pub audit_log_retention_days: u32,
    pub log_pii_data: bool, // Whether to log personally identifiable information
    pub mask_sensitive_fields: Vec<String>,
}

impl SecurityLoggingConfig {
    pub fn from_env() -> Self {
        Self {
            log_authentication_events: env::var("LOG_AUTH_EVENTS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            log_authorization_events: env::var("LOG_AUTHZ_EVENTS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            log_data_access: env::var("LOG_DATA_ACCESS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            log_configuration_changes: env::var("LOG_CONFIG_CHANGES")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            enable_audit_logging: env::var("AUDIT_LOGGING_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
            audit_log_retention_days: env::var("AUDIT_LOG_RETENTION_DAYS")
                .unwrap_or_else(|_| "90".to_string())
                .parse()
                .unwrap_or(90),
            log_pii_data: env::var("LOG_PII_DATA")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            mask_sensitive_fields: env::var("MASK_SENSITIVE_FIELDS")
                .unwrap_or_else(|_| "password,token,secret,api_key".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
        }
    }
    
    pub fn default() -> Self {
        Self {
            log_authentication_events: true,
            log_authorization_events: true,
            log_data_access: true,
            log_configuration_changes: true,
            enable_audit_logging: true,
            audit_log_retention_days: 90,
            log_pii_data: false,
            mask_sensitive_fields: vec![
                "password".to_string(),
                "token".to_string(),
                "secret".to_string(),
                "api_key".to_string(),
                "credit_card".to_string(),
                "ssn".to_string(),
            ],
        }
    }
}