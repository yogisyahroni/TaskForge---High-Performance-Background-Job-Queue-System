# Deployment Guide dan Production Checklist untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menyediakan panduan lengkap untuk melakukan deployment TaskForge ke lingkungan production. Ini mencakup persiapan, prosedur deployment, dan checklist verifikasi yang diperlukan untuk memastikan deployment yang sukses dan aman.

## 2. Prasyarat Deployment

### 2.1. Infrastruktur Minimum

**Server Requirements:**
- **CPU**: 4 cores atau lebih (disarankan 8 cores untuk beban tinggi)
- **RAM**: 8 GB atau lebih (disarankan 16 GB untuk beban tinggi)
- **Storage**: 50 GB SSD atau lebih (bergantung pada volume data)
- **OS**: Ubuntu 20.04 LTS atau lebih baru, CentOS 8 atau lebih baru
- **Jaringan**: Konektivitas internet yang stabil

**Layanan Eksternal:**
- PostgreSQL 13+ (disarankan 14+)
- Redis 6+
- TLS/SSL certificate (jika menggunakan HTTPS)
- Load balancer (opsional tapi disarankan)

### 2.2. Persiapan Lingkungan

Sebelum deployment, pastikan lingkungan telah siap:

```bash
# Cek versi sistem
uname -a
free -h
df -h
lscpu

# Verifikasi dependensi
docker --version
docker-compose --version
rustc --version
cargo --version
```

## 3. Konfigurasi Environment

### 3.1. File Environment (.env)

```env
# File: .env.production
# Database Configuration
DATABASE_URL=postgresql://taskforge:password@db:5432/taskforge_production
DB_HOST=db
DB_PORT=5432
DB_NAME=taskforge_production
DB_USER=taskforge
DB_PASSWORD=your_secure_password

# Redis Configuration
REDIS_URL=redis://:password@redis:6379/
REDIS_HOST=redis
REDIS_PORT=6379
REDIS_PASSWORD=your_redis_password

# Application Configuration
APP_ENVIRONMENT=production
APPLICATION_HOST=0.0.0.0
APPLICATION_PORT=8000
BASE_URL=https://api.yourdomain.com
CORS_ALLOWED_ORIGINS=https://yourdomain.com,https://app.yourdomain.com

# Security Configuration
JWT_SECRET=your_super_secret_jwt_key_here_must_be_at_least_32_chars_long
JWT_EXPIRY_TIME_MINUTES=60
JWT_REFRESH_SECRET=your_refresh_secret_key_at_least_32_chars
JWT_REFRESH_EXPIRY_TIME_MINUTES=10080

# Logging Configuration
LOG_LEVEL=info
LOG_FORMAT=json
LOG_FILE_PATH=/var/log/taskforge/app.log

# Metrics Configuration
METRICS_ENABLED=true
METRICS_ENDPOINT=/metrics
PROMETHEUS_PUSH_GATEWAY=

# Email Configuration (opsional)
SMTP_HOST=smtp.gmail.com
SMTP_PORT=587
SMTP_USERNAME=your_smtp_username
SMTP_PASSWORD=your_smtp_password
EMAIL_FROM=noreply@yourdomain.com

# Payment Gateway Configuration (jika digunakan)
STRIPE_SECRET_KEY=sk_live_your_stripe_secret_key
STRIPE_WEBHOOK_SECRET=whsec_your_webhook_secret

# Monitoring Configuration
SENTRY_DSN=your_sentry_dsn_here
HEALTH_CHECK_TIMEOUT_SECONDS=30

# Security Headers
SECURITY_HSTS=true
SECURITY_CSP=default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline';
```

### 3.2. Konfigurasi Produksi Docker Compose

