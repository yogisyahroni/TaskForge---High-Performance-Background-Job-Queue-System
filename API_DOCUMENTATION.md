# Dokumentasi API dan Penggunaan untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menyediakan dokumentasi lengkap untuk API TaskForge dan panduan penggunaan sistem. Dokumentasi ini mencakup semua endpoint API, contoh penggunaan, skema data, dan instruksi untuk integrasi dengan sistem eksternal.

## 2. Struktur API

### 2.1. Base URL dan Versi

```
Base URL: https://api.taskforge.com
Version: v1
Base Path: /api/v1
```

### 2.2. Format Data

- **Content-Type**: `application/json`
- **Character Encoding**: UTF-8
- **Date Format**: ISO 8601 (YYYY-MM-DDTHH:MM:SSZ)
- **Response Format**: JSON

### 2.3. Standar Response Format

```json
{
  "success": true,
  "data": {},
  "message": "Success message",
  "timestamp": "2023-01-01T00:00:00Z",
  "request_id": "uuid-v4-string"
}
```

### 2.4. Standar Error Format

```json
{
  "success": false,
  "error": {
    "code": "ERROR_CODE",
    "message": "Human readable error message",
    "details": {}
  },
  "timestamp": "2023-01-01T00:00:00Z",
  "request_id": "uuid-v4-string"
}
```

## 3. Otentikasi dan Otorisasi

### 3.1. JWT Token Authentication

Semua endpoint API dilindungi dengan otentikasi JWT. Token harus disertakan dalam header:

```
Authorization: Bearer <jwt-token>
```

### 3.2. Mendapatkan Token

**Endpoint**: `POST /auth/login`

```json
{
  "email": "user@example.com",
  "password": "secure-password"
}
```

**Response**:
```json
{
  "success": true,
  "data": {
    "access_token": "jwt-token-string",
    "refresh_token": "refresh-token-string",
    "expires_in": 3600,
    "token_type": "Bearer"
  },
  "message": "Login successful"
}
```

### 3.3. Refresh Token

**Endpoint**: `POST /auth/refresh`

```json
{
  "refresh_token": "refresh-token-string"
}
```

## 4. Organisasi dan Proyek API

### 4.1. Mendapatkan Daftar Organisasi

**Endpoint**: `GET /organizations`

**Response**:
```json
{
  "success": true,
  "data": {
    "organizations": [
      {
        "id": "uuid-v4",
        "name": "Company Name",
        "billing_email": "billing@company.com",
        "tier": "pro",
        "created_at": "2023-01-01T00:00:00Z"
      }
    ],
    "pagination": {
      "page": 1,
      "limit": 10,
      "total": 1,
      "pages": 1
    }
  }
}
```

### 4.2. Membuat Proyek Baru

**Endpoint**: `POST /projects`

**Request Body**:
```json
{
  "name": "My Project",
  "description": "Project description",
  "organization_id": "uuid-v4"
}
```

**Response**:
```json
{
  "success": true,
  "data": {
    "id": "uuid-v4",
    "name": "My Project",
    "description": "Project description",
    "organization_id": "uuid-v4",
    "created_at": "2023-01-01T00:00:00Z"
  },
  "message": "Project created successfully"
}
```

## 5. Queue Management API

### 5.1. Membuat Queue Baru

**Endpoint**: `POST /queues`

**Request Body**:
```json
{
  "name": "email-processing-queue",
  "description": "Queue for processing emails",
  "project_id": "uuid-v4",
  "priority": 5,
  "settings": {
    "max_concurrent_jobs": 10,
    "max_retries": 3,
    "timeout_seconds": 300,
    "retry_delay_seconds": 30,
    "max_retry_delay_seconds": 300,
    "backoff_multiplier": 2.0,
    "dead_letter_queue_id": null
  }
}
```

**Response**:
```json
{
  "success": true,
  "data": {
    "id": "uuid-v4",
    "project_id": "uuid-v4",
    "name": "email-processing-queue",
    "description": "Queue for processing emails",
    "priority": 5,
    "settings": {
      "max_concurrent_jobs": 10,
      "max_retries": 3,
      "timeout_seconds": 300,
      "retry_delay_seconds": 30,
      "max_retry_delay_seconds": 300,
      "backoff_multiplier": 2.0,
      "dead_letter_queue_id": null
    },
    "is_active": true,
    "created_at": "2023-01-01T00:00:00Z",
    "updated_at": "2023-01-01T00:00:00Z"
  },
  "message": "Queue created successfully"
}
```

