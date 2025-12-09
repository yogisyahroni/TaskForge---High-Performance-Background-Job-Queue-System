# Konfigurasi dan Manajemen Environment untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan sistem konfigurasi dan manajemen environment untuk aplikasi TaskForge. Sistem ini dirancang untuk mendukung berbagai environment (development, staging, production) dengan konfigurasi yang berbeda-beda.

## 2. Pendekatan Konfigurasi

TaskForge akan menggunakan pendekatan konfigurasi berlapis yang menggabungkan beberapa sumber:

1. **Environment Variables**: Sebagai sumber konfigurasi utama untuk production
2. **File Konfigurasi**: Untuk konfigurasi kompleks dan terstruktur
3. **Nilai Default**: Sebagai fallback untuk konfigurasi opsional

## 3. Struktur Konfigurasi Aplikasi

### 3.1. File Konfigurasi Utama

Kita akan menggunakan crate `config` untuk Rust yang mendukung berbagai format (JSON, YAML, TOML) dan beberapa sumber konfigurasi.

```rust
// File: src/config/settings.rs
use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;
use std::net::SocketAddr;
use secrecy::{Secret, SecretString};

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub application: ApplicationSettings,
    pub redis: RedisSettings,
    pub jwt: JwtSettings,
    pub metrics: MetricsSettings,
    pub logging: LoggingSettings,
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
            self.username, self.password.expose_secret(), self.host, self.port, self.database_name
        ))
    }

    pub fn connection_string_without_db(&self) -> Secret<String> {
        Secret::new(format!(
            "postgres://{}:{}@{}:{}/postgres",
            self.username, self.password.expose_secret(), self.host, self.port
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
}

impl ApplicationSettings {
    pub fn base_url(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn socket_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port).parse().expect("Invalid socket address")
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisSettings {
    pub url: String,
    pub max_connections: u32,
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
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingSettings {
    pub level: String,
    pub format: String, // json atau plain
}

#[derive(Debug, Deserialize, Clone)]
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

pub fn get_configuration() -> Result<Settings, ConfigError> {
    let mut settings = Config::default();

    // Membaca file konfigurasi berdasarkan environment
    let environment = std::env::var("APP_ENVIRONMENT").unwrap_or_else(|_| "local".into());
    let environment_filename = format!("configuration/{}.yaml", environment);

    settings.merge(File::with_name(&environment_filename).required(false))?;
    
    // Membaca file .env jika ada
    settings.merge(File::with_name(".env").required(false))?;
    
    // Membaca dari environment variables (dengan prefix APP_)
    settings.merge(Environment::with_prefix("APP").separator("__"))?;

    settings.try_into()
}
```

### 3.2. File Konfigurasi untuk Berbagai Environment

#### Local/Development Environment (`configuration/local.yaml`)
```yaml
database:
  username: "postgres"
  password: "password"
  port: 5432
  host: "localhost"
  database_name: "taskforge_local"
  require_ssl: false

application:
  host: "127.0.0.1"
  port: 800
  environment: "local"
  cors_origin: "*"
  base_url: "http://localhost:8000"

redis:
  url: "redis://127.0.0.1/"
  max_connections: 10

jwt:
  secret: "local_secret_key_for_development"
  expiry_time_minutes: 60
  refresh_secret: "local_refresh_secret_key"
  refresh_expiry_time_minutes: 10080  # 7 days

metrics:
  enabled: true
  endpoint: "/metrics"

logging:
  level: "debug"
  format: "plain"
```

#### Production Environment (`configuration/production.yaml`)
```yaml
database:
  username: ${DATABASE_USERNAME}
  password: ${DATABASE_PASSWORD}
  port: ${DATABASE_PORT}
  host: ${DATABASE_HOST}
  database_name: ${DATABASE_NAME}
  require_ssl: true

application:
  host: ${APPLICATION_HOST}
  port: ${APPLICATION_PORT}
  environment: "production"
 cors_origin: ${CORS_ORIGIN}
  base_url: ${BASE_URL}

redis:
  url: ${REDIS_URL}
  max_connections: 50

jwt:
  secret: ${JWT_SECRET}
  expiry_time_minutes: ${JWT_EXPIRY_TIME_MINUTES}
  refresh_secret: ${JWT_REFRESH_SECRET}
  refresh_expiry_time_minutes: ${JWT_REFRESH_EXPIRY_TIME_MINUTES}

metrics:
  enabled: true
  endpoint: "/metrics"

logging:
  level: "info"
  format: "json"
```

## 4. Sistem Environment Variables

### 4.1. Variabel Lingkungan Penting

Berikut adalah variabel lingkungan yang harus disediakan untuk berbagai environment:

#### Database Configuration
- `DATABASE_USERNAME`: Username untuk koneksi database
- `DATABASE_PASSWORD`: Password untuk koneksi database (harus dirahasiakan)
- `DATABASE_HOST`: Host database
- `DATABASE_PORT`: Port database (default: 5432)
- `DATABASE_NAME`: Nama database
- `DATABASE_REQUIRE_SSL`: Boolean untuk mengaktifkan SSL (default: true di production)