```yaml
# File: docker-compose.prod.yml
version: '3.8'

services:
  # Database Production
  db:
    image: postgres:14-alpine
    container_name: taskforge_db_prod
    environment:
      POSTGRES_DB: ${DB_NAME}
      POSTGRES_USER: ${DB_USER}
      POSTGRES_PASSWORD: ${DB_PASSWORD}
      PGDATA: /var/lib/postgresql/data/pgdata
    volumes:
      - taskforge_postgres_data:/var/lib/postgresql/data
      - ./init-scripts:/docker-entrypoint-initdb.d
      - ./backups:/backups
    ports:
      - "5432:5432"
    networks:
      - taskforge_prod
    restart: unless-stopped
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U ${DB_USER} -d ${DB_NAME}"]
      interval: 30s
      timeout: 10s
      retries: 5
      start_period: 60s
    deploy:
      resources:
        limits:
          cpus: '${DB_CPUS:-2.0}'
          memory: '${DB_MEMORY:-4G}'
        reservations:
          cpus: '${DB_CPUS_RESERVE:-1.0}'
          memory: '${DB_MEMORY_RESERVE:-2G}'

  # Redis Production
  redis:
    image: redis:7-alpine
    container_name: taskforge_redis_prod
    command: >
      redis-server
      --requirepass ${REDIS_PASSWORD}
      --maxmemory ${REDIS_MAXMEMORY:-1gb}
      --maxmemory-policy allkeys-lru
      --save 900 1
      --save 300 10
      --save 60 10000
      --appendonly yes
    volumes:
      - taskforge_redis_data:/data
      - ./redis.conf:/usr/local/etc/redis/redis.conf
    ports:
      - "6379:6379"
    networks:
      - taskforge_prod
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 30s
    deploy:
      resources:
        limits:
          cpus: '${REDIS_CPUS:-1.0}'
          memory: '${REDIS_MEMORY:-1G}'
        reservations:
          cpus: '${REDIS_CPUS_RESERVE:-0.25}'
          memory: '${REDIS_MEMORY_RESERVE:-256M}'

  # TaskForge Application (Production)
  app:
    build:
      context: .
      dockerfile: Dockerfile.prod
    container_name: taskforge_app_prod
    depends_on:
      db:
        condition: service_healthy
      redis:
        condition: service_healthy
    environment:
      # Environment variables from .env.production
      - DATABASE_URL=${DATABASE_URL}
      - DB_HOST=${DB_HOST}
      - DB_PORT=${DB_PORT}
      - DB_NAME=${DB_NAME}
      - DB_USER=${DB_USER}
      - DB_PASSWORD=${DB_PASSWORD}
      - REDIS_URL=${REDIS_URL}
      - REDIS_HOST=${REDIS_HOST}
      - REDIS_PORT=${REDIS_PORT}
      - REDIS_PASSWORD=${REDIS_PASSWORD}
      - APP_ENVIRONMENT=${APP_ENVIRONMENT}
      - APPLICATION_HOST=${APPLICATION_HOST}
      - APPLICATION_PORT=${APPLICATION_PORT}
      - BASE_URL=${BASE_URL}
      - CORS_ALLOWED_ORIGINS=${CORS_ALLOWED_ORIGINS}
      - JWT_SECRET=${JWT_SECRET}
      - JWT_EXPIRY_TIME_MINUTES=${JWT_EXPIRY_TIME_MINUTES}
      - JWT_REFRESH_SECRET=${JWT_REFRESH_SECRET}
      - JWT_REFRESH_EXPIRY_TIME_MINUTES=${JWT_REFRESH_EXPIRY_TIME_MINUTES}
      - LOG_LEVEL=${LOG_LEVEL}
      - LOG_FORMAT=${LOG_FORMAT}
      - METRICS_ENABLED=${METRICS_ENABLED}
      - METRICS_ENDPOINT=${METRICS_ENDPOINT}
      # Production-specific settings
      - WORKERS=${WORKERS:-4}
      - MAX_CONNECTIONS=${MAX_CONNECTIONS:-100}
      - POOL_SIZE=${POOL_SIZE:-20}
      - RUST_BACKTRACE=0  # Disable backtrace in production
    ports:
      - "8000:8000"
    volumes:
      - ./logs:/app/logs
      - ./config:/app/config
      - /var/run/docker.sock:/var/run/docker.sock:ro  # For monitoring
    networks:
      - taskforge_prod
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 60s
    deploy:
      resources:
        limits:
          cpus: '${APP_CPUS:-2.0}'
          memory: '${APP_MEMORY:-2G}'
        reservations:
          cpus: '${APP_CPUS_RESERVE:-0.5}'
          memory: '${APP_MEMORY_RESERVE:-512M}'

  # Reverse Proxy (NGINX)
  nginx:
    image: nginx:alpine
    container_name: taskforge_nginx_prod
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx/nginx.conf:/etc/nginx/nginx.conf
      - ./nginx/ssl:/etc/nginx/ssl
      - ./logs/nginx:/var/log/nginx
    networks:
      - taskforge_prod
    depends_on:
      - app
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 30s

  # Prometheus (Monitoring)
  prometheus:
    image: prom/prometheus:latest
    container_name: taskforge_prometheus_prod
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus/prometheus.yml:/etc/prometheus/prometheus.yml
      - taskforge_prometheus_data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--web.console.libraries=/etc/prometheus/console_libraries'
      - '--web.console.templates=/etc/prometheus/consoles'
      - '--storage.tsdb.retention.time=365d'
      - '--web.enable-lifecycle'
      - '--web.enable-admin-api'
    networks:
      - taskforge_prod
    restart: unless-stopped
    deploy:
      resources:
        limits:
          cpus: '${PROMETHEUS_CPUS:-1.0}'
          memory: '${PROMETHEUS_MEMORY:-1G}'
        reservations:
          cpus: '${PROMETHEUS_CPUS_RESERVE:-0.25}'
          memory: '${PROMETHEUS_MEMORY_RESERVE:-256M}'

  # Grafana (Dashboard)
  grafana:
    image: grafana/grafana-enterprise:latest
    container_name: taskforge_grafana_prod
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=${GRAFANA_ADMIN_PASSWORD}
      - GF_USERS_ALLOW_SIGN_UP=false
      - GF_SMTP_ENABLED=${GF_SMTP_ENABLED:-false}
    volumes:
      - taskforge_grafana_data:/var/lib/grafana
      - ./grafana/provisioning:/etc/grafana/provisioning
    networks:
      - taskforge_prod
    restart: unless-stopped
    depends_on:
      - prometheus

  # Backup Service
  backup:
    image: tiredofit/db-backup
    container_name: taskforge_backup
    volumes:
      - ./backups:/backup
      - /etc/localtime:/etc/localtime:ro
    environment:
      - DB_TYPE=postgres
      - DB_HOST=db
      - DB_NAME=${DB_NAME}
      - DB_USER=${DB_USER}
      - DB_PASS=${DB_PASSWORD}
      - SCHEDULE=@daily
      - BACKUP_KEEP_DAYS=7
      - BACKUP_KEEP_WEEKS=4
      - BACKUP_KEEP_MONTHS=6
      - HEALTHCHECK_PORT=8080
    networks:
      - taskforge_prod
    restart: unless-stopped

networks:
  taskforge_prod:
    driver: bridge
    attachable: true

volumes:
  taskforge_postgres_data:
    driver: local
    driver_opts:
      type: none
      o: bind
      device: /opt/taskforge/data/postgres
  taskforge_redis_data:
    driver: local
    driver_opts:
      type: none
      o: bind
      device: /opt/taskforge/data/redis
  taskforge_prometheus_data:
    driver: local
    driver_opts:
      type: none
      o: bind
      device: /opt/taskforge/data/prometheus
  taskforge_grafana_data:
    driver: local
    driver_opts:
      type: none
      o: bind
      device: /opt/taskforge/data/grafana
```

