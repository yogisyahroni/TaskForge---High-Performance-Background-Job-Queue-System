# Pembuatan Dockerfile dan Docker Compose untuk Deployment TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan pembuatan Dockerfile dan docker-compose.yml untuk deployment aplikasi TaskForge. Sistem ini dirancang untuk memudahkan deployment di berbagai lingkungan (development, staging, production) dengan pendekatan containerization yang efisien dan scalable.

## 2. Arsitektur Deployment Container

### 2.1. Komponen Utama

Deployment TaskForge terdiri dari beberapa komponen utama dalam container:

1. **Aplikasi TaskForge** - Service utama yang menjalankan backend
2. **PostgreSQL** - Database utama untuk persistensi data
3. **Redis** - Cache dan internal messaging
4. **Prometheus** - Monitoring dan metrics collection
5. **Grafana** - Dashboard untuk observability
6. **NGINX** - Reverse proxy dan load balancing (opsional)

### 2.2. Struktur Direktori

```
taskforge/
├── Dockerfile                 # Dockerfile untuk aplikasi TaskForge
├── docker-compose.yml         # Konfigurasi deployment utama
├── docker-compose.prod.yml    # Konfigurasi deployment production
├── docker-compose.dev.yml     # Konfigurasi development
├── docker-compose.override.yml # Override lokal
├── nginx/
│   ├── nginx.conf            # Konfigurasi reverse proxy
│   └── ssl/                  # Sertifikat SSL
├── scripts/
│   ├── build.sh              # Script build untuk CI/CD
│   ├── deploy.sh             # Script deployment
│   └── migrate.sh            # Script migrasi database
└── .dockerignore            # File-file yang diabaikan oleh Docker
```

## 3. Dockerfile untuk Aplikasi TaskForge

### 3.1. Multi-stage Build Dockerfile

```dockerfile
# File: Dockerfile
# Stage 1: Builder
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

# Stage 2: Runtime
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl1.1 \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r taskforge && useradd -r -g taskforge taskforge

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
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8000/health || exit 1

# Command to run the application
CMD ["taskforge"]
```

### 3.2. File .dockerignore

```
# File: .dockerignore
.git
.gitignore
README.md
Dockerfile
.dockerignore
.env
*.log
target/
node_modules/
!target/release/taskforge  # Include the binary from target
!.env
```

## 4. Docker Compose Configuration

### 4.1. Docker Compose Utama

