Product Requirements Document (PRD): TaskForge - High-Performance Background Job Queue SaaS
1. Vision & Executive Summary
Nama Produk: TaskForge
Tagline: The Unbreakable Background Job Engine. Built with Rust for mission-critical workloads.
Visi: Menjadi platform SaaS background job queue yang paling andal dan berperforma tinggi di pasaran, khususnya untuk bisnis yang proses latar belakangnya bersifat kritis (fintech, e-commerce, data processing).

Value Proposition Utama:

"Zero-Downtime, Zero-Memory-Leak" Reliability: Menggunakan jaminan keamanan memori dan konkurensi Rust untuk operasi yang stabil 24/7.

Latensi Ultra-Rendah & Throughput Tinggi: Menjalankan dan mengirimkan ratusan ribu job per detik dengan konsistensi tinggi.

Developer Experience yang Sempurna: Dokumentasi API yang jelas, SDK untuk berbagai bahasa, dan dashboard observasi yang powerful.

2. User Personas
DevOps Engineer (Sasha):

Goals: Memastikan semua sistem background processing berjalan stabil, mudah dimonitor, dan tidak menyebabkan insiden.

Pain Points: Job queue yang sering hang, memory leak yang tidak terdeteksi, dan alat monitoring yang terbatas.

Backend Developer (Budi):

Goals: Integrasi job queue yang mudah dan cepat ke dalam aplikasinya. Ingin fokus pada logika bisnis, bukan infrastruktur queue.

Pain Points: Setup dan maintenance queue yang rumit (Redis + worker), skalabilitas yang manual, dan kesulitan debugging job yang gagal.

CTO / Tech Lead (Citra):

Goals: Mengurangi biaya infrastruktur dan risiko downtime. Meningkatkan efisiensi tim engineering.

Pain Points: Biaya tersembunyi untuk mengelola kluster queue, waktu engineer yang terbuang untuk maintenance, dan ketakutan akan kehilangan data job penting.

3. Entity-Relationship Diagram (ERD)
Diagram ini menunjukkan desain database inti untuk mendukung fitur-fitur TaskForge.

