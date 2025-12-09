# Sistem Keamanan dan Enkripsi untuk TaskForge

## 1. Gambaran Umum

Dokumen ini menjelaskan sistem keamanan dan enkripsi dalam aplikasi TaskForge. Sistem ini bertanggung jawab untuk melindungi data sensitif, memastikan komunikasi aman, dan mengimplementasikan berbagai lapisan keamanan untuk menjaga integritas dan kerahasiaan sistem.

## 2. Arsitektur Keamanan

### 2.1. Prinsip Keamanan

TaskForge menerapkan prinsip keamanan berikut:

1. **Defense in Depth**: Lapisan keamanan ganda untuk perlindungan maksimal
2. **Principle of Least Privilege**: Akses hanya diberikan sesuai kebutuhan
3. **Zero Trust**: Tidak ada kepercayaan implisit, semua harus divalidasi
4. **Security by Design**: Keamanan diintegrasikan sejak tahap desain

### 2.2. Model Keamanan

```rust
// File: src/models/security.rs
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "encryption_algorithm", rename_all = "lowercase")]
pub enum EncryptionAlgorithm {
    Aes256Gcm,
    ChaCha20Poly1305,
    Rsa4096,
    Ed25519,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "key_purpose", rename_all = "lowercase")]
pub enum KeyPurpose {
    DataEncryption,
    ApiSigning,
    TokenSigning,
    AuditLogging,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EncryptionKey {
    pub id: Uuid,
    pub key_id: String,              // ID unik untuk key
    pub purpose: KeyPurpose,
    pub algorithm: EncryptionAlgorithm,
    pub key_material: Vec<u8>,       // Material kunci yang dienkripsi
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub activated_at: Option<DateTime<Utc>>,
    pub deactivated_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "audit_event_type", rename_all = "lowercase")]
pub enum AuditEventType {
    Login,
    Logout,
    ApiAccess,
    DataAccess,
    ConfigurationChange,
    UserManagement,
    KeyRotation,
    SecurityIncident,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SecurityAuditLog {
    pub id: Uuid,
    pub event_type: AuditEventType,
    pub user_id: Option<Uuid>,
    pub organization_id: Option<Uuid>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<Uuid>,
    pub action: String,
    pub success: bool,
    pub details: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Type)]
#[sqlx(type_name = "security_policy_type", rename_all = "lowercase")]
pub enum SecurityPolicyType {
    Password,
    Session,
    ApiRateLimit,
    IpWhitelist,
    TwoFactor,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SecurityPolicy {
    pub id: Uuid,
    pub policy_type: SecurityPolicyType,
    pub organization_id: Option<Uuid>,
    pub settings: Value,             // Konfigurasi kebijakan dalam format JSON
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EncryptionKey {
    pub fn new(
        key_id: String,
        purpose: KeyPurpose,
        algorithm: EncryptionAlgorithm,
        key_material: Vec<u8>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            key_id,
            purpose,
            algorithm,
            key_material,
            is_active: false, // Tidak aktif sampai diaktifkan
            created_at: now,
            activated_at: None,
            deactivated_at: None,
            expires_at: None,
            metadata: serde_json::json!({}),
        }
    }
    
    pub fn activate(&mut self) {
        self.is_active = true;
        self.activated_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.deactivated_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
    
    pub fn rotate(&mut self, new_key_material: Vec<u8>) {
        self.key_material = new_key_material;
        self.updated_at = Utc::now();
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.key_id.is_empty() || self.key_id.len() > 100 {
            return Err("Key ID must be between 1 and 100 characters".to_string());
        }
        
        // Validasi panjang material kunci berdasarkan algoritma
        match self.algorithm {
            EncryptionAlgorithm::Aes256Gcm => {
                if self.key_material.len() != 32 { // 256-bit key
                    return Err("AES-256-GCM key must be 32 bytes".to_string());
                }
            },
            EncryptionAlgorithm::ChaCha20Poly1305 => {
                if self.key_material.len() != 32 { // 256-bit key
                    return Err("ChaCha20-Poly1305 key must be 32 bytes".to_string());
                }
            },
            EncryptionAlgorithm::Rsa4096 => {
                if self.key_material.len() < 512 { // Minimal 4096-bit key
                    return Err("RSA-4096 key must be at least 512 bytes".to_string());
                }
            },
            EncryptionAlgorithm::Ed25519 => {
                if self.key_material.len() != 32 { // 256-bit key
                    return Err("Ed25519 key must be 32 bytes".to_string());
                }
            },
        }
        
        Ok(())
    }
    
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }
    
    pub fn is_usable(&self) -> bool {
        self.is_active && !self.is_expired()
    }
}

impl SecurityAuditLog {
    pub fn new(
        event_type: AuditEventType,
        user_id: Option<Uuid>,
        organization_id: Option<Uuid>,
        ip_address: Option<String>,
        user_agent: Option<String>,
        resource_type: Option<String>,
        resource_id: Option<Uuid>,
        action: String,
        success: bool,
        details: Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            user_id,
            organization_id,
            ip_address,
            user_agent,
            resource_type,
            resource_id,
            action,
            success,
            details,
            created_at: Utc::now(),
        }
    }
    
    pub fn validate(&self) -> Result<(), String> {
        if self.action.is_empty() || self.action.len() > 500 {
            return Err("Action must be between 1 and 500 characters".to_string());
        }
        
        if let Some(ip) = &self.ip_address {
            if !is_valid_ip_address(ip) {
                return Err("Invalid IP address format".to_string());
            }
        }
        
        Ok(())
    }
}

fn is_valid_ip_address(ip: &str) -> bool {
    ip.parse::<std::net::IpAddr>().is_ok()
}
```