### 5.2. Mendapatkan Daftar Queue

**Endpoint**: `GET /queues`

**Parameter Query**:
- `project_id` (optional): Filter by project
- `page` (optional): Page number (default: 1)
- `limit` (optional): Items per page (default: 10, max: 100)

**Response**:
```json
{
  "success": true,
  "data": {
    "queues": [
      {
        "id": "uuid-v4",
        "project_id": "uuid-v4",
        "name": "email-processing-queue",
        "description": "Queue for processing emails",
        "priority": 5,
        "settings": {},
        "is_active": true,
        "stats": {
          "pending_jobs": 5,
          "processing_jobs": 2,
          "succeeded_jobs": 100,
          "failed_jobs": 3,
          "scheduled_jobs": 1
        },
        "created_at": "2023-01-01T00:00:00Z"
      }
    ],
    "pagination": {
      "page": 1,
      "limit": 10,
      "total": 1,
      "pages": 1
    }
  }
}
```

### 5.3. Mengupdate Queue

**Endpoint**: `PUT /queues/{queue_id}`

**Request Body**:
```json
{
  "name": "updated-queue-name",
  "description": "Updated description",
  "priority": 7,
  "settings": {
    "max_concurrent_jobs": 15,
    "max_retries": 5
  },
  "is_active": true
}
```

### 5.4. Menghapus Queue

**Endpoint**: `DELETE /queues/{queue_id}`

## 6. Job Management API

### 6.1. Submit Job Baru

**Endpoint**: `POST /jobs`

**Request Body**:
```json
{
  "queue_name": "email-processing-queue",
  "job_type": "send_email",
  "payload": {
    "to": "recipient@example.com",
    "subject": "Hello World",
    "body": "This is a test email"
  },
  "priority": 5,
  "max_attempts": 3,
  "scheduled_for": null,
  "idempotency_key": "unique-key-for-idempotency"
}
```

**Response**:
```json
{
  "success": true,
  "data": {
    "id": "uuid-v4",
    "queue_id": "uuid-v4",
    "job_type": "send_email",
    "payload": {
      "to": "recipient@example.com",
      "subject": "Hello World",
      "body": "This is a test email"
    },
    "status": "pending",
    "priority": 5,
    "max_attempts": 3,
    "attempt_count": 0,
    "created_at": "2023-01-01T00:00:00Z",
    "updated_at": "2023-01-01T00:00:00Z"
  },
  "message": "Job submitted successfully"
}
```

### 6.2. Submit Scheduled Job

**Endpoint**: `POST /jobs/scheduled`

**Request Body**:
```json
{
  "queue_name": "report-generation-queue",
  "job_type": "generate_report",
  "payload": {
    "report_type": "daily_sales",
    "date": "2023-01-02"
  },
  "scheduled_for": "2023-01-02T09:00:00Z",
  "priority": 3
}
```

### 6.3. Mendapatkan Job

**Endpoint**: `GET /jobs/{job_id}`

**Response**:
```json
{
  "success": true,
  "data": {
    "id": "uuid-v4",
    "queue_id": "uuid-v4",
    "job_type": "send_email",
    "payload": {},
    "status": "succeeded",
    "priority": 5,
    "max_attempts": 3,
    "attempt_count": 1,
    "output": "Email sent successfully",
    "error_message": null,
    "created_at": "2023-01-01T00:00:00Z",
    "updated_at": "2023-01-01T00:00:05Z",
    "completed_at": "2023-01-01T00:00:05Z",
    "execution_stats": {
      "duration_ms": 2345,
      "worker_id": "uuid-v4"
    }
  }
}
```

### 6.4. Mendapatkan Daftar Job

**Endpoint**: `GET /jobs`

**Parameter Query**:
- `queue_id` (optional): Filter by queue
- `status` (optional): Filter by status (pending, processing, succeeded, failed, cancelled, scheduled)
- `job_type` (optional): Filter by job type
- `page` (optional): Page number
- `limit` (optional): Items per page

**Response**:
```json
{
  "success": true,
  "data": {
    "jobs": [
      {
        "id": "uuid-v4",
        "queue_id": "uuid-v4",
        "job_type": "send_email",
        "status": "succeeded",
        "priority": 5,
        "created_at": "2023-01-01T00:00:00Z"
      }
    ],
    "pagination": {
      "page": 1,
      "limit": 10,
      "total": 1,
      "pages": 1
    }
  }
}
```

### 6.5. Membatalkan Job