erDiagram
    Organization ||--o{ Project : owns
    Organization ||--o{ User : has
    Organization ||--o{ Subscription : has

    Project ||--o{ JobQueue : contains
    Project ||--o{ Worker : manages
    Project ||--o{ ApiKey : has

    JobQueue ||--o{ Job : receives
    JobQueue ||--o{ QueueWorkerAssignment : configures

    Worker ||--o{ QueueWorkerAssignment : assigned_to
    Worker ||--o{ JobExecution : performs

    Job ||--o{ JobExecution : has_history
    Job ||--o{ JobDependency : may_have

    JobExecution ||--o{ ExecutionLog : produces

    Organization {
        uuid id PK
        string name
        string billing_email
        timestamp created_at
    }
    User {
        uuid id PK
        uuid organization_id FK
        string email
        string password_hash
        string role "admin, member"
    }
    Subscription {
        uuid id PK
        uuid organization_id FK
        string tier "starter, pro, enterprise"
        jsonb limits "max_jobs_per_month, max_workers, etc"
        timestamp current_period_end
    }
    Project {
        uuid id PK
        uuid organization_id FK
        string name
        string description
    }
    ApiKey {
        uuid id PK
        uuid project_id FK
        string key_hash
        string name
        string[] permissions
        timestamp last_used_at
        timestamp expires_at
    }
    JobQueue {
        uuid id PK
        uuid project_id FK
        string name
        int priority
        jsonb settings "retry_policy, timeout, etc"
    }
    Job {
        uuid id PK
        uuid queue_id FK
        string job_type
        jsonb payload
        string status "pending, scheduled, processing, succeeded, failed"
        int priority
        timestamp scheduled_for
        timestamp created_at
    }
    Worker {
        uuid id PK
        uuid project_id FK
        string name
        string type "general, specialized_cpu, specialized_io"
        string status "online, offline, draining"
        timestamp last_heartbeat
    }
    QueueWorkerAssignment {
        uuid queue_id FK
        uuid worker_id FK
        int weight
        boolean is_active
    }
    JobExecution {
        uuid id PK
        uuid job_id FK
        uuid worker_id FK
        string status "started, succeeded, failed, retrying"
        text output
        int attempt_number
        timestamp started_at
        timestamp finished_at
    }
    ExecutionLog {
        uuid id PK
        uuid execution_id FK
        string level "info, warn, error"
        text message
        jsonb metadata
        timestamp logged_at
    }
    JobDependency {
        uuid parent_job_id FK
        uuid child_job_id FK
    }

    Catatan Desain untuk Performa (Rust-centric):

status fields: Menggunakan string enum sederhana untuk memudahkan query dan logika aplikasi.

jsonb fields (PostgreSQL): Untuk payload, settings, dan metadata yang fleksibel, memungkinkan schema yang dinamis tanpa migrasi database.

Indexing Strategy: Index akan dibuat pada kolom: Job(status, scheduled_for, priority), Job(queue_id), JobExecution(job_id), Worker(last_heartbeat), dan Organization(created_at) untuk query yang cepat.

Partisi Tabel: Tabel Job dan JobExecution dapat dipartisi berdasarkan waktu (created_at) untuk manajemen data historis yang efisien.

4. Fitur Utama (Core Features)
4.1. Job Management & API
Job Submission: RESTful API & gRPC endpoint untuk men-submit job (sync & async). Payload support JSON, Protobuf, atau plain text.

Job Scheduling: Jadwalkan job untuk dieksekusi sekali di masa depan atau secara periodik (cron-like expressions).

Job Dependencies: Definisikan job yang hanya berjalan setelah job lain berhasil selesai (Directed Acyclic Graph).

Job Priorities: Tentukan prioritas (0-9) untuk job dalam queue yang sama. Job dengan prioritas lebih tinggi dieksekusi terlebih dahulu.

Delayed Jobs: Kirim job dengan penundaan tertentu.

Batch Jobs: Submit sekumpulan job sebagai satu unit.

4.2. Worker & Queue Management
Dynamic Worker Registration: Worker (klien Anda) dapat mendaftar secara dinamis via HTTP API, mengidentifikasi jenis dan kapabilitasnya.

Smart Queue-Worker Assignment: Admin dapat mengatur worker mana yang menarik job dari queue mana, dengan konfigurasi pembobotan (weight).

Worker Autoscaling Hooks: Webhook/API yang terintegrasi dengan cloud provider (AWS ASG, K8s HPA) untuk menambah/mengurangi worker berdasarkan panjang queue (queue depth).

Worker Drain Mode: Secara graceful menghentikan worker dari menerima job baru, menyelesaikan job yang sedang berjalan, lalu shutdown.

4.3. Resilience & Reliability (Rust's Forte)
Atomic Retries with Backoff: Mekanisme retry yang built-in dengan exponential backoff (configurable). Status job dijamin konsisten.

Job Timeout & Heartbeat: Set timeout per job atau per queue. Worker harus mengirim heartbeat selama eksekusi; job dianggap gagal jika heartbeat hilang.

Dead Letter Queue (DLQ): Job yang gagal terus-menerus setelah retry maksimal dipindahkan ke DLQ untuk inspeksi manual.

At-Least-Once Delivery: Jaminan bahwa job tidak akan hilang (jaminan dikirim minimal sekali).

Idempotency Keys: Klien dapat menyertakan kunci idempoten untuk mencegah eksekusi duplikat jika terjadi retry network call.

4.4. Monitoring, Observability & Alerts
Real-time Dashboard: Live view antrian, status worker, throughput (jobs/sec), dan latency.

Job Lifecycle Tracing: Melacak sejarah lengkap setiap job: kapan dibuat, di-queue, diproses oleh worker mana, berapa lama, log output, dan status akhir.

Advanced Filtering & Search: Cari job berdasarkan ID, tipe, status, periode waktu, atau isi payload (full-text search terbatas).

Comprehensive Metrics (Prometheus): Export metrik kunci (queue length, job completion rate, error rate, worker count, latency percentiles) ke Prometheus.

Custom Alerts: Konfigurasi alert via email/Slack/Webhook untuk: queue yang menumpuk (stalled), tingkat kegagalan tinggi, worker offline, atau latency melampaui threshold.

4.5. Security, Multi-tenancy & Billing
Project & API Key Isolation: Setiap organisasi memiliki project terpisah. API Key dilimitasi per project dengan permission granular (read, write, admin).

Audit Log: Mencatat semua aksi administratif (buat/ubah/hapus queue, rotasi API key, dll).

Encryption at Rest & in Transit: Semua data dienkripsi (TLS 1.3 untuk transit, AES-256 untuk data sensitif di database).

Usage-Based Metering & Billing: Sistem metering real-time menghitung: jumlah job eksekusi, total durasi eksekusi (GB-seconds), dan jumlah worker aktif. Data ini menjadi dasar billing pay-as-you-go.

Rate Limiting & Quotas: Limit API calls dan job submissions per project berdasarkan tier subscription.

5. Tech Stack Proposal
Backend (Rust): Axum (web framework), Tokio (runtime async), SQLx (database client dengan compile-time query check), Serde (serialization).

Database: PostgreSQL (utama), Redis (untuk caching rate limit dan pub/sub event internal).

Observability: OpenTelemetry (tracing), Prometheus (metrics), Grafana (dashboard).

Message Broker (Internal): Untuk komunikasi internal yang membutuhkan throughput sangat tinggi, pertimbangkan NATS atau memanfaatkan channel Tokio yang ringan.

Deployment: Docker, Kubernetes. Dapat di-deploy di AWS/GCP/Azure atau di platform yang Rust-friendly seperti Fly.io.

6. Success Metrics (KPIs)
Reliability: 99.99% Uptime SLA, Zero Critical Bugs related to memory safety.

Performance: P95 Job Latency < 100ms untuk job kosong, throughput > 10,000 jobs/sec per core.

Business: Customer Acquisition Cost (CAC), Monthly Recurring Revenue (MRR), Net Promoter Score (NPS) dari developer.