## 3. Sistem Enkripsi Data

### 3.1. Layanan Enkripsi

```rust
// File: src/services/encryption_service.rs
use crate::models::security::{EncryptionKey, EncryptionAlgorithm, KeyPurpose};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct EncryptionService {
    key_manager: KeyManager,
    active_keys: Arc<RwLock<std::collections::HashMap<KeyPurpose, EncryptionKey>>>,
}

impl EncryptionService {
    pub fn new(key_manager: KeyManager) -> Self {
        Self {
            key_manager,
            active_keys: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }
    
    pub async fn encrypt_data(&self, data: &[u8], purpose: KeyPurpose) -> Result<Vec<u8>, EncryptionError> {
        let key = self.get_active_key(purpose).await?;
        
        match key.algorithm {
            EncryptionAlgorithm::Aes256Gcm => {
                self.encrypt_with_aes256gcm(data, &key.key_material)
            },
            EncryptionAlgorithm::ChaCha20Poly1305 => {
                self.encrypt_with_chacha20poly1305(data, &key.key_material)
            },
            _ => Err(EncryptionError::UnsupportedAlgorithm),
        }
    }
    
    pub async fn decrypt_data(&self, encrypted_data: &[u8], purpose: KeyPurpose) -> Result<Vec<u8>, EncryptionError> {
        let key = self.get_active_key(purpose).await?;
        
        match key.algorithm {
            EncryptionAlgorithm::Aes256Gcm => {
                self.decrypt_with_aes256gcm(encrypted_data, &key.key_material)
            },
            EncryptionAlgorithm::ChaCha20Poly1305 => {
                self.decrypt_with_chacha20poly1305(encrypted_data, &key.key_material)
            },
            _ => Err(EncryptionError::UnsupportedAlgorithm),
        }
    }
    
    async fn get_active_key(&self, purpose: KeyPurpose) -> Result<EncryptionKey, EncryptionError> {
        // Coba dapatkan dari cache dulu
        {
            let keys = self.active_keys.read().await;
            if let Some(key) = keys.get(&purpose) {
                if key.is_usable() {
                    return Ok(key.clone());
                }
            }
        }
        
        // Jika tidak ada di cache atau tidak valid, ambil dari database
        let key = self.key_manager.get_active_key(purpose).await
            .map_err(|_| EncryptionError::KeyNotFound)?;
        
        if !key.is_usable() {
            return Err(EncryptionError::KeyNotUsable);
        }
        
        // Simpan ke cache
        {
            let mut keys = self.active_keys.write().await;
            keys.insert(purpose, key.clone());
        }
        
        Ok(key)
    }
    
    fn encrypt_with_aes256gcm(&self, data: &[u8], key_material: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        use aes_gcm::{Aes256Gcm, KeyInit, AeadInPlace, Nonce};
        use rand::RngCore;
        
        if key_material.len() != 32 {
            return Err(EncryptionError::InvalidKeyLength);
        }
        
        let cipher = Aes256Gcm::new_from_slice(key_material)
            .map_err(|_| EncryptionError::InitializationFailed)?;
        
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let mut buffer = data.to_vec();
        let tag = cipher.encrypt_in_place_detached(nonce, b"", &mut buffer)
            .map_err(|_| EncryptionError::EncryptionFailed)?;
        
        // Gabungkan nonce, tag, dan ciphertext
        let mut result = Vec::with_capacity(nonce_bytes.len() + tag.len() + buffer.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(tag.as_slice());
        result.extend_from_slice(&buffer);
        
        Ok(result)
    }
    
    fn decrypt_with_aes256gcm(&self, encrypted_data: &[u8], key_material: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        use aes_gcm::{Aes256Gcm, KeyInit, AeadInPlace, Nonce};
        
        if key_material.len() != 32 {
            return Err(EncryptionError::InvalidKeyLength);
        }
        
        if encrypted_data.len() < 12 + 16 { // nonce (12) + tag (minimal 16)
            return Err(EncryptionError::InvalidDataLength);
        }
        
        let cipher = Aes256Gcm::new_from_slice(key_material)
            .map_err(|_| EncryptionError::InitializationFailed)?;
        
        let nonce = Nonce::from_slice(&encrypted_data[..12]);
        let tag_len = 16; // Untuk kesederhanaan, kita asumsikan tag 16 byte
        let tag = &encrypted_data[12..12 + tag_len];
        let ciphertext = &encrypted_data[12 + tag_len..];
        
        let mut buffer = ciphertext.to_vec();
        let tag_slice = aes_gcm::Tag::from_slice(tag);
        
        cipher.decrypt_in_place_detached(nonce, b"", &mut buffer, tag_slice)
            .map_err(|_| EncryptionError::DecryptionFailed)?;
        
        Ok(buffer)
    }
    
    fn encrypt_with_chacha20poly1305(&self, data: &[u8], key_material: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        use chacha20poly1305::{ChaCha20Poly1305, KeyInit, AeadInPlace, Nonce};
        use rand::RngCore;
        
        if key_material.len() != 32 {
            return Err(EncryptionError::InvalidKeyLength);
        }
        
        let cipher = ChaCha20Poly1305::new_from_slice(key_material)
            .map_err(|_| EncryptionError::InitializationFailed)?;
        
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        
        let mut buffer = data.to_vec();
        let tag = cipher.encrypt_in_place_detached(nonce, b"", &mut buffer)
            .map_err(|_| EncryptionError::EncryptionFailed)?;
        
        // Gabungkan nonce, tag, dan ciphertext
        let mut result = Vec::with_capacity(nonce_bytes.len() + tag.len() + buffer.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(tag.as_slice());
        result.extend_from_slice(&buffer);
        
        Ok(result)
    }
    
    fn decrypt_with_chacha20poly1305(&self, encrypted_data: &[u8], key_material: &[u8]) -> Result<Vec<u8>, EncryptionError> {
        use chacha20poly1305::{ChaCha20Poly1305, KeyInit, AeadInPlace, Nonce};
        
        if key_material.len() != 32 {
            return Err(EncryptionError::InvalidKeyLength);
        }
        
        if encrypted_data.len() < 12 + 16 { // nonce (12) + tag (minimal 16)
            return Err(EncryptionError::InvalidDataLength);
        }
        
        let cipher = ChaCha20Poly1305::new_from_slice(key_material)
            .map_err(|_| EncryptionError::InitializationFailed)?;
        
        let nonce = Nonce::from_slice(&encrypted_data[..12]);
        let tag_len = 16; // Untuk kesederhanaan, kita asumsikan tag 16 byte
        let tag = &encrypted_data[12..12 + tag_len];
        let ciphertext = &encrypted_data[12 + tag_len..];
        
        let mut buffer = ciphertext.to_vec();
        let tag_slice = chacha20poly1305::Tag::from_slice(tag);
        
        cipher.decrypt_in_place_detached(nonce, b"", &mut buffer, tag_slice)
            .map_err(|_| EncryptionError::DecryptionFailed)?;
        
        Ok(buffer)
    }
    
    pub async fn encrypt_sensitive_fields(&self, mut data: serde_json::Value) -> Result<serde_json::Value, EncryptionError> {
        // Enkripsi field-field sensitif dalam struktur JSON
        self.traverse_and_encrypt(&mut data, 0).await?;
        Ok(data)
    }
    
    async fn traverse_and_encrypt(&self, value: &mut serde_json::Value, depth: usize) -> Result<(), EncryptionError> {
        if depth > 10 { // Mencegah rekursi yang terlalu dalam
            return Ok(());
        }
        
        match value {
            serde_json::Value::String(s) => {
                if self.is_sensitive_field_content(s) {
                    let encrypted = self.encrypt_data(s.as_bytes(), KeyPurpose::DataEncryption).await?;
                    *value = serde_json::Value::String(base64::encode(&encrypted));
                }
            },
            serde_json::Value::Object(obj) => {
                for (key, val) in obj.iter_mut() {
                    if self.is_sensitive_field_name(key) {
                        // Jika ini field sensitif, enkripsi nilainya
                        if let serde_json::Value::String(content) = val {
                            let encrypted = self.encrypt_data(content.as_bytes(), KeyPurpose::DataEncryption).await?;
                            *val = serde_json::Value::String(base64::encode(&encrypted));
                        } else {
                            // Jika bukan string, kita tetap enkripsi representasi JSON-nya
                            let json_str = serde_json::to_string(val)
                                .map_err(|_| EncryptionError::SerializationFailed)?;
                            let encrypted = self.encrypt_data(json_str.as_bytes(), KeyPurpose::DataEncryption).await?;
                            *val = serde_json::Value::String(base64::encode(&encrypted));
                        }
                    } else {
                        self.traverse_and_encrypt(val, depth + 1).await?;
                    }
                }
            },
            serde_json::Value::Array(arr) => {
                for item in arr.iter_mut() {
                    self.traverse_and_encrypt(item, depth + 1).await?;
                }
            },
            _ => {}
        }
        
        Ok(())
    }
    
    fn is_sensitive_field_name(&self, field_name: &str) -> bool {
        let sensitive_fields = [
            "password", "secret", "key", "token", "credential", "api_key",
            "private_key", "certificate", "credit_card", "ssn", "social_security",
            "phone", "email", "address", "payment_info", "bank_account"
        ];
        
        sensitive_fields.iter().any(|&field| {
            field_name.to_lowercase().contains(field)
        })
    }
    
    fn is_sensitive_field_content(&self, content: &str) -> bool {
        // Deteksi apakah konten tampak seperti data sensitif
        // Ini adalah contoh sederhana - dalam implementasi nyata, ini akan lebih kompleks
        
        // Cek apakah terlihat seperti token (panjangnya antara 32-128 karakter dan berisi hex)
        if content.len() >= 32 && content.len() <= 128 && content.chars().all(|c| c.is_ascii_hexdigit()) {
            return true;
        }
        
        // Cek apakah terlihat seperti email
        if content.contains('@') && content.contains('.') && content.len() > 5 && content.len() < 255 {
            return true;
        }
        
        false
    }
}

#[derive(Debug)]
pub enum EncryptionError {
    InitializationFailed,
    EncryptionFailed,
    DecryptionFailed,
    InvalidKeyLength,
    InvalidDataLength,
    KeyNotFound,
    KeyNotUsable,
    UnsupportedAlgorithm,
    SerializationFailed,
    IoError,
}

impl std::fmt::Display for EncryptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncryptionError::InitializationFailed => write!(f, "Encryption initialization failed"),
            EncryptionError::EncryptionFailed => write!(f, "Encryption failed"),
            EncryptionError::DecryptionFailed => write!(f, "Decryption failed"),
            EncryptionError::InvalidKeyLength => write!(f, "Invalid key length"),
            EncryptionError::InvalidDataLength => write!(f, "Invalid data length"),
            EncryptionError::KeyNotFound => write!(f, "Encryption key not found"),
            EncryptionError::KeyNotUsable => write!(f, "Encryption key is not usable"),
            EncryptionError::UnsupportedAlgorithm => write!(f, "Unsupported encryption algorithm"),
            EncryptionError::SerializationFailed => write!(f, "Serialization failed"),
            EncryptionError::IoError => write!(f, "IO error occurred"),
        }
    }
}

impl std::error::Error for EncryptionError {}
```