### 3.3. Dockerfile Production

```dockerfile
# File: Dockerfile.prod
# Production Dockerfile
FROM rust:1.70 as builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    musl-tools \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to allow cargo to download dependencies
RUN mkdir src
RUN echo "fn main() { println!(\"Dummy\"); }" > src/main.rs

# Download and compile dependencies
RUN cargo build --release
RUN rm src/*.rs

# Copy source code
COPY src ./src
COPY migrations ./migrations

# Build the application
RUN touch src/main.rs  # Force rebuild
RUN cargo build --release --bin taskforge

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl1.1 \
    libpq5 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN addgroup --system taskforge && adduser --system --ingroup taskforge taskforge

# Set working directory
WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /app/target/release/taskforge /usr/local/bin/taskforge

# Copy migrations
COPY --from=builder /app/migrations ./migrations

# Create necessary directories
RUN mkdir -p /app/config /app/logs
RUN chown -R taskforge:taskforge /app

# Switch to non-root user
USER taskforge

# Expose port
EXPOSE 8000

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=60s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# Command to run the application
CMD ["taskforge"]
```

## 4. Prosedur Deployment

### 4.1. Pra-Deployment Checklist

Sebelum melakukan deployment, pastikan semua item berikut telah terpenuhi:

- [ ] Kredensial database dan layanan eksternal siap
- [ ] SSL certificate valid dan terpasang
- [ ] Domain telah diarahkan ke server
- [ ] Backup terbaru dari lingkungan sebelumnya telah diambil
- [ ] Konfigurasi production telah disiapkan
- [ ] Tim operasional telah diberitahu tentang jadwal deployment
- [ ] Rollback plan telah disiapkan

### 4.2. Deployment Langkah Demi Langkah

#### Langkah 1: Persiapan Server

```bash
# SSH ke server production
ssh user@your-production-server

# Update sistem
sudo apt update && sudo apt upgrade -y

# Install Docker dan Docker Compose
sudo apt install docker.io docker-compose -y

# Tambahkan user ke grup docker
sudo usermod -aG docker $USER

# Restart Docker service
sudo systemctl restart docker

# Verifikasi instalasi
docker --version
docker-compose --version
```