```yaml
# File: docker-compose.yml
version: '3.8'

services:
  # Database Service
  db:
    image: postgres:14-alpine
    container_name: taskforge_db
    environment:
      POSTGRES_DB: taskforge
      POSTGRES_USER: taskforge_user
      POSTGRES_PASSWORD: ${DB_PASSWORD:-taskforge_password}
      PGDATA: /var/lib/postgresql/data/pgdata
    volumes:
      - taskforge_postgres_data:/var/lib/postgresql/data
      - ./init-scripts:/docker-entrypoint-initdb.d
    ports:
      - "5432:5432"
    networks:
      - taskforge_network
    restart: unless-stopped
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U taskforge_user -d taskforge"]
      interval: 10s
      timeout: 5s
      retries: 5
      start_period: 30s

  # Redis Service
  redis:
    image: redis:7-alpine
    container_name: taskforge_redis
    command: redis-server --appendonly yes --requirepass ${REDIS_PASSWORD:-taskforge_redis_password}
    volumes:
      - taskforge_redis_data:/data
    ports:
      - "6379:6379"
    networks:
      - taskforge_network
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 10s
      timeout: 3s
      retries: 3
      start_period: 10s

  # TaskForge Application
  app:
    build:
      context: .
      dockerfile: Dockerfile
    container_name: taskforge_app
    depends_on:
      db:
        condition: service_healthy
      redis:
        condition: service_healthy
    environment:
      # Database Configuration
      - DATABASE_URL=postgresql://taskforge_user:${DB_PASSWORD:-taskforge_password}@db:5432/taskforge
      - DB_HOST=db
      - DB_PORT=5432
      - DB_NAME=taskforge
      - DB_USER=taskforge_user
      - DB_PASSWORD=${DB_PASSWORD:-taskforge_password}
      
      # Redis Configuration
      - REDIS_URL=redis://:${REDIS_PASSWORD:-taskforge_redis_password}@redis:6379/
      - REDIS_HOST=redis
      - REDIS_PORT=6379
      - REDIS_PASSWORD=${REDIS_PASSWORD:-taskforge_redis_password}
      
      # Application Configuration
      - APP_ENVIRONMENT=${APP_ENVIRONMENT:-development}
      - APPLICATION_HOST=0.0.0.0
      - APPLICATION_PORT=8000
      - CORS_ORIGIN=${CORS_ORIGIN:-*}
      - BASE_URL=${BASE_URL:-http://localhost:8000}
      
      # JWT Configuration
      - JWT_SECRET=${JWT_SECRET:-taskforge_jwt_secret}
      - JWT_EXPIRY_TIME_MINUTES=${JWT_EXPIRY_TIME_MINUTES:-60}
      - JWT_REFRESH_SECRET=${JWT_REFRESH_SECRET:-taskforge_jwt_refresh_secret}
      - JWT_REFRESH_EXPIRY_TIME_MINUTES=${JWT_REFRESH_EXPIRY_TIME_MINUTES:-10080}
      
      # Logging Configuration
      - LOG_LEVEL=${LOG_LEVEL:-info}
      - LOG_FORMAT=json
      
      # Metrics Configuration
      - METRICS_ENABLED=true
      - METRICS_ENDPOINT=/metrics
    ports:
      - "${APP_PORT:-8000}:8000"
    volumes:
      - ./logs:/app/logs
      - ./config:/app/config
    networks:
      - taskforge_network
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 60s

  # Prometheus for Metrics
  prometheus:
    image: prom/prometheus:latest
    container_name: taskforge_prometheus
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
      - '--storage.tsdb.retention.time=200h'
      - '--web.enable-lifecycle'
    networks:
      - taskforge_network
    restart: unless-stopped

  # Grafana for Dashboards
  grafana:
    image: grafana/grafana-enterprise:latest
    container_name: taskforge_grafana
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=${GRAFANA_ADMIN_PASSWORD:-admin}
      - GF_USERS_ALLOW_SIGN_UP=false
    volumes:
      - taskforge_grafana_data:/var/lib/grafana
      - ./grafana/provisioning:/etc/grafana/provisioning
    networks:
      - taskforge_network
    restart: unless-stopped
    depends_on:
      - prometheus

networks:
  taskforge_network:
    driver: bridge

volumes:
  taskforge_postgres_data:
    driver: local
  taskforge_redis_data:
    driver: local
  taskforge_prometheus_data:
    driver: local
  taskforge_grafana_data:
    driver: local
```

### 4.2. Konfigurasi Production

