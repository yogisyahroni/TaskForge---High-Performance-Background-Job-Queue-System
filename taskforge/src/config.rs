use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use secrecy::{Secret, SecretString};
use std::net::SocketAddr;
use url::Url;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub application: ApplicationSettings,
    pub redis: RedisSettings,
    pub authentication: AuthenticationSettings,
    pub jwt: JwtSettings,
    pub metrics: MetricsSettings,
    pub logging: LoggingSettings,
    pub security: SecuritySettings,
    pub workers: WorkerSettings,
    pub queues: QueueSettings,
    pub monitoring: MonitoringSettings,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: Secret<String>,
    pub port: u16,
    pub host: String,
    pub database_name: String,
    pub require_ssl: bool,
    pub max_connections: u32,
    pub connection_timeout_seconds: u64,
}

impl DatabaseSettings {
    pub fn connection_string(&self) -> Secret<String> {
        Secret::new(format!(
            "postgresql://{}:{}@{}:{}/{}",
            self.username, 
            self.password.expose_secret(), 
            self.host, 
            self.port, 
            self.database_name
        ))
    }

    pub fn connection_string_without_db(&self) -> Secret<String> {
        Secret::new(format!(
            "postgresql://{}:{}@{}:{}/postgres",
            self.username, 
            self.password.expose_secret(), 
            self.host, 
            self.port
        ))
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApplicationSettings {
    pub host: String,
    pub port: u16,
    pub environment: Environment,
    pub cors_origin: String,
    pub base_url: String,
    pub max_request_size_mb: u32,
    pub request_timeout_seconds: u64,
    pub graceful_shutdown_seconds: u64,
}

impl ApplicationSettings {
    pub fn base_url(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn socket_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("Invalid socket address")
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisSettings {
    pub url: String,
    pub max_connections: u32,
    pub connection_timeout_seconds: u64,
    pub default_ttl_seconds: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthenticationSettings {
    pub jwt_secret: Secret<String>,
    pub jwt_expiry_time_minutes: i64,
    pub refresh_secret: Secret<String>,
    pub refresh_expiry_time_minutes: i64,
    pub password_min_length: u8,
    pub password_require_uppercase: bool,
    pub password_require_lowercase: bool,
    pub password_require_numbers: bool,
    pub password_require_symbols: bool,
    pub rate_limit_requests_per_minute: u64,
    pub max_login_attempts: u32,
    pub login_attempt_reset_window_minutes: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct JwtSettings {
    pub secret: Secret<String>,
    pub expiry_time_minutes: i64,
    pub refresh_secret: Secret<String>,
    pub refresh_expiry_time_minutes: i64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MetricsSettings {
    pub enabled: bool,
    pub endpoint: String,
    pub prometheus_enabled: bool,
    pub prometheus_endpoint: String,
    pub collection_interval_seconds: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingSettings {
    pub level: String,
    pub format: String, // json atau plain
    pub enable_tracing: bool,
    pub max_file_size_mb: u64,
    pub max_files: u32,
    pub output_file: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SecuritySettings {
    pub rate_limit_requests_per_minute: u64,
    pub enable_hsts: bool,
    pub enable_csp: bool,
    pub cors_allow_credentials: bool,
    pub cors_max_age: Option<u64>,
    pub encryption_keys_location: String,
    pub enable_encryption_at_rest: bool,
    pub enable_rate_limiting: bool,
    pub max_request_size_mb: u32,
    pub enable_ip_whitelist: bool,
    pub trusted_proxies: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WorkerSettings {
    pub default_max_concurrent_jobs: u32,
    pub heartbeat_interval_seconds: u64,
    pub heartbeat_timeout_seconds: u64,
    pub drain_mode_timeout_seconds: u64,
    pub worker_registration_timeout_seconds: u64,
    pub default_polling_interval_ms: u64,
    pub max_polling_interval_ms: u64,
    pub enable_auto_scaling: bool,
    pub scaling_check_interval_seconds: u64,
    pub scaling_up_threshold: u32,
    pub scaling_down_threshold: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct QueueSettings {
    pub default_max_retries: u32,
    pub default_timeout_seconds: u64,
    pub default_retry_delay_seconds: u32,
    pub default_max_retry_delay_seconds: u32,
    pub default_backoff_multiplier: f64,
    pub enable_dead_letter_queue: bool,
    pub default_rate_limit_per_minute: Option<u32>,
    pub max_pending_jobs_before_pause: Option<u32>,
    pub enable_priority_queues: bool,
    pub enable_scheduled_jobs: bool,
    pub enable_job_dependencies: bool,
    pub default_retention_days: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MonitoringSettings {
    pub enable_health_checks: bool,
    pub health_check_interval_seconds: u64,
    pub enable_metrics: bool,
    pub metrics_collection_interval_seconds: u64,
    pub enable_alerting: bool,
    pub alerting_webhook_url: Option<String>,
    pub enable_audit_logging: bool,
    pub audit_log_retention_days: u32,
    pub enable_performance_monitoring: bool,
    pub performance_sampling_rate: f64, // 0.0 to 1.0
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Local,
    Development,
    Staging,
    Production,
}

impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Development => "development",
            Environment::Staging => "staging",
            Environment::Production => "production",
        }
    }
}

impl Default for Environment {
    fn default() -> Self {
        Environment::Local
    }
}

pub fn get_settings() -> Result<Settings, ConfigError> {
    let mut settings = Config::default();

    // Dapatkan environment dari variabel lingkungan atau default ke local
    let environment = std::env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into());

    // Baca file konfigurasi berdasarkan environment
    let environment_filename = format!("configuration/{}.yaml", environment);
    
    // Coba baca file konfigurasi environment
    settings.merge(File::with_name(&environment_filename).required(false))?;

    // Baca file .env jika ada
    settings.merge(File::with_name(".env").required(false))?;

    // Baca dari environment variables (dengan prefix APP_)
    settings.merge(
        Environment::with_prefix("APP")
            .separator("__")
            .try_parsing(true)
            .ignore_empty(true),
    )?;

    settings.try_deserialize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    
    #[test]
    fn test_get_settings_with_defaults() {
        // Dalam pengujian, kita tidak ingin membaca file konfigurasi aktual
        // jadi kita akan menggunakan konfigurasi in-memory
        
        let settings = Config::builder()
            .set_default("database.host", "localhost")?
            .set_default("database.port", 5432)?
            .set_default("database.username", "postgres")?
            .set_default("database.password", "password")?
            .set_default("database.database_name", "taskforge")?
            .set_default("database.require_ssl", false)?
            .set_default("application.host", "127.0.0.1")?
            .set_default("application.port", 8000)?
            .set_default("application.environment", "local")?
            .set_default("application.cors_origin", "*")?
            .set_default("application.base_url", "http://localhost:8000")?
            .set_default("application.max_request_size_mb", 10)?
            .set_default("application.request_timeout_seconds", 30)?
            .set_default("application.graceful_shutdown_seconds", 30)?
            .set_default("redis.url", "redis://localhost:6379")?
            .set_default("redis.max_connections", 10)?
            .set_default("redis.connection_timeout_seconds", 5)?
            .set_default("redis.default_ttl_seconds", 3600)?
            .set_default("authentication.jwt_secret", "test_secret")?
            .set_default("authentication.jwt_expiry_time_minutes", 60)?
            .set_default("authentication.refresh_secret", "test_refresh_secret")?
            .set_default("authentication.refresh_expiry_time_minutes", 10080)?
            .set_default("authentication.password_min_length", 8)?
            .set_default("authentication.password_require_uppercase", true)?
            .set_default("authentication.password_require_lowercase", true)?
            .set_default("authentication.password_require_numbers", true)?
            .set_default("authentication.password_require_symbols", false)?
            .set_default("authentication.rate_limit_requests_per_minute", 1000)?
            .set_default("authentication.max_login_attempts", 5)?
            .set_default("authentication.login_attempt_reset_window_minutes", 15)?
            .set_default("jwt.secret", "test_secret")?
            .set_default("jwt.expiry_time_minutes", 60)?
            .set_default("jwt.refresh_secret", "test_refresh_secret")?
            .set_default("jwt.refresh_expiry_time_minutes", 10080)?
            .set_default("metrics.enabled", true)?
            .set_default("metrics.endpoint", "/metrics")?
            .set_default("metrics.prometheus_enabled", true)?
            .set_default("metrics.prometheus_endpoint", "/metrics")?
            .set_default("metrics.collection_interval_seconds", 30)?
            .set_default("logging.level", "info")?
            .set_default("logging.format", "json")?
            .set_default("logging.enable_tracing", true)?
            .set_default("logging.max_file_size_mb", 100)?
            .set_default("logging.max_files", 10)?
            .set_default("security.rate_limit_requests_per_minute", 1000)?
            .set_default("security.enable_hsts", true)?
            .set_default("security.enable_csp", true)?
            .set_default("security.cors_allow_credentials", true)?
            .set_default("security.cors_max_age", 86400)?
            .set_default("security.encryption_keys_location", "./keys")?
            .set_default("security.enable_encryption_at_rest", true)?
            .set_default("security.enable_rate_limiting", true)?
            .set_default("security.max_request_size_mb", 10)?
            .set_default("security.enable_ip_whitelist", false)?
            .set_default("security.trusted_proxies", Vec::<String>::new())?
            .set_default("workers.default_max_concurrent_jobs", 10)?
            .set_default("workers.heartbeat_interval_seconds", 30)?
            .set_default("workers.heartbeat_timeout_seconds", 90)?
            .set_default("workers.drain_mode_timeout_seconds", 300)?
            .set_default("workers.worker_registration_timeout_seconds", 60)?
            .set_default("workers.default_polling_interval_ms", 1000)?
            .set_default("workers.max_polling_interval_ms", 30000)?
            .set_default("workers.enable_auto_scaling", true)?
            .set_default("workers.scaling_check_interval_seconds", 60)?
            .set_default("workers.scaling_up_threshold", 80)?
            .set_default("workers.scaling_down_threshold", 20)?
            .set_default("queues.default_max_retries", 3)?
            .set_default("queues.default_timeout_seconds", 300)?
            .set_default("queues.default_retry_delay_seconds", 1)?
            .set_default("queues.default_max_retry_delay_seconds", 300)?
            .set_default("queues.default_backoff_multiplier", 2.0)?
            .set_default("queues.enable_dead_letter_queue", true)?
            .set_default("queues.default_rate_limit_per_minute", 1000)?
            .set_default("queues.max_pending_jobs_before_pause", 10000)?
            .set_default("queues.enable_priority_queues", true)?
            .set_default("queues.enable_scheduled_jobs", true)?
            .set_default("queues.enable_job_dependencies", true)?
            .set_default("queues.default_retention_days", 30)?
            .set_default("monitoring.enable_health_checks", true)?
            .set_default("monitoring.health_check_interval_seconds", 30)?
            .set_default("monitoring.enable_metrics", true)?
            .set_default("monitoring.metrics_collection_interval_seconds", 15)?
            .set_default("monitoring.enable_alerting", true)?
            .set_default("monitoring.alerting_webhook_url", Option::<String>::None)?
            .set_default("monitoring.enable_audit_logging", true)?
            .set_default("monitoring.audit_log_retention_days", 90)?
            .set_default("monitoring.enable_performance_monitoring", true)?
            .set_default("monitoring.performance_sampling_rate", 1.0)?
            .build()?;
        
        let settings: Settings = settings.try_deserialize()?;
        
        assert_eq!(settings.database.host, "localhost");
        assert_eq!(settings.database.port, 5432);
        assert_eq!(settings.application.port, 8000);
        assert_eq!(settings.application.environment, Environment::Local);
        assert!(settings.metrics.enabled);
        assert!(settings.security.enable_rate_limiting);
        
        Ok(())
    }
    
    #[test]
    fn test_database_connection_string() {
        let db_settings = DatabaseSettings {
            username: "test_user".to_string(),
            password: Secret::new("test_password".to_string()),
            port: 5432,
            host: "localhost".to_string(),
            database_name: "test_db".to_string(),
            require_ssl: false,
            max_connections: 10,
            connection_timeout_seconds: 30,
        };
        
        let conn_string = db_settings.connection_string();
        let exposed = conn_string.expose_secret();
        
        assert!(exposed.contains("test_user"));
        assert!(exposed.contains("localhost"));
        assert!(exposed.contains("5432"));
        assert!(exposed.contains("test_db"));
        assert!(exposed.contains("test_password")); // Ini hanya untuk testing, dalam produksi tidak akan dicek
    }
    
    #[test]
    fn test_application_socket_addr() {
        let app_settings = ApplicationSettings {
            host: "127.0.0.1".to_string(),
            port: 8080,
            environment: Environment::Local,
            cors_origin: "*".to_string(),
            base_url: "http://localhost:8080".to_string(),
            max_request_size_mb: 10,
            request_timeout_seconds: 30,
            graceful_shutdown_seconds: 30,
        };
        
        let socket_addr = app_settings.socket_addr();
        assert_eq!(socket_addr.port(), 8080);
        assert_eq!(socket_addr.ip().to_string(), "127.0.0.1");
    }
}