### 3.2. Key Management System

```rust
// File: src/services/key_management_service.rs
use crate::{
    models::security::{EncryptionKey, KeyPurpose, KeyManager},
    database::Database,
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use chrono::{DateTime, Utc, Duration};
use std::collections::HashMap;

pub struct KeyManager {
    db: Database,
    key_cache: std::sync::Arc<tokio::sync::RwLock<HashMap<String, EncryptionKey>>>,
}

impl KeyManager {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            key_cache: std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }
    
    pub async fn create_encryption_key(&self, purpose: KeyPurpose, algorithm: EncryptionAlgorithm) -> Result<EncryptionKey, SqlxError> {
        // Generate key material
        let key_material = self.generate_key_material(&algorithm).await?;
        
        // Create key ID
        let key_id = format!("tf_{}_{}", purpose.as_str(), Utc::now().timestamp());
        
        let mut key = EncryptionKey::new(key_id, purpose, algorithm, key_material);
        key.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        let created_key = self.db.create_encryption_key(key).await?;
        
        // Update cache
        {
            let mut cache = self.key_cache.write().await;
            cache.insert(created_key.key_id.clone(), created_key.clone());
        }
        
        Ok(created_key)
    }
    
    async fn generate_key_material(&self, algorithm: &EncryptionAlgorithm) -> Result<Vec<u8>, SqlxError> {
        use rand::RngCore;
        
        let mut key_bytes = vec![0u8; match algorithm {
            EncryptionAlgorithm::Aes256Gcm => 32,
            EncryptionAlgorithm::ChaCha20Poly1305 => 32,
            EncryptionAlgorithm::Rsa4096 => 512, // Minimal untuk 4096-bit
            EncryptionAlgorithm::Ed25519 => 32,
        }];
        
        rand::thread_rng().fill_bytes(&mut key_bytes);
        
        Ok(key_bytes)
    }
    
    pub async fn get_active_key(&self, purpose: KeyPurpose) -> Result<EncryptionKey, SqlxError> {
        // Coba dari cache dulu
        {
            let cache = self.key_cache.read().await;
            for (_, key) in cache.iter() {
                if key.purpose == purpose && key.is_active && !key.is_expired() {
                    return Ok(key.clone());
                }
            }
        }
        
        // Jika tidak ditemukan di cache, cari di database
        let key = self.db.get_active_encryption_key(purpose).await?;
        
        if let Some(key) = key {
            // Simpan ke cache
            {
                let mut cache = self.key_cache.write().await;
                cache.insert(key.key_id.clone(), key.clone());
            }
            Ok(key)
        } else {
            Err(SqlxError::RowNotFound)
        }
    }
    
    pub async fn get_key_by_id(&self, key_id: &str) -> Result<Option<EncryptionKey>, SqlxError> {
        // Coba dari cache dulu
        {
            let cache = self.key_cache.read().await;
            if let Some(key) = cache.get(key_id) {
                return Ok(Some(key.clone()));
            }
        }
        
        // Jika tidak ditemukan di cache, cari di database
        let key = self.db.get_encryption_key_by_id(key_id).await?;
        
        if let Some(key) = key {
            // Simpan ke cache
            {
                let mut cache = self.key_cache.write().await;
                cache.insert(key.key_id.clone(), key.clone());
            }
        }
        
        Ok(key)
    }
    
    pub async fn activate_key(&self, key_id: &str) -> Result<(), SqlxError> {
        let mut key = self.get_key_by_id(key_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        key.activate();
        self.db.update_encryption_key(key.clone()).await?;
        
        // Update cache
        {
            let mut cache = self.key_cache.write().await;
            cache.insert(key.key_id.clone(), key);
        }
        
        Ok(())
    }
    
    pub async fn deactivate_key(&self, key_id: &str) -> Result<(), SqlxError> {
        let mut key = self.get_key_by_id(key_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        key.deactivate();
        self.db.update_encryption_key(key.clone()).await?;
        
        // Update cache
        {
            let mut cache = self.key_cache.write().await;
            cache.insert(key.key_id.clone(), key);
        }
        
        Ok(())
    }
    
    pub async fn rotate_key(&self, key_id: &str) -> Result<EncryptionKey, SqlxError> {
        let mut old_key = self.get_key_by_id(key_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        // Nonaktifkan kunci lama
        old_key.deactivate();
        self.db.update_encryption_key(old_key.clone()).await?;
        
        // Buat kunci baru dengan parameter yang sama
        let new_key = self.create_encryption_key(old_key.purpose, old_key.algorithm).await?;
        
        // Aktifkan kunci baru
        let mut new_key_activated = new_key;
        new_key_activated.activate();
        new_key_activated.expires_at = Some(Utc::now() + Duration::days(365)); // Expire dalam 1 tahun
        
        self.db.update_encryption_key(new_key_activated.clone()).await?;
        
        // Update cache
        {
            let mut cache = self.key_cache.write().await;
            cache.insert(old_key.key_id.clone(), old_key);
            cache.insert(new_key_activated.key_id.clone(), new_key_activated.clone());
        }
        
        Ok(new_key_activated)
    }
    
    pub async fn schedule_key_rotation(&self, key_id: &str, rotation_days: i64) -> Result<(), SqlxError> {
        let mut key = self.get_key_by_id(key_id).await?
            .ok_or(SqlxError::RowNotFound)?;
        
        key.expires_at = Some(Utc::now() + Duration::days(rotation_days));
        self.db.update_encryption_key(key).await?;
        
        // Update cache
        {
            let mut cache = self.key_cache.write().await;
            if let Some(cached_key) = cache.get_mut(key_id) {
                cached_key.expires_at = Some(Utc::now() + Duration::days(rotation_days));
            }
        }
        
        Ok(())
    }
    
    pub async fn cleanup_expired_keys(&self) -> Result<(), SqlxError> {
        let expired_keys = self.db.get_expired_encryption_keys(Utc::now()).await?;
        
        for mut key in expired_keys {
            key.deactivate();
            self.db.update_encryption_key(key).await?;
        }
        
        // Bersihkan cache
        {
            let mut cache = self.key_cache.write().await;
            cache.retain(|_, key| key.is_usable());
        }
        
        Ok(())
    }
}

impl KeyPurpose {
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyPurpose::DataEncryption => "data_encryption",
            KeyPurpose::ApiSigning => "api_signing",
            KeyPurpose::TokenSigning => "token_signing",
            KeyPurpose::AuditLogging => "audit_logging",
        }
    }
}
```