```yaml
# File: docker-compose.prod.yml
version: '3.8'

services:
  app:
    # Production-specific environment variables
    environment:
      - APP_ENVIRONMENT=production
      - LOG_LEVEL=warn
      - CORS_ORIGIN=https://taskforge.yourdomain.com
      - BASE_URL=https://api.taskforge.yourdomain.com
      
      # Security enhancements
      - DATABASE_REQUIRE_SSL=true
      - SECURITY_HSTS=true
      - SECURITY_CSP=default-src 'self'
      
      # Performance optimizations
      - WORKERS=${WORKERS:-4}
      - MAX_CONNECTIONS=${MAX_CONNECTIONS:-100}
      - POOL_SIZE=${POOL_SIZE:-20}
    
    # Resource limits for production
    deploy:
      resources:
        limits:
          cpus: '${APP_CPUS:-2.0}'
          memory: '${APP_MEMORY:-2G}'
        reservations:
          cpus: '${APP_CPUS_RESERVE:-0.5}'
          memory: '${APP_MEMORY_RESERVE:-512M}'
    
    # Production-specific health check
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 60s

  db:
    deploy:
      resources:
        limits:
          cpus: '${DB_CPUS:-2.0}'
          memory: '${DB_MEMORY:-4G}'
        reservations:
          cpus: '${DB_CPUS_RESERVE:-1.0}'
          memory: '${DB_MEMORY_RESERVE:-1G}'
    environment:
      - POSTGRES_DB=taskforge_prod
      # Production database settings
      - POSTGRES_INITDB_ARGS="--encoding=UTF-8 --locale=C"
    
    # Additional production database configuration
    command: >
      postgres
      -c max_connections=${DB_MAX_CONNECTIONS:-200}
      -c shared_buffers=${DB_SHARED_BUFFERS:-1GB}
      -c effective_cache_size=${DB_EFFECTIVE_CACHE_SIZE:-3GB}
      -c maintenance_work_mem=${DB_MAINTENANCE_WORK_MEM:-256MB}
      -c checkpoint_completion_target=0.9
      -c wal_buffers=16MB
      -c default_statistics_target=100
      -c random_page_cost=1.1
      -c effective_io_concurrency=200
      -c work_mem=4MB
      -c min_wal_size=1GB
      -c max_wal_size=4GB
      -c max_worker_processes=2
      -c max_parallel_workers_per_gather=1
      -c max_parallel_workers=2
      -c max_parallel_maintenance_workers=1

  redis:
    deploy:
      resources:
        limits:
          cpus: '${REDIS_CPUS:-1.0}'
          memory: '${REDIS_MEMORY:-1G}'
        reservations:
          cpus: '${REDIS_CPUS_RESERVE:-0.25}'
          memory: '${REDIS_MEMORY_RESERVE:-256M}'
    command: >
      redis-server
      --maxmemory ${REDIS_MAXMEMORY:-512mb}
      --maxmemory-policy allkeys-lru
      --save 900 1
      --save 300 10
      --save 60 10000
      --appendonly yes
      --requirepass ${REDIS_PASSWORD}
```

### 4.3. Konfigurasi Development

```yaml
# File: docker-compose.dev.yml
version: '3.8'

services:
  app:
    # Mount source code for hot reloading
    volumes:
      - .:/app
      - /app/target  # Exclude target directory
      - ./logs:/app/logs
      - ./config:/app/config
      # Development-specific volume for faster rebuilds
      - cargo_registry:/usr/local/cargo/registry
      - cargo_git:/usr/local/cargo/git
    
    # Development-specific environment
    environment:
      - APP_ENVIRONMENT=development
      - LOG_LEVEL=debug
      - RUST_LOG=debug
      - RELOAD_CODE=true
    
    # Enable development tools
    command: >
      sh -c "
      cargo watch -x run --port 8000
      "
    
    # Development ports
    ports:
      - "${APP_PORT:-8000}:8000"
      - "8001:8001"  # Debug port
      - "5678:5678"  # Debugger port

  db:
    # Development-specific database settings
    environment:
      - POSTGRES_DB=taskforge_dev
    volumes:
      - taskforge_postgres_data_dev:/var/lib/postgresql/data
      - ./migrations:/docker-entrypoint-initdb.d

  redis:
    volumes:
      - taskforge_redis_data_dev:/data

volumes:
  cargo_registry:
    driver: local
  cargo_git:
    driver: local
  taskforge_postgres_data_dev:
    driver: local
  taskforge_redis_data_dev:
    driver: local
```

## 5. Konfigurasi Monitoring dan Observability

### 5.1. Konfigurasi Prometheus

```yaml
# File: prometheus/prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

rule_files:
  - "rules.yml"

scrape_configs:
  - job_name: 'taskforge-app'
    static_configs:
      - targets: ['app:8000']
    metrics_path: /metrics
    scrape_interval: 10s
    scrape_timeout: 5s
    honor_labels: true
    relabel_configs:
      - source_labels: [__address__]
        target_label: instance

  - job_name: 'postgres-exporter'
    static_configs:
      - targets: ['db:5432']
    scrape_interval: 30s
    scrape_timeout: 10s

  - job_name: 'redis-exporter'
    static_configs:
      - targets: ['redis:6379']
    scrape_interval: 30s
    scrape_timeout: 10s

  - job_name: 'node-exporter'
    static_configs:
      - targets: ['node-exporter:9100']
    scrape_interval: 30s
    scrape_timeout: 10s

alerting:
  alertmanagers:
    - static_configs:
        - targets:
          - alertmanager:9093
```

### 5.2. Provisioning Grafana