#### Langkah 2: Persiapan Direktori dan File

```bash
# Buat direktori untuk aplikasi
sudo mkdir -p /opt/taskforge/{app,config,data,logs,backups}
sudo chown -R $USER:$USER /opt/taskforge

# Salin kode aplikasi (dari development/local)
scp -r /path/to/taskforge/* user@production-server:/opt/taskforge/app/

# Salin konfigurasi produksi
scp .env.production user@production-server:/opt/taskforge/config/.env
scp docker-compose.prod.yml user@production-server:/opt/taskforge/app/
```

#### Langkah 3: Konfigurasi SSL

```bash
# Buat direktori untuk SSL certificate
sudo mkdir -p /opt/taskforge/app/nginx/ssl

# Salin SSL certificate (telah diperoleh sebelumnya)
sudo cp /path/to/your/certificate.crt /opt/taskforge/app/nginx/ssl/
sudo cp /path/to/your/private.key /opt/taskforge/app/nginx/ssl/
```

#### Langkah 4: Jalankan Deployment

```bash
# Pindah ke direktori aplikasi
cd /opt/taskforge/app

# Set environment variables
export $(grep -v '^#' .env | xargs)

# Build dan start services
docker-compose -f docker-compose.prod.yml up -d --build

# Tunggu beberapa saat dan cek status
docker-compose -f docker-compose.prod.yml ps
```

#### Langkah 5: Jalankan Migrasi Database

```bash
# Tunggu database siap
sleep 60

# Jalankan migrasi database
docker-compose -f docker-compose.prod.yml exec db psql -U taskforge -d taskforge_production -c "CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\";"
docker-compose -f docker-compose.prod.yml exec app sqlx migrate run
```

## 5. Post-Deployment Verification

### 5.1. Checklist Verifikasi

Setelah deployment selesai, lakukan verifikasi berikut:

- [ ] Aplikasi dapat diakses melalui domain
- [ ] Health check endpoint merespon dengan sukses
- [ ] Database dapat diakses dan migrasi telah dijalankan
- [ ] Redis dapat diakses dan berfungsi dengan baik
- [ ] Metrics endpoint dapat diakses oleh Prometheus
- [ ] Logging berfungsi dengan baik
- [ ] SSL certificate valid
- [ ] API endpoints berfungsi sebagaimana mestinya

### 5.2. Testing Fungsional

```bash
# Cek health endpoint
curl -k https://your-domain.com/health

# Cek metrics endpoint
curl -k https://your-domain.com/metrics

# Coba submit job sederhana
curl -X POST https://your-domain.com/api/v1/jobs \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_VALID_TOKEN" \
  -d '{
    "queue_name": "default",
    "job_type": "test_job",
    "payload": {"test": "data"},
    "priority": 5
  }'
```

## 6. Production Checklist

### 6.1. Keamanan

- [ ] SSL certificate valid dan terbaru
- [ ] Firewall dikonfigurasi dengan benar
- [ ] Akses SSH dibatasi (gunakan key, bukan password)
- [ ] Akses database hanya dari aplikasi yang sah
- [ ] JWT secret dan kredensial lainnya tidak dalam kode
- [ ] Security headers dikonfigurasi di NGINX
- [ ] Rate limiting diimplementasikan
- [ ] Audit log diaktifkan

### 6.2. Monitoring dan Observasi

- [ ] Logging dalam format JSON untuk kemudahan parsing
- [ ] Metrics endpoint tersedia untuk Prometheus
- [ ] Dashboard Grafana telah dikonfigurasi
- [ ] Alerting telah disiapkan untuk kondisi kritis
- [ ] Health check endpoint berfungsi
- [ ] Database monitoring aktif
- [ ] Application performance monitoring aktif

### 6.3. Ketersediaan dan Skalabilitas

- [ ] Load balancing dikonfigurasi (jika diperlukan)
- [ ] Auto-scaling rules disiapkan
- [ ] Backup otomatis berjalan
- [ ] Recovery procedure telah diuji
- [ ] Resource limits dan requests dikonfigurasi
- [ ] Health checks untuk semua service

### 6.4. Performa

- [ ] Database indexing optimal
- [ ] Connection pooling dikonfigurasi
- [ ] Caching mechanisms aktif
- [ ] CDN (jika diperlukan) dikonfigurasi
- [ ] Database connection limits sesuai kebutuhan
- [ ] Application workers disesuaikan dengan kapasitas

## 7. Rollback Procedure

Jika deployment gagal atau menyebabkan masalah, ikuti prosedur rollback berikut:

```bash
# 1. Kembali ke versi sebelumnya
cd /opt/taskforge/app

# 2. Stop layanan saat ini
docker-compose -f docker-compose.prod.yml down

# 3. Kembali ke backup jika diperlukan
# (restore database backup)
docker exec -i taskforge_db_prod psql -U taskforge -d taskforge_production < /path/to/backup.sql

# 4. Jalankan versi sebelumnya
docker-compose -f docker-compose.prod.yml up -d
```

## 8. Maintenance Routine

### 8.1. Tugas Harian

- [ ] Cek log aplikasi untuk error
- [ ] Monitor kesehatan sistem
- [ ] Cek kapasitas disk
- [ ] Verifikasi backup otomatis

### 8.2. Tugas Mingguan

- [ ] Review metrics dan kinerja
- [ ] Update SSL certificate jika perlu
- [ ] Cek keamanan sistem
- [ ] Backup manual sebagai verifikasi

### 8.3. Tugas Bulanan

- [ ] Update sistem dan dependensi
- [ ] Review dan update konfigurasi
- [ ] Update dokumentasi jika diperlukan
- [ ] Review capacity planning

## 9. Troubleshooting Production Issues

### 9.1. Common Issues and Solutions

**Issue: Aplikasi tidak merespon**
- Cek status container: `docker-compose ps`
- Cek log: `docker-compose logs app`
- Cek koneksi database: `docker-compose exec db pg_isready`

**Issue: Database connection timeout**
- Cek konfigurasi connection pool
- Cek kapasitas database
- Cek firewall rules

**Issue: High memory usage**
- Cek konfigurasi resource limits
- Cek untuk memory leaks
- Cek jumlah concurrent jobs

**Issue: SSL certificate error**
- Verifikasi certificate validity
- Cek konfigurasi NGINX
- Pastikan certificate path benar

### 9.2. Diagnostic Commands

```bash
# Cek status semua service
docker-compose -f docker-compose.prod.yml ps

# Cek log aplikasi
docker-compose -f docker-compose.prod.yml logs app

# Cek log database
docker-compose -f docker-compose.prod.yml logs db

# Cek log NGINX
docker-compose -f docker-compose.prod.yml logs nginx

# Cek resource usage
docker stats

# Cek koneksi database
docker-compose -f docker-compose.prod.yml exec db pg_isready

# Cek Redis
docker-compose -f docker-compose.prod.yml exec redis redis-cli ping
```

## 10. Backup dan Disaster Recovery

### 10.1. Prosedur Backup

```bash
# Manual backup database
docker-compose -f docker-compose.prod.yml exec db pg_dump -U taskforge taskforge_production > backup_$(date +%Y%m%d_%H%M%S).sql

# Backup konfigurasi
tar -czf config_backup_$(date +%Y%m%d_%H%M%S).tar.gz /opt/taskforge/config/

# Backup logs penting
tar -czf logs_backup_$(date +%Y%m%d_%H%M%S).tar.gz /opt/taskforge/logs/
```

### 10.2. Prosedur Recovery

```bash
# Stop aplikasi
docker-compose -f docker-compose.prod.yml down

# Restore database
cat backup_file.sql | docker-compose -f docker-compose.prod.yml exec -T db psql -U taskforge taskforge_production

# Start aplikasi
docker-compose -f docker-compose.prod.yml up -d
```

## 11. Dokumentasi Operasional

### 11.1. Runbook Procedures

Untuk setiap jenis insiden, siapkan runbook procedures yang mencakup:
- Identifikasi gejala
- Langkah diagnosis
- Langkah remediasi
- Kriteria untuk eskalasi

### 11.2. On-Call Procedures

- Siapa yang harus dihubungi untuk jenis insiden tertentu
- Escalation path
- Tools dan dashboard yang digunakan
- Komunikasi incident ke stakeholder

## 12. Dokumentasi Tambahan

### 12.1. Architecture Decision Records (ADRs)

Simpan keputusan arsitektur penting dalam ADRs untuk referensi masa depan:
- Keputusan teknologi
- Trade-off yang dipertimbangkan
- Alasan pemilihan solusi tertentu

### 12.2. Deployment Notes

Dokumentasikan catatan deployment untuk setiap release:
- Tanggal deployment
- Versi aplikasi
- Perubahan penting
- Isu yang ditemukan dan solusi
- Metrics pasca-deployment