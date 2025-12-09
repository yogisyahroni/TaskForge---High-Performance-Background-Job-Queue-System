pub mod logging_config;
pub mod app_config;

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
    pub jwt: JwtSettings,
    pub logging: logging_config::LoggingConfig,
    pub monitoring: logging_config::MonitoringConfig,
    pub security: SecuritySettings,
    pub workers: WorkerSettings,
    pub queues: QueueSettings,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: Secret<String>,
    pub port: u16,
    pub host: String,
    pub database_name: String,
    pub require_ssl: bool,
}

impl DatabaseSettings {
    pub fn connection_string(&self) -> Secret<String> {
        Secret::new(format!(
            "postgres://{}:{}@{}:{}/{}",
            self.username, 
            self.password.expose_secret(), 
            self.host, 
            self.port, 
            self.database_name
        ))
    }

    pub fn connection_string_without_db(&self) -> Secret<String> {
        Secret::new(format!(
            "postgres://{}:{}@{}:{}/postgres",
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
    pub base_url: String,
    pub cors_origins: Vec<String>,
    pub max_request_size_mb: u32,
    pub request_timeout_seconds: u64,
}

impl ApplicationSettings {
    pub fn socket_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("Could not parse socket address")
    }
    
    pub fn base_url(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisSettings {
    pub url: String,
    pub max_connections: u32,
    pub connection_timeout_seconds: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct JwtSettings {
    pub secret: Secret<String>,
    pub expiry_time_minutes: i64,
    pub refresh_secret: Secret<String>,
    pub refresh_expiry_time_minutes: i64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SecuritySettings {
    pub rate_limit_requests_per_minute: u64,
    pub enable_hsts: bool,
    pub enable_csp: bool,
    pub cors_allow_credentials: bool,
    pub cors_max_age: Option<u64>,
    pub encryption_keys_location: String, // Lokasi untuk menyimpan kunci enkripsi
    pub enable_encryption_at_rest: bool,
    pub encryption_algorithm: String, // "AES-256-GCM", "ChaCha20-Poly1305"
}

#[derive(Debug, Deserialize, Clone)]
pub struct WorkerSettings {
    pub default_max_concurrent_jobs: u32,
    pub heartbeat_interval_seconds: u64,
    pub heartbeat_timeout_seconds: u64,
    pub drain_mode_timeout_seconds: u64,
    pub worker_registration_timeout_seconds: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct QueueSettings {
    pub default_max_retries: u32,
    pub default_timeout_seconds: u64,
    pub default_retry_delay_seconds: u64,
    pub default_max_retry_delay_seconds: u64,
    pub default_backoff_multiplier: f64,
    pub enable_dead_letter_queue: bool,
    pub default_rate_limit_per_minute: Option<u32>,
    pub max_pending_jobs_before_pause: Option<u32>,
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