## 4. Sistem Keamanan API dan Otentikasi

### 4.1. Layanan Keamanan API

```rust
// File: src/services/api_security_service.rs
use crate::{
    models::security::{SecurityAuditLog, AuditEventType},
    services::{authentication_service::AuthenticationService, encryption_service::EncryptionService},
};
use uuid::Uuid;
use sqlx::Error as SqlxError;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc, Duration};

pub struct ApiSecurityService {
    auth_service: AuthenticationService,
    encryption_service: EncryptionService,
    rate_limiter: Arc<RwLock<HashMap<String, Vec<ApiCallRecord>>>>,
    audit_log_service: AuditLogService,
    security_policies: SecurityPolicyService,
}

pub struct ApiCallRecord {
    pub timestamp: DateTime<Utc>,
    pub ip_address: String,
    pub user_agent: String,
    pub endpoint: String,
    pub method: String,
}

impl ApiSecurityService {
    pub fn new(
        auth_service: AuthenticationService,
        encryption_service: EncryptionService,
        audit_log_service: AuditLogService,
        security_policies: SecurityPolicyService,
    ) -> Self {
        Self {
            auth_service,
            encryption_service,
            rate_limiter: Arc::new(RwLock::new(HashMap::new())),
            audit_log_service,
            security_policies,
        }
    }
    
    pub async fn validate_api_request(
        &self,
        ip_address: &str,
        user_agent: &str,
        endpoint: &str,
        method: &str,
        auth_header: Option<&str>,
    ) -> Result<ValidationResult, ApiSecurityError> {
        // 1. Cek rate limiting
        if !self.check_rate_limit(ip_address, endpoint, method).await {
            self.audit_log_service.log_security_event(
                AuditEventType::ApiAccess,
                None,
                None,
                Some(ip_address.to_string()),
                Some(user_agent.to_string()),
                Some("api_endpoint".to_string()),
                None,
                "rate_limit_exceeded".to_string(),
                false,
                serde_json::json!({
                    "endpoint": endpoint,
                    "method": method,
                    "ip_address": ip_address
                }),
            ).await?;
            
            return Err(ApiSecurityError::RateLimitExceeded);
        }
        
        // 2. Cek IP whitelist jika diperlukan
        if !self.check_ip_whitelist(ip_address).await {
            self.audit_log_service.log_security_event(
                AuditEventType::ApiAccess,
                None,
                None,
                Some(ip_address.to_string()),
                Some(user_agent.to_string()),
                Some("api_endpoint".to_string()),
                None,
                "ip_not_whitelisted".to_string(),
                false,
                serde_json::json!({
                    "endpoint": endpoint,
                    "method": method,
                    "ip_address": ip_address
                }),
            ).await?;
            
            return Err(ApiSecurityError::IpNotAllowed);
        }
        
        // 3. Validasi otentikasi jika diperlukan
        let user_id = if let Some(auth_token) = auth_header {
            if let Some(user) = self.auth_service.validate_token(auth_token).await? {
                Some(user.id)
            } else {
                self.audit_log_service.log_security_event(
                    AuditEventType::ApiAccess,
                    None,
                    None,
                    Some(ip_address.to_string()),
                    Some(user_agent.to_string()),
                    Some("api_endpoint".to_string()),
                    None,
                    "invalid_token".to_string(),
                    false,
                    serde_json::json!({
                        "endpoint": endpoint,
                        "method": method,
                        "ip_address": ip_address
                    }),
                ).await?;
                
                return Err(ApiSecurityError::InvalidToken);
            }
        } else {
            None
        };
        
        // 4. Cek hak akses jika otentikasi berhasil
        if let Some(user_id) = user_id {
            if !self.check_user_permissions(user_id, endpoint, method).await {
                self.audit_log_service.log_security_event(
                    AuditEventType::ApiAccess,
                    Some(user_id),
                    None,
                    Some(ip_address.to_string()),
                    Some(user_agent.to_string()),
                    Some("api_endpoint".to_string()),
                    None,
                    "insufficient_permissions".to_string(),
                    false,
                    serde_json::json!({
                        "endpoint": endpoint,
                        "method": method,
                        "user_id": user_id,
                        "ip_address": ip_address
                    }),
                ).await?;
                
                return Err(ApiSecurityError::InsufficientPermissions);
            }
        }
        
        // 5. Log akses API yang berhasil
        self.audit_log_service.log_security_event(
            AuditEventType::ApiAccess,
            user_id,
            user_id.map(|_| self.get_user_organization(user_id).await.unwrap_or(Uuid::nil())), // Dalam implementasi nyata, ini akan mengambil org ID
            Some(ip_address.to_string()),
            Some(user_agent.to_string()),
            Some("api_endpoint".to_string()),
            None,
            "api_access_granted".to_string(),
            true,
            serde_json::json!({
                "endpoint": endpoint,
                "method": method,
                "user_id": user_id,
                "ip_address": ip_address
            }),
        ).await?;
        
        Ok(ValidationResult {
            is_valid: true,
            user_id,
            ip_address: ip_address.to_string(),
            risk_score: 0.0,
        })
    }
    
    async fn check_rate_limit(&self, ip_address: &str, endpoint: &str, method: &str) -> bool {
        let mut limiter = self.rate_limiter.write().await;
        let key = format!("{}:{}:{}", ip_address, endpoint, method);
        
        let now = Utc::now();
        let window_size = Duration::minutes(1); // 1 menit window
        let max_requests = 100; // Max 100 requests per minute
        
        // Hapus record lama
        if let Some(records) = limiter.get_mut(&key) {
            records.retain(|record| now - record.timestamp < window_size);
            
            if records.len() >= max_requests {
                return false; // Rate limit exceeded
            }
            
            records.push(ApiCallRecord {
                timestamp: now,
                ip_address: ip_address.to_string(),
                user_agent: "".to_string(), // Dalam implementasi nyata, ini akan diisi
                endpoint: endpoint.to_string(),
                method: method.to_string(),
            });
        } else {
            limiter.insert(key, vec![ApiCallRecord {
                timestamp: now,
                ip_address: ip_address.to_string(),
                user_agent: "".to_string(),
                endpoint: endpoint.to_string(),
                method: method.to_string(),
            }]);
        }
        
        true
    }
    
    async fn check_ip_whitelist(&self, ip_address: &str) -> bool {
        // Dalam implementasi nyata, ini akan memeriksa kebijakan IP whitelist dari database
        // Untuk sekarang, kita asumsikan semua IP diperbolehkan
        true
    }
    
    async fn check_user_permissions(&self, user_id: Uuid, endpoint: &str, method: &str) -> bool {
        // Dalam implementasi nyata, ini akan memeriksa permission user dari database
        // Berdasarkan RBAC atau sistem permission lainnya
        // Untuk sekarang, kita asumsikan semua user memiliki akses
        true
    }
    
    async fn get_user_organization(&self, user_id: Uuid) -> Option<Uuid> {
        // Dalam implementasi nyata, ini akan mengambil ID organisasi dari database
        None
    }
    
    pub async fn encrypt_api_response(&self, data: serde_json::Value) -> Result<serde_json::Value, ApiSecurityError> {
        self.encryption_service.encrypt_sensitive_fields(data)
            .await
            .map_err(|e| ApiSecurityError::EncryptionError(e))
    }
    
    pub async fn log_security_incident(&self, incident: SecurityIncident) -> Result<(), SqlxError> {
        self.audit_log_service.log_security_event(
            AuditEventType::SecurityIncident,
            incident.user_id,
            incident.organization_id,
            incident.ip_address,
            incident.user_agent,
            Some("security".to_string()),
            incident.resource_id,
            incident.description,
            incident.is_resolved,
            incident.details,
        ).await?;
        
        Ok(())
    }
}

pub struct ValidationResult {
    pub is_valid: bool,
    pub user_id: Option<Uuid>,
    pub ip_address: String,
    pub risk_score: f64,
}

pub struct SecurityIncident {
    pub user_id: Option<Uuid>,
    pub organization_id: Option<Uuid>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub resource_id: Option<Uuid>,
    pub description: String,
    pub is_resolved: bool,
    pub details: serde_json::Value,
}

#[derive(Debug)]
pub enum ApiSecurityError {
    RateLimitExceeded,
    IpNotAllowed,
    InvalidToken,
    InsufficientPermissions,
    EncryptionError(crate::services::encryption_service::EncryptionError),
    AuditLogError(SqlxError),
}

impl std::fmt::Display for ApiSecurityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiSecurityError::RateLimitExceeded => write!(f, "Rate limit exceeded"),
            ApiSecurityError::IpNotAllowed => write!(f, "IP address not allowed"),
            ApiSecurityError::InvalidToken => write!(f, "Invalid authentication token"),
            ApiSecurityError::InsufficientPermissions => write!(f, "Insufficient permissions"),
            ApiSecurityError::EncryptionError(e) => write!(f, "Encryption error: {}", e),
            ApiSecurityError::AuditLogError(e) => write!(f, "Audit log error: {}", e),
        }
    }
}

impl std::error::Error for ApiSecurityError {}

// Service tambahan untuk audit log
pub struct AuditLogService {
    db: Database, // Gantilah dengan tipe database Anda
}

impl AuditLogService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn log_security_event(
        &self,
        event_type: AuditEventType,
        user_id: Option<Uuid>,
        organization_id: Option<Uuid>,
        ip_address: Option<String>,
        user_agent: Option<String>,
        resource_type: Option<String>,
        resource_id: Option<Uuid>,
        action: String,
        success: bool,
        details: serde_json::Value,
    ) -> Result<(), SqlxError> {
        let log = SecurityAuditLog::new(
            event_type,
            user_id,
            organization_id,
            ip_address,
            user_agent,
            resource_type,
            resource_id,
            action,
            success,
            details,
        );
        
        log.validate().map_err(|e| SqlxError::Decode(e.into()))?;
        
        self.db.create_security_audit_log(log).await?;
        
        Ok(())
    }
    
    pub async fn get_audit_logs(
        &self,
        organization_id: Option<Uuid>,
        event_types: Option<Vec<AuditEventType>>,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> Result<Vec<SecurityAuditLog>, SqlxError> {
        self.db.get_security_audit_logs(organization_id, event_types, start_time, end_time, limit, offset).await
    }
}

// Service untuk kebijakan keamanan
pub struct SecurityPolicyService {
    db: Database, // Gantilah dengan tipe database Anda
}

impl SecurityPolicyService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    pub async fn get_policy_for_organization(
        &self,
        organization_id: Uuid,
        policy_type: crate::models::security::SecurityPolicyType,
    ) -> Result<Option<crate::models::security::SecurityPolicy>, SqlxError> {
        self.db.get_security_policy(organization_id, policy_type).await
    }
    
    pub async fn evaluate_password_policy(&self, password: &str, organization_id: Option<Uuid>) -> Result<PasswordPolicyResult, SqlxError> {
        let policy = self.get_policy_for_organization(
            organization_id.unwrap_or(Uuid::nil()), // Dalam implementasi nyata, ini akan menggunakan org ID yang valid
            crate::models::security::SecurityPolicyType::Password,
        ).await?;
        
        let mut result = PasswordPolicyResult::default();
        
        if let Some(policy) = policy {
            // Evaluasi password berdasarkan kebijakan
            let settings = &policy.settings;
            
            // Cek panjang minimum
            if let Some(min_length) = settings.get("min_length").and_then(|v| v.as_i64()) {
                if password.len() < min_length as usize {
                    result.errors.push(format!("Password must be at least {} characters long", min_length));
                    result.is_valid = false;
                }
            }
            
            // Cek kompleksitas
            if settings.get("require_uppercase").and_then(|v| v.as_bool()).unwrap_or(false) {
                if !password.chars().any(|c| c.is_uppercase()) {
                    result.errors.push("Password must contain at least one uppercase letter".to_string());
                    result.is_valid = false;
                }
            }
            
            if settings.get("require_lowercase").and_then(|v| v.as_bool()).unwrap_or(false) {
                if !password.chars().any(|c| c.is_lowercase()) {
                    result.errors.push("Password must contain at least one lowercase letter".to_string());
                    result.is_valid = false;
                }
            }
            
            if settings.get("require_numbers").and_then(|v| v.as_bool()).unwrap_or(false) {
                if !password.chars().any(|c| c.is_numeric()) {
                    result.errors.push("Password must contain at least one number".to_string());
                    result.is_valid = false;
                }
            }
            
            if settings.get("require_symbols").and_then(|v| v.as_bool()).unwrap_or(false) {
                if !password.chars().any(|c| !c.is_alphanumeric()) {
                    result.errors.push("Password must contain at least one symbol".to_string());
                    result.is_valid = false;
                }
            }
        } else {
            // Kebijakan default
            if password.len() < 8 {
                result.errors.push("Password must be at least 8 characters long".to_string());
                result.is_valid = false;
            }
        }
        
        Ok(result)
    }
}

#[derive(Default)]
pub struct PasswordPolicyResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
}

impl PasswordPolicyResult {
    pub fn new() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
        }
    }
}
```