#### Application Configuration
- `APPLICATION_HOST`: Host untuk aplikasi (default: 127.0.0.1)
- `APPLICATION_PORT`: Port untuk aplikasi (default: 8000)
- `APP_ENVIRONMENT`: Environment aplikasi (local, development, staging, production)
- `CORS_ORIGIN`: Origin untuk CORS (default: * di local)
- `BASE_URL`: Base URL untuk aplikasi

#### Redis Configuration
- `REDIS_URL`: URL koneksi Redis
- `REDIS_MAX_CONNECTIONS`: Jumlah maksimum koneksi Redis (default: 10)

#### JWT Configuration
- `JWT_SECRET`: Secret key untuk JWT
- `JWT_EXPIRY_TIME_MINUTES`: Waktu expiry JWT dalam menit
- `JWT_REFRESH_SECRET`: Secret key untuk refresh token
- `JWT_REFRESH_EXPIRY_TIME_MINUTES`: Waktu expiry refresh token dalam menit

#### Logging Configuration
- `LOG_LEVEL`: Level logging (trace, debug, info, warn, error)
- `LOG_FORMAT`: Format logging (json, plain)

### 4.2. File .env Contoh

```bash
# Database Configuration
DATABASE_USERNAME=postgres
DATABASE_PASSWORD=your_secure_password
DATABASE_HOST=localhost
DATABASE_PORT=5432
DATABASE_NAME=taskforge_dev
DATABASE_REQUIRE_SSL=false

# Application Configuration
APPLICATION_HOST=127.0.0.1
APPLICATION_PORT=8000
APP_ENVIRONMENT=development
CORS_ORIGIN=*
BASE_URL=http://localhost:8000

# Redis Configuration
REDIS_URL=redis://127.0.0.1/
REDIS_MAX_CONNECTIONS=20

# JWT Configuration
JWT_SECRET=your_very_secure_jwt_secret_key_here
JWT_EXPIRY_TIME_MINUTES=60
JWT_REFRESH_SECRET=your_very_secure_refresh_secret_key_here
JWT_REFRESH_EXPIRY_TIME_MINUTES=10080

# Logging Configuration
LOG_LEVEL=debug
LOG_FORMAT=plain

# Metrics Configuration
METRICS_ENABLED=true
METRICS_ENDPOINT=/metrics
```

## 5. Sistem Validasi Konfigurasi

### 5.1. Validasi Konfigurasi pada Startup

```rust
// File: src/config/validation.rs
use crate::config::settings::Settings;
use std::env;

pub fn validate_settings(settings: &Settings) -> Result<(), String> {
    // Validasi database settings
    validate_database_settings(&settings.database)?;
    
    // Validasi application settings
    validate_application_settings(&settings.application)?;
    
    // Validasi JWT settings
    validate_jwt_settings(&settings.jwt)?;
    
    // Validasi logging settings
    validate_logging_settings(&settings.logging)?;
    
    Ok(())
}

fn validate_database_settings(settings: &super::settings::DatabaseSettings) -> Result<(), String> {
    if settings.username.is_empty() {
        return Err("Database username cannot be empty".to_string());
    }
    
    if settings.password.expose_secret().is_empty() {
        return Err("Database password cannot be empty".to_string());
    }
    
    if settings.host.is_empty() {
        return Err("Database host cannot be empty".to_string());
    }
    
    if settings.database_name.is_empty() {
        return Err("Database name cannot be empty".to_string());
    }
    
    if settings.port == 0 || settings.port > 65535 {
        return Err("Database port must be between 1 and 65535".to_string());
    }
    
    Ok(())
}

fn validate_application_settings(settings: &super::settings::ApplicationSettings) -> Result<(), String> {
    if settings.host.is_empty() {
        return Err("Application host cannot be empty".to_string());
    }
    
    if settings.port == 0 || settings.port > 65535 {
        return Err("Application port must be between 1 and 65535".to_string());
    }
    
    if settings.environment.as_str().is_empty() {
        return Err("Application environment cannot be empty".to_string());
    }
    
    Ok(())
}

fn validate_jwt_settings(settings: &super::settings::JwtSettings) -> Result<(), String> {
    if settings.secret.expose_secret().len() < 32 {
        return Err("JWT secret must be at least 32 characters long".to_string());
    }
    
    if settings.refresh_secret.expose_secret().len() < 32 {
        return Err("JWT refresh secret must be at least 32 characters long".to_string());
    }
    
    if settings.expiry_time_minutes <= 0 {
        return Err("JWT expiry time must be positive".to_string());
    }
    
    if settings.refresh_expiry_time_minutes <= 0 {
        return Err("JWT refresh expiry time must be positive".to_string());
    }
    
    Ok(())
}

fn validate_logging_settings(settings: &super::settings::LoggingSettings) -> Result<(), String> {
    let valid_levels = ["trace", "debug", "info", "warn", "error"];
    if !valid_levels.contains(&settings.level.as_str()) {
        return Err(format!("Invalid log level: {}. Must be one of {:?}", settings.level, valid_levels));
    }
    
    let valid_formats = ["json", "plain"];
    if !valid_formats.contains(&settings.format.as_str()) {
        return Err(format!("Invalid log format: {}. Must be one of {:?}", settings.format, valid_formats));
    }
    
    Ok(())
}
```