**Endpoint**: `POST /jobs/{job_id}/cancel`

**Response**:
```json
{
  "success": true,
  "message": "Job cancelled successfully"
}
```

### 6.6. Meretry Job

**Endpoint**: `POST /jobs/{job_id}/retry`

**Response**:
```json
{
  "success": true,
  "message": "Job retry scheduled successfully"
}
```

## 7. Worker Management API

### 7.1. Mendaftarkan Worker Baru

**Endpoint**: `POST /workers`

**Request Body**:
```json
{
  "name": "email-worker-01",
  "project_id": "uuid-v4",
  "worker_type": "general",
  "capabilities": {
    "supports_email": true,
    "max_concurrent_jobs": 5
  },
  "max_concurrent_jobs": 5
}
```

**Response**:
```json
{
  "success": true,
  "data": {
    "id": "uuid-v4",
    "project_id": "uuid-v4",
    "name": "email-worker-01",
    "worker_type": "general",
    "status": "online",
    "capabilities": {
      "supports_email": true,
      "max_concurrent_jobs": 5
    },
    "max_concurrent_jobs": 5,
    "last_heartbeat": "2023-01-01T00:00:00Z",
    "registered_at": "2023-01-01T00:00:00Z"
  },
  "message": "Worker registered successfully"
}
```

### 7.2. Heartbeat Worker

**Endpoint**: `POST /workers/{worker_id}/heartbeat`

**Response**:
```json
{
  "success": true,
  "data": {
    "worker_id": "uuid-v4",
    "status": "online",
    "last_heartbeat": "2023-01-01T00:00:00Z",
    "next_expected_heartbeat": "2023-01-01T00:01:00Z"
  }
}
```

### 7.3. Mendapatkan Daftar Worker

**Endpoint**: `GET /workers`

**Parameter Query**:
- `project_id` (required): Filter by project
- `worker_type` (optional): Filter by worker type
- `status` (optional): Filter by status (online, offline, draining)

**Response**:
```json
{
  "success": true,
  "data": {
    "workers": [
      {
        "id": "uuid-v4",
        "name": "email-worker-01",
        "worker_type": "general",
        "status": "online",
        "max_concurrent_jobs": 5,
        "active_jobs": 2,
        "last_heartbeat": "2023-01-01T00:00:00Z",
        "uptime_seconds": 3600
      }
    ]
  }
}
```

## 8. API Key Management

### 8.1. Membuat API Key Baru

**Endpoint**: `POST /api-keys`

**Request Body**:
```json
{
  "name": "production-key",
  "project_id": "uuid-v4",
  "permissions": [
    "job:read",
    "job:write",
    "queue:read"
  ],
  "expires_at": "2023-12-31T23:59:59Z"
}
```

**Response**:
```json
{
  "success": true,
  "data": {
    "id": "uuid-v4",
    "project_id": "uuid-v4",
    "name": "production-key",
    "permissions": ["job:read", "job:write", "queue:read"],
    "key": "tf_abcdefghijklmnopqrstuvwxyz123456", // This is the actual key to be used
    "expires_at": "2023-12-31T23:59:59Z",
    "created_at": "2023-01-01T00:00:00Z"
  },
  "message": "API key created successfully"
}
```

**Catatan Penting**: API key hanya ditampilkan sekali saat pembuatan. Simpan dengan aman karena tidak akan ditampilkan lagi.

## 9. Penjadwalan Job API

### 9.1. Membuat Job Terjadwal

**Endpoint**: `POST /scheduled-jobs`

**Request Body**:
```json
{
  "name": "daily-report",
  "queue_name": "report-queue",
  "schedule_type": "cron",
  "schedule_expression": "0 9 * * 1-5",  // Setiap hari Senin-Jumat pukul 09:00
  "job_type": "generate_daily_report",
  "payload": {
    "report_type": "sales"
  },
  "max_runs": 1000,
  "priority": 3
}
```

**Response**:
```json
{
  "success": true,
  "data": {
    "id": "uuid-v4",
    "name": "daily-report",
    "queue_id": "uuid-v4",
    "schedule_type": "cron",
    "schedule_expression": "0 9 * * 1-5",
    "job_type": "generate_daily_report",
    "payload": {"report_type": "sales"},
    "status": "active",
    "max_runs": 1000,
    "runs_count": 0,
    "next_run_at": "2023-01-02T09:00:00Z",
    "last_run_at": null,
    "created_at": "2023-01-01T00:00:00Z"
  },
  "message": "Scheduled job created successfully"
}
```