## 5. Transport Layer Security (TLS)

### 5.1. TLS Configuration

```rust
// File: src/services/tls_service.rs
use rustls::{ServerConfig, Certificate, PrivateKey, RootCertStore, ClientCertVerified};
use std::sync::Arc;
use std::fs::File;
use std::io::BufReader;

pub struct TlsService {
    server_config: Arc<ServerConfig>,
}

impl TlsService {
    pub fn new(cert_path: &str, key_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let cert_chain = load_certs(cert_path)?;
        let private_key = load_private_key(key_path)?;
        
        let server_config = ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth() // Untuk server TLS
            .with_single_cert(cert_chain, private_key)
            .map_err(|_| "Failed to create server config")?;
        
        Ok(Self {
            server_config: Arc::new(server_config),
        })
    }
    
    pub fn get_server_config(&self) -> Arc<ServerConfig> {
        Arc::clone(&self.server_config)
    }
}

fn load_certs(filename: &str) -> Result<Vec<Certificate>, Box<dyn std::error::Error>> {
    let certfile = File::open(filename)?;
    let mut reader = BufReader::new(certfile);
    
    rustls_pemfile::certs(&mut reader)
        .map(|certs| certs.into_iter().map(Certificate).collect())
        .map_err(|_| "Failed to load certificate".into())
}

fn load_private_key(filename: &str) -> Result<PrivateKey, Box<dyn std::error::Error>> {
    let keyfile = File::open(filename)?;
    let mut reader = BufReader::new(keyfile);
    
    loop {
        match rustls_pemfile::read_one(&mut reader)? {
            Some(rustls_pemfile::Item::RSAKey(key)) => return Ok(PrivateKey(key)),
            Some(rustls_pemfile::Item::PKCS8Key(key)) => return Ok(PrivateKey(key)),
            Some(rustls_pemfile::Item::ECKey(key)) => return Ok(PrivateKey(key)),
            None => break,
            _ => {}
        }
    }
    
    Err("No private key found".into())
}

// Middleware untuk memastikan koneksi TLS
pub struct TlsRequiredMiddleware;

impl TlsRequiredMiddleware {
    pub fn new() -> Self {
        Self
    }
    
    pub fn check_tls(&self, headers: &axum::http::HeaderMap) -> Result<(), axum::http::StatusCode> {
        // Cek header untuk memastikan koneksi menggunakan TLS
        if headers.get("x-forwarded-proto").and_then(|hv| hv.to_str().ok()) == Some("https") ||
           headers.get("x-forwarded-ssl").and_then(|hv| hv.to_str().ok()) == Some("on") {
            Ok(())
        } else {
            Err(axum::http::StatusCode::UPGRADE_REQUIRED)
        }
    }
}
```