```yaml
# File: grafana/provisioning/dashboards/default.yaml
apiVersion: 1

providers:
  - name: 'default'
    orgId: 1
    folder: ''
    type: file
    disableDeletion: false
    updateIntervalSeconds: 10
    options:
      path: /etc/grafana/provisioning/dashboards
```

```yaml
# File: grafana/provisioning/datasources/default.yaml
apiVersion: 1

datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus:9090
    isDefault: true
    editable: true
```

## 6. Konfigurasi Reverse Proxy (NGINX)

### 6.1. Konfigurasi NGINX

```nginx
# File: nginx/nginx.conf
events {
    worker_connections 1024;
}

http {
    upstream taskforge_backend {
        server app:8000;
        # Jika menggunakan multiple instances
        # server app1:8000;
        # server app2:8000;
    }

    # Security headers
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-XSS-Protection "1; mode=block" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header Referrer-Policy "no-referrer-when-downgrade" always;
    add_header Content-Security-Policy "default-src 'self' http: https: data: blob: 'unsafe-inline'" always;

    # Gzip compression
    gzip on;
    gzip_vary on;
    gzip_min_length 1024;
    gzip_proxied expired no-cache no-store private must-revalidate auth;
    gzip_types text/plain text/css text/xml text/javascript application/x-javascript application/xml+rss application/javascript application/json;

    server {
        listen 80;
        server_name localhost;
        
        # Redirect HTTP to HTTPS
        return 301 https://$server_name$request_uri;
    }

    server {
        listen 443 ssl http2;
        server_name localhost;

        # SSL Configuration
        ssl_certificate /etc/nginx/ssl/cert.pem;
        ssl_certificate_key /etc/nginx/ssl/key.pem;
        ssl_protocols TLSv1.2 TLSv1.3;
        ssl_ciphers ECDHE-RSA-AES256-GCM-SHA512:DHE-RSA-AES256-GCM-SHA512:ECDHE-RSA-AES256-GCM-SHA384:DHE-RSA-AES256-GCM-SHA384;
        ssl_prefer_server_ciphers off;
        ssl_session_cache shared:SSL:10m;
        ssl_session_timeout 10m;

        # Rate limiting
        limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
        limit_req zone=api burst=20 nodelay;

        location / {
            proxy_pass http://taskforge_backend;
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
            proxy_set_header X-Forwarded-Host $server_name;
            
            # Timeout settings
            proxy_connect_timeout 60s;
            proxy_send_timeout 60s;
            proxy_read_timeout 60s;
            
            # Buffer settings
            proxy_buffer_size 4k;
            proxy_buffers 8 4k;
            proxy_busy_buffers_size 8k;
        }

        # Health check endpoint
        location /health {
            access_log off;
            proxy_pass http://taskforge_backend/health;
        }

        # Metrics endpoint (secured)
        location /metrics {
            allow 127.0.0.1;
            allow 172.16.0.0/12;  # Docker network range
            deny all;
            proxy_pass http://taskforge_backend/metrics;
        }

        # Rate limiting for API endpoints
        location /api {
            limit_req zone=api burst=5 nodelay;
            proxy_pass http://taskforge_backend;
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
        }
    }
}
```

## 7. Script Deployment

### 7.1. Script Build

```bash
#!/bin/bash
# File: scripts/build.sh

set -e

echo "Building TaskForge application..."

# Build the application
cargo build --release

# Build Docker image
echo "Building Docker image..."
docker build -t taskforge:latest .

echo "Build completed successfully!"
```

### 7.2. Script Deployment

```bash
#!/bin/bash
# File: scripts/deploy.sh

set -e

ENVIRONMENT=${1:-production}
TAG=${2:-latest}

echo "Deploying TaskForge to $ENVIRONMENT environment with tag $TAG..."

case $ENVIRONMENT in
  "production")
    echo "Starting production deployment..."
    docker-compose -f docker-compose.yml -f docker-compose.prod.yml pull
    docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d --remove-orphans
    ;;
  "staging")
    echo "Starting staging deployment..."
    docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d --remove-orphans
    ;;
  "development")
    echo "Starting development deployment..."
    docker-compose -f docker-compose.yml -f docker-compose.dev.yml up -d --remove-orphans
    ;;
  *)
    echo "Usage: $0 [production|staging|development] [tag]"
    exit 1
    ;;
esac

echo "Deployment completed!"

# Wait for services to be healthy
echo "Waiting for services to be healthy..."
sleep 30

# Check health status
docker-compose ps
```