### 9.2. Mendapatkan Daftar Job Terjadwal

**Endpoint**: `GET /scheduled-jobs`

**Parameter Query**:
- `project_id` (optional): Filter by project
- `status` (optional): Filter by status

## 10. Ketergantungan Job API

### 10.1. Membuat Ketergantungan Job

**Endpoint**: `POST /job-dependencies`

**Request Body**:
```json
{
  "parent_job_id": "uuid-v4",
  "child_job_id": "uuid-v4",
  "dependency_type": "sequential",
  "condition": "succeeded"  // succeeded, failed, always
}
```

## 11. Monitoring dan Metrics API

### 11.1. Mendapatkan Metrics Sistem

**Endpoint**: `GET /metrics/system`

**Response**:
```json
{
  "success": true,
  "data": {
    "system": {
      "cpu_usage_percent": 15.2,
      "memory_usage_percent": 45.8,
      "disk_usage_percent": 62.3,
      "database_connections": 12,
      "database_queue_length": 0,
      "uptime_seconds": 86400
    },
    "queues": {
      "total": 5,
      "active": 4,
      "paused": 1,
      "total_pending_jobs": 23,
      "total_processing_jobs": 7,
      "total_succeeded_jobs": 1542,
      "total_failed_jobs": 12
    },
    "workers": {
      "total": 8,
      "online": 7,
      "offline": 1,
      "draining": 0,
      "total_active_jobs": 15
    }
  }
}
```

### 11.2. Mendapatkan Metrics Queue

**Endpoint**: `GET /metrics/queues/{queue_id}`

**Response**:
```json
{
  "success": true,
  "data": {
    "queue_id": "uuid-v4",
    "name": "email-processing-queue",
    "stats": {
      "pending_jobs": 5,
      "processing_jobs": 2,
      "succeeded_jobs": 100,
      "failed_jobs": 3,
      "avg_processing_time_ms": 1250.5,
      "p95_processing_time_ms": 2100.0,
      "p99_processing_time_ms": 3200.0,
      "throughput_per_minute": 45.2,
      "error_rate_percent": 2.9
    },
    "period": {
      "start": "2023-01-01T00:00:00Z",
      "end": "2023-01-01T01:00:00Z"
    }
  }
}
```

### 11.3. Endpoint Prometheus Metrics

**Endpoint**: `GET /metrics` (format Prometheus)

Output dalam format Prometheus exposition format untuk digunakan dengan Prometheus scraper.

## 12. Error Codes dan Penanganan

### 12.1. Kode Error Umum

| Kode | Deskripsi | Solusi |
|------|-----------|---------|
| `AUTH_001` | Invalid token | Refresh token atau login kembali |
| `AUTH_002` | Token expired | Refresh token atau login kembali |
| `PERM_001` | Insufficient permissions | Periksa role dan permission user |
| `VALIDATION_001` | Validation error | Periksa format dan nilai parameter |
| `RESOURCE_NOT_FOUND` | Resource tidak ditemukan | Pastikan ID atau parameter benar |
| `RATE_LIMIT_EXCEEDED` | Rate limit exceeded | Tunggu dan coba kembali nanti |
| `RESOURCE_CONFLICT` | Konflik resource | Periksa apakah resource sudah ada |

### 12.2. Rate Limiting

API ini menerapkan rate limiting:

- **Per IP**: 1000 permintaan per jam
- **Per API Key**: 5000 permintaan per jam
- **Per endpoint**: Batas spesifik per endpoint

Header yang dikembalikan:
```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 999
X-RateLimit-Reset: 1640995200
```

## 13. Panduan Penggunaan

### 13.1. Contoh Penggunaan dengan cURL

**Mengirim Job Baru:**
```bash
curl -X POST https://api.taskforge.com/api/v1/jobs \
  -H "Authorization: Bearer YOUR_JWT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "queue_name": "default",
    "job_type": "send_notification",
    "payload": {
      "user_id": "123",
      "message": "Hello World"
    }
  }'
```

**Mengecek Status Job:**
```bash
curl -X GET https://api.taskforge.com/api/v1/jobs/JOB_ID \
  -H "Authorization: Bearer YOUR_JWT_TOKEN"
```

### 13.2. Contoh Penggunaan dengan Python

```python
import requests
import json

class TaskForgeClient:
    def __init__(self, base_url, token):
        self.base_url = base_url
        self.token = token
        self.headers = {
            'Authorization': f'Bearer {token}',
            'Content-Type': 'application/json'
        }
    
    def submit_job(self, queue_name, job_type, payload, priority=0):
        url = f"{self.base_url}/api/v1/jobs"
        data = {
            'queue_name': queue_name,
            'job_type': job_type,
            'payload': payload,
            'priority': priority
        }
        
        response = requests.post(url, headers=self.headers, json=data)
        return response.json()
    
    def get_job_status(self, job_id):
        url = f"{self.base_url}/api/v1/jobs/{job_id}"
        response = requests.get(url, headers=self.headers)
        return response.json()
    
    def get_queue_stats(self, queue_id):
        url = f"{self.base_url}/api/v1/metrics/queues/{queue_id}"
        response = requests.get(url, headers=self.headers)
        return response.json()

# Penggunaan
client = TaskForgeClient('https://api.taskforge.com', 'YOUR_JWT_TOKEN')

# Submit job
result = client.submit_job(
    queue_name='email-queue',
    job_type='send_email',
    payload={
        'to': 'user@example.com',
        'subject': 'Test',
        'body': 'Hello World'
    }
)

print(f"Job submitted: {result['data']['id']}")

# Cek status job
job_status = client.get_job_status(result['data']['id'])
print(f"Job status: {job_status['data']['status']}")
```

### 13.3. Contoh Penggunaan dengan Node.js

```javascript
const axios = require('axios');

class TaskForgeClient {
  constructor(baseUrl, token) {
    this.baseUrl = baseUrl;
    this.token = token;
    this.client = axios.create({
      baseURL: `${baseUrl}/api/v1`,
      headers: {
        'Authorization': `Bearer ${token}`,
        'Content-Type': 'application/json'
      }
    });
  }
  
  async submitJob(queueName, jobType, payload, priority = 0) {
    try {
      const response = await this.client.post('/jobs', {
        queue_name: queueName,
        job_type: jobType,
        payload: payload,
        priority: priority
      });
      return response.data;
    } catch (error) {
      throw new Error(`Failed to submit job: ${error.response.data.error.message}`);
    }
  }
  
  async getJobStatus(jobId) {
    try {
      const response = await this.client.get(`/jobs/${jobId}`);
      return response.data;
    } catch (error) {
      throw new Error(`Failed to get job status: ${error.response.data.error.message}`);
    }
  }
  
  async getQueueStats(queueId) {
    try {
      const response = await this.client.get(`/metrics/queues/${queueId}`);
      return response.data;
    } catch (error) {
      throw new Error(`Failed to get queue stats: ${error.response.data.error.message}`);
    }
  }
}

// Penggunaan
const client = new TaskForgeClient('https://api.taskforge.com', 'YOUR_JWT_TOKEN');

// Submit job
client.submitJob('email-queue', 'send_email', {
  to: 'user@example.com',
  subject: 'Test',
  body: 'Hello World'
})
.then(result => {
  console.log(`Job submitted: ${result.data.id}`);
  
  // Cek status job setelah beberapa detik
  setTimeout(async () => {
    try {
      const status = await client.getJobStatus(result.data.id);
      console.log(`Job status: ${status.data.status}`);
    } catch (error) {
      console.error('Error getting job status:', error.message);
    }
  }, 5000);
})
.catch(error => {
  console.error('Error submitting job:', error.message);
});
```

## 14. Best Practices dan Tips

### 14.1. Praktik Terbaik

1. **Gunakan connection pooling** untuk permintaan yang sering
2. **Implementasikan retry dengan exponential backoff** untuk permintaan yang gagal
3. **Gunakan batch processing** untuk mengirim banyak job sekaligus
4. **Monitor rate limits** untuk menghindari pemblokiran
5. **Gunakan idempotency keys** untuk mencegah pengiriman duplikat

### 14.2. Tips Performa

1. **Gunakan queue dengan priority yang sesuai** untuk job-time critical tasks
2. **Gunakan scheduled jobs** untuk tugas rutin daripada polling
3. **Gunakan dead letter queues** untuk menangani job yang tidak bisa diproses
4. **Monitor metrics secara berkala** untuk mendeteksi masalah awal
5. **Gunakan API keys dengan scope yang terbatas** untuk keamanan

### 14.3. Troubleshooting

**Job tidak diproses:**
- Periksa apakah worker tersedia dan online
- Pastikan queue tidak dalam status paused
- Cek apakah ada cukup resource

**Rate limit exceeded:**
- Implementasikan exponential backoff
- Gunakan queue untuk menampung permintaan
- Upgrade plan jika diperlukan

**Token expired:**
- Implementasikan refresh token otomatis
- Gunakan long-lived token untuk service-to-service communication

## 15. SDK dan Library

### 15.1. Official SDKs

TaskForge menyediakan SDK resmi untuk beberapa bahasa:

- **JavaScript/Node.js**: `@taskforge/client`
- **Python**: `taskforge-client`
- **Rust**: `taskforge-sdk`
- **Go**: `taskforge-go`

### 15.2. Instalasi dan Penggunaan SDK

**JavaScript/Node.js:**
```bash
npm install @taskforge/client
```

```javascript
const { TaskForgeClient } = require('@taskforge/client');

const client = new TaskForgeClient({
  baseUrl: 'https://api.taskforge.com',
  token: 'YOUR_JWT_TOKEN'
});

// Submit job
const job = await client.jobs.submit({
  queueName: 'email-queue',
  jobType: 'send_email',
  payload: {
    to: 'user@example.com',
    subject: 'Hello'
  }
});
```

**Python:**
```bash
pip install taskforge-client
```

```python
from taskforge import TaskForgeClient

client = TaskForgeClient(
    base_url='https://api.taskforge.com',
    token='YOUR_JWT_TOKEN'
)

# Submit job
job = client.jobs.submit(
    queue_name='email-queue',
    job_type='send_email',
    payload={
        'to': 'user@example.com',
        'subject': 'Hello'
    }
)
```

## 16. Webhook dan Event Streaming

### 16.1. Konfigurasi Webhook

TaskForge mendukung webhook untuk memberi tahu sistem eksternal tentang event penting:

**Endpoint**: `POST /webhooks`

**Request Body:**
```json
{
  "name": "job-completion-webhook",
  "url": "https://your-app.com/webhooks/taskforge",
  "events": ["job.completed", "job.failed"],
  "secret": "your-webhook-secret",
  "enabled": true
}
```

### 16.2. Format Payload Webhook

```json
{
  "event": "job.completed",
  "timestamp": "2023-01-01T00:00:00Z",
  "data": {
    "job_id": "uuid-v4",
    "queue_id": "uuid-v4",
    "status": "succeeded",
    "output": "Job completed successfully",
    "duration_ms": 1234
  }
}
```

### 16.3. Verifikasi Webhook

Untuk memverifikasi asal webhook, gunakan signature yang disertakan dalam header `X-TaskForge-Signature`:

```
HMAC-SHA256(payload, webhook_secret)
```

## 17. Migration dan Backup

### 17.1. Panduan Migration

Untuk memigrasi dari versi sebelumnya:

1. **Backup data** sebelum melakukan migration
2. **Gunakan migration script** yang disediakan
3. **Verifikasi data** setelah migration
4. **Testing menyeluruh** sebelum production

### 17.2. Backup dan Restore

**Backup:**
```bash
# Backup database
pg_dump taskforge_db > backup.sql

# Backup konfigurasi
tar -czf config-backup.tar.gz config/
```

**Restore:**
```bash
# Restore database
psql taskforge_db < backup.sql
```

## 18. Security dan Compliance

### 18.1. Security Headers

API TaskForge menyertakan security headers berikut:

- `X-Content-Type-Options: nosniff`
- `X-Frame-Options: SAMEORIGIN`
- `X-XSS-Protection: 1; mode=block`
- `Content-Security-Policy`

### 18.2. Compliance

TaskForge mematuhi standar berikut:

- **GDPR**: Hak penghapusan data dan portabilitas data
- **SOC 2**: Keamanan dan availability
- **PCI DSS**: Jika digunakan untuk payment processing

## 19. Dukungan dan Community

### 19.1. Sumber Daya

- **Documentation**: https://docs.taskforge.com
- **API Reference**: https://api.taskforge.com/docs
- **Community Forum**: https://community.taskforge.com
- **GitHub**: https://github.com/taskforge/taskforge

### 19.2. Dukungan

- **Email**: support@taskforge.com
- **Slack**: https://join.slack.com/taskforge-workspace
- **Status Page**: https://status.taskforge.com

### 19.3. Contributing

TaskForge adalah proyek open source. Kontribusi sangat dihargai:

1. Fork repository
2. Buat branch fitur (`git checkout -b feature/amazing-feature`)
3. Commit perubahan (`git commit -m 'Add amazing feature'`)
4. Push ke branch (`git push origin feature/amazing-feature`)
5. Buat Pull Request