## 6. Best Practices dan Rekomendasi

### 6.1. Praktik Terbaik untuk Keamanan

1. **Gunakan enkripsi end-to-end** - untuk semua data sensitif
2. **Terapkan prinsip least privilege** - untuk akses sistem dan data
3. **Gunakan key rotation secara berkala** - untuk mencegah eksploitasi kunci
4. **Implementasikan audit trail yang komprehensif** - untuk keperluan forensik
5. **Gunakan rate limiting yang bijak** - untuk mencegah abuse dan DoS

### 6.2. Praktik Terbaik untuk Enkripsi

1. **Gunakan algoritma enkripsi yang kuat** - seperti AES-256-GCM atau ChaCha20-Poly1305
2. **Jangan pernah menyimpan kunci dalam bentuk plaintext** - dalam kode atau konfigurasi
3. **Gunakan hardware security modules (HSM)** - untuk perlindungan kunci yang lebih kuat
4. **Implementasikan key derivation functions** - untuk menghasilkan kunci dari password
5. **Gunakan envelope encryption** - untuk efisiensi enkripsi data besar

### 6.3. Skalabilitas dan Kinerja

1. **Gunakan caching untuk kunci aktif** - untuk mengurangi akses database
2. **Gunakan connection pooling** - untuk efisiensi koneksi enkripsi
3. **Implementasikan bulk encryption/decryption** - untuk efisiensi proses data besar
4. **Gunakan hardware acceleration** - untuk operasi kriptografi jika tersedia
5. **Gunakan streaming encryption** - untuk data yang sangat besar