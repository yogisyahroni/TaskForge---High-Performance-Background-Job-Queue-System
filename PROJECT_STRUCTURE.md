# Struktur Proyek Rust untuk TaskForge

## 1. Gambaran Umum

TaskForge adalah platform SaaS background job queue berbasis Rust yang dirancang untuk keandalan dan kinerja tinggi. Dokumen ini menjelaskan struktur proyek Rust yang akan digunakan dalam pengembangan aplikasi.

## 2. Struktur Direktori

```
taskforge/
├── Cargo.toml              # Konfigurasi proyek utama dan dependensi
├── Cargo.lock              # Lock file untuk dependensi
├── README.md               # Dokumentasi proyek
├── .env                    # Konfigurasi environment (tidak disimpan di git)
├── .gitignore              # File yang diabaikan oleh git
├── docker-compose.yml      # Konfigurasi untuk development environment
├── Dockerfile              # Dockerfile untuk deployment
├── migrations/             # File migrasi database
│   ├── 001_create_organizations_table.sql
│   ├── 002_create_users_table.sql
│   ├── 03_create_subscriptions_table.sql
│   ├── 004_create_projects_table.sql
│   ├── 005_create_api_keys_table.sql
│   ├── 006_create_job_queues_table.sql
│   ├── 007_create_jobs_table.sql
│   ├── 008_create_workers_table.sql
│   ├── 009_create_queue_worker_assignments_table.sql
│   ├── 010_create_job_executions_table.sql
│   ├── 011_create_execution_logs_table.sql
│   └── 012_create_job_dependencies_table.sql
├── src/
│   ├── main.rs             # Entry point aplikasi
│   ├── config/             # Konfigurasi aplikasi
│   │   ├── mod.rs
│   │   └── settings.rs
│   ├── models/             # Model data dan ORM mapping
│   │   ├── mod.rs
│   │   ├── organization.rs
│   │   ├── user.rs
│   │   ├── subscription.rs
│   │   ├── project.rs
│   │   ├── api_key.rs
│   │   ├── job_queue.rs
│   │   ├── job.rs
│   │   ├── worker.rs
│   │   ├── queue_worker_assignment.rs
│   │   ├── job_execution.rs
│   │   ├── execution_log.rs
│   │   └── job_dependency.rs
│   ├── services/           # Business logic dan service layer
│   │   ├── mod.rs
│   │   ├── auth_service.rs
│   │   ├── organization_service.rs
│   │   ├── project_service.rs
│   │   ├── api_key_service.rs
│   │   ├── queue_service.rs
│   │   ├── job_service.rs
│   │   ├── worker_service.rs
│   │   ├── execution_service.rs
│   │   └── scheduler_service.rs
│   ├── handlers/           # Handler untuk API endpoints
│   │   ├── mod.rs
│   │   ├── auth_handler.rs
│   │   ├── organization_handler.rs
│   │   ├── project_handler.rs
│   │   ├── api_key_handler.rs
│   │   ├── queue_handler.rs
│   │   ├── job_handler.rs
│   │   └── worker_handler.rs
│   ├── utils/              # Fungsi utilitas
│   │   ├── mod.rs
│   │   ├── crypto.rs
│   │   ├── validation.rs
│   │   └── constants.rs
│   ├── middleware/         # Middleware untuk aplikasi
│   │   ├── mod.rs
│   │   └── auth_middleware.rs
│   ├── database/           # Konfigurasi dan operasi database
│   │   ├── mod.rs
│   │   └── connection.rs
│   └── grpc/               # Implementasi gRPC jika diperlukan
│       ├── mod.rs
│       └── job_queue.proto
├── tests/                  # File-file pengujian
│   ├── integration/
│   ├── unit/
│   └── e2e/
├── benches/                # Benchmark untuk kinerja
├── scripts/                # Script utilitas
│   └── setup_db.sh
└── docs/                   # Dokumentasi tambahan
    ├── api.md
    └── deployment.md
```

## 3. Dependensi Utama

### Framework dan Runtime
- `tokio`: Runtime async untuk Rust
- `axum`: Web framework modern untuk Rust
- `serde`: Serialization/deserialization framework
- `serde_json`: JSON serialization support

### Database dan ORM
- `sqlx`: SQL toolkit dan ORM dengan compile-time checking
- `uuid`: Pembuatan dan parsing UUID
- `chrono`: Manipulasi tanggal dan waktu

### Keamanan dan Otentikasi
- `jsonwebtoken`: JWT implementation
- `bcrypt`: Password hashing
- `hex`: Konversi hex untuk kriptografi

### Observability
- `tracing`: Framework untuk logging dan tracing
- `tracing-subscriber`: Subscribers untuk tracing
- `prometheus`: Client untuk mengumpulkan metrics

### Konfigurasi dan Environment
- `config`: Konfigurasi aplikasi dari berbagai sumber
- `dotenv`: Pembacaan variabel environment dari file .env

### Testing
- `tokio-test`: Testing untuk async code
- `assert-json-diff`: Perbandingan JSON dalam testing

## 4. Pola Arsitektur yang Digunakan

### Model-View-Controller (MVC) Teradaptasi
- **Models**: Mendefinisikan struktur data dan interaksi dengan database
- **Services**: Menangani business logic dan operasi kompleks
- **Handlers**: Menangani permintaan HTTP dan mengembalikan respons

### Repository Pattern
- Abstraksi dari operasi database untuk memudahkan testing dan pemeliharaan
- Setiap model memiliki repository yang bertanggung jawab atas operasi CRUD

### Dependency Injection
- Menggunakan Axum's state management untuk menyediakan dependensi ke handler
- Memudahkan testing dengan kemampuan untuk menyuntikkan mock

## 5. Konfigurasi Aplikasi

Aplikasi akan menggunakan sistem konfigurasi berlapis:

1. **Environment Variables**: Untuk konfigurasi yang berbeda antara environment
2. **Configuration Files**: Untuk konfigurasi yang kompleks dan terstruktur
3. **Defaults**: Nilai bawaan untuk konfigurasi opsional

## 6. Manajemen Database

### Migrasi Database
- Menggunakan sistem migrasi berbasis SQL dengan SQLx
- Migrasi akan dijalankan otomatis saat aplikasi dimulai
- Setiap migrasi memiliki versi dan dapat di-rollback

### Koneksi Database
- Connection pooling menggunakan SQLx
- Timeout konfigurasi untuk operasi database
- Retry mechanism untuk koneksi yang gagal

## 7. Konfigurasi dan Environment

Aplikasi akan mendukung beberapa environment:

- **Development**: Mode debugging dengan logging verbose
- **Staging**: Mode untuk testing sebelum production
- **Production**: Mode performa maksimum dengan logging optimal

Setiap environment memiliki konfigurasi yang berbeda untuk:
- Database connection strings
- API keys dan secrets
- Logging levels
- Feature flags