## 6. Sistem Konfigurasi Runtime

### 6.1. Fitur Konfigurasi yang Dapat Diubah Runtime

Beberapa konfigurasi mungkin perlu diubah tanpa restart aplikasi:

- Logging level
- Feature flags
- Rate limiting thresholds
- Metrik collection settings

```rust
// File: src/config/runtime_config.rs
use std::sync::RwLock;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub feature_flags: RwLock<HashMap<String, bool>>,
    pub rate_limits: RwLock<HashMap<String, u32>>,
    pub log_levels: RwLock<HashMap<String, String>>,
}

impl RuntimeConfig {
    pub fn new() -> Self {
        Self {
            feature_flags: RwLock::new(HashMap::new()),
            rate_limits: RwLock::new(HashMap::new()),
            log_levels: RwLock::new(HashMap::new()),
        }
    }
    
    pub fn is_feature_enabled(&self, feature_name: &str) -> bool {
        let flags = self.feature_flags.read().unwrap();
        *flags.get(feature_name).unwrap_or(&false)
    }
    
    pub fn set_feature_flag(&self, feature_name: String, enabled: bool) {
        let mut flags = self.feature_flags.write().unwrap();
        flags.insert(feature_name, enabled);
    }
    
    pub fn get_rate_limit(&self, endpoint: &str) -> u32 {
        let limits = self.rate_limits.read().unwrap();
        *limits.get(endpoint).unwrap_or(&100) // default 10 requests per minute
    }
    
    pub fn set_rate_limit(&self, endpoint: String, limit: u32) {
        let mut limits = self.rate_limits.write().unwrap();
        limits.insert(endpoint, limit);
    }
}
```

## 7. Konfigurasi untuk Deployment

### 7.1. Docker Environment

Saat menggunakan Docker, konfigurasi akan dilewatkan melalui environment variables atau volume mounts:

```dockerfile
# Dockerfile
FROM rust:1.70 as builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /app/target/release/taskforge /usr/local/bin/
COPY configuration/ /etc/taskforge/
EXPOSE 8000
CMD ["taskforge"]
```

```yaml
# docker-compose.yml
version: '3.8'

services:
  taskforge:
    build: .
    ports:
      - "8000:8000"
    environment:
      - APP_ENVIRONMENT=development
      - DATABASE_HOST=taskforge-db
      - DATABASE_PORT=5432
      - DATABASE_NAME=taskforge
      - DATABASE_USERNAME=postgres
      - DATABASE_PASSWORD=secretpassword
      - REDIS_URL=redis://taskforge-redis:6379
    depends_on:
      - db
      - redis

  db:
    image: postgres:14
    environment:
      - POSTGRES_DB=taskforge
      - POSTGRES_USER=postgres
      - POSTGRES_PASSWORD=secretpassword
    ports:
      - "5432:5432"
    volumes:
      - postgres_data:/var/lib/postgresql/data

 redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"

volumes:
  postgres_data:
```

### 7.2. Kubernetes Environment

Untuk deployment di Kubernetes, konfigurasi akan menggunakan ConfigMap dan Secret:

```yaml
# k8s/configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: taskforge-config
data:
  APP__APPLICATION__HOST: "0.0.0.0"
  APP__APPLICATION__PORT: "8000"
 APP__METRICS__ENABLED: "true"
  APP__LOGGING__LEVEL: "info"
```

```yaml
# k8s/secret.yaml
apiVersion: v1
kind: Secret
metadata:
  name: taskforge-secrets
type: Opaque
data:
  DATABASE_PASSWORD: <base64_encoded_password>
  JWT_SECRET: <base64_encoded_secret>
  JWT_REFRESH_SECRET: <base64_encoded_refresh_secret>
```

## 8. Best Practices

### 8.1. Prinsip Konfigurasi

1. **Jangan hardcode nilai** dalam kode sumber
2. **Gunakan environment variables** untuk konfigurasi sensitif
3. **Validasi semua konfigurasi** sebelum aplikasi dimulai
4. **Gunakan default yang aman** untuk konfigurasi opsional
5. **Dokumentasikan semua opsi konfigurasi** dengan jelas

### 8.2. Keamanan Konfigurasi

1. **Jangan log nilai konfigurasi sensitif**
2. **Gunakan tipe Secret** untuk password dan token
3. **Gunakan konfigurasi terenkripsi** di production
4. **Rotasi secret secara berkala**

### 8.3. Manajemen Environment

1. **Gunakan environment yang konsisten** antara development dan production
2. **Gunakan CI/CD untuk mengelola konfigurasi** berbeda per environment
3. **Gunakan feature flags** untuk mengontrol fitur per environment
4. **Gunakan konfigurasi versi kontrol** untuk konfigurasi non-sensitif