### 7.3. Script Migrasi Database

```bash
#!/bin/bash
# File: scripts/migrate.sh

set -e

echo "Running database migrations..."

# Wait for database to be ready
echo "Waiting for database to be ready..."
until docker-compose exec db pg_isready > /dev/null 2>&1
do
    sleep 2
done

echo "Database is ready. Running migrations..."

# Run migrations using SQLx CLI
docker-compose exec app sqlx migrate run

echo "Migrations completed!"
```

## 8. Konfigurasi CI/CD

### 8.1. GitHub Actions Workflow

```yaml
# File: .github/workflows/deploy.yml
name: Deploy TaskForge

on:
  push:
    branches: [ main, develop ]
  pull_request:
    branches: [ main ]

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:14
        env:
          POSTGRES_PASSWORD: postgres
          POSTGRES_DB: taskforge_test
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5432:5432
      redis:
        image: redis:7-alpine
        options: >-
          --health-cmd "redis-cli ping"
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 6379:6379

    steps:
    - uses: actions/checkout@v3
    
    - name: Install Rust
      uses: actions-rs/toolchain@v1
      with:
        profile: minimal
        toolchain: stable
        override: true
    
    - name: Install dependencies
      run: |
        sudo apt-get update
        sudo apt-get install -y libpq-dev pkg-config
    
    - name: Cache Cargo registry
      uses: actions/cache@v3
      with:
        path: |
          ~/.cargo/registry
          ~/.cargo/git
        key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
    
    - name: Run tests
      run: |
        cargo test --verbose
      env:
        DATABASE_URL: postgresql://postgres:postgres@localhost:5432/taskforge_test
        REDIS_URL: redis://localhost:6379/

  build-and-push:
    needs: test
    runs-on: ubuntu-latest
    if: github.ref == 'refs/heads/main'
    
    steps:
    - uses: actions/checkout@v3
    
    - name: Set up Docker Buildx
      uses: docker/setup-buildx-action@v2
    
    - name: Login to DockerHub
      uses: docker/login-action@v2
      with:
        username: ${{ secrets.DOCKERHUB_USERNAME }}
        password: ${{ secrets.DOCKERHUB_TOKEN }}
    
    - name: Build and push
      uses: docker/build-push-action@v4
      with:
        context: .
        push: true
        tags: |
          yourusername/taskforge:${{ github.sha }}
          yourusername/taskforge:latest
        cache-from: type=gha
        cache-to: type=gha,mode=max

  deploy:
    needs: build-and-push
    runs-on: self-hosted
    if: github.ref == 'refs/heads/main'
    
    steps:
    - name: Deploy to production
      run: |
        cd /path/to/taskforge
        git pull origin main
        docker-compose -f docker-compose.yml -f docker-compose.prod.yml pull
        docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d --remove-orphans
```

## 9. Best Practices dan Rekomendasi

### 9.1. Praktik Terbaik untuk Docker

1. **Gunakan multi-stage builds** - untuk mengurangi ukuran image
2. **Gunakan .dockerignore** - untuk mencegah file yang tidak perlu disertakan
3. **Gunakan non-root user** - untuk keamanan
4. **Gunakan resource limits** - untuk mencegah resource exhaustion
5. **Gunakan health checks** - untuk monitoring kesehatan service

### 9.2. Praktik Terbaik untuk Docker Compose

1. **Gunakan environment variables** - untuk konfigurasi fleksibel
2. **Gunakan named volumes** - untuk persistensi data
3. **Gunakan external networks** - untuk integrasi dengan service lain
4. **Gunakan override files** - untuk konfigurasi berbeda per environment
5. **Gunakan secrets management** - untuk credential yang aman

### 9.3. Skalabilitas dan Kinerja

1. **Gunakan connection pooling** - untuk efisiensi koneksi database
2. **Gunakan caching strategies** - untuk kinerja aplikasi
3. **Gunakan load balancing** - untuk distribusi beban
4. **Gunakan monitoring dan alerting** - untuk deteksi masalah
5. **Gunakan auto-scaling** - untuk respons terhadap beban