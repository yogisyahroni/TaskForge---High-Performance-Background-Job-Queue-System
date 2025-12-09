pub mod connection;
pub mod migrations;
pub mod repositories;

use sqlx::{PgPool, Row};
use std::sync::Arc;
use secrecy::Secret;

pub struct Database {
    pool: PgPool,
}

impl Database {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
    
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
    
    pub async fn health_check(&self) -> Result<bool, sqlx::Error> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map(|_| true)
            .map_err(|e| e)
    }
    
    pub async fn get_connection_info(&self) -> Result<ConnectionInfo, sqlx::Error> {
        let row = sqlx::query(
            "SELECT version(), current_database(), current_user, inet_client_addr(), inet_client_port(), inet_server_addr(), inet_server_port()"
        )
        .fetch_one(&self.pool)
        .await?;
        
        Ok(ConnectionInfo {
            server_version: row.get(0),
            database_name: row.get(1),
            user: row.get(2),
            client_address: row.get(3),
            client_port: row.get(4),
            server_address: row.get(5),
            server_port: row.get(6),
        })
    }
}

pub struct ConnectionInfo {
    pub server_version: String,
    pub database_name: String,
    pub user: String,
    pub client_address: Option<std::net::IpAddr>,
    pub client_port: Option<u16>,
    pub server_address: std::net::IpAddr,
    pub server_port: u16,
}

pub async fn establish_connection(database_url: &Secret<String>) -> Result<PgPool, sqlx::Error> {
    let opts = database_url.expose_secret().parse::<sqlx::postgres::PgConnectOptions>()?;
    
    let pool = PgPool::connect_with(opts)
        .await?;
    
    Ok(pool)
}

pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await?;
    
    Ok(())
}

pub async fn create_database_if_not_exists(database_url: &str) -> Result<(), sqlx::Error> {
    // Parse database URL untuk mendapatkan informasi koneksi
    let mut url = url::Url::parse(database_url)
        .map_err(|e| sqlx::Error::Configuration(e.into()))?;
    
    let database_name = url.path().trim_start_matches('/');
    url.set_path("/postgres"); // Connect to default postgres database first
    
    let connection = sqlx::PgConnection::connect(url.as_str()).await?;
    
    // Periksa apakah database sudah ada
    let exists: (bool,) = sqlx::query_as("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
        .bind(database_name)
        .fetch_one(&connection)
        .await?;
    
    if !exists.0 {
        // Buat database jika belum ada
        sqlx::query(&format!("CREATE DATABASE {}", database_name))
            .execute(&connection)
            .await?;
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;
    use secrecy::Secret;
    
    async fn setup_test_db() -> PgPool {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:password@localhost/test_taskforge".to_string());
        
        let pool = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to test database");
        
        // Jalankan migrasi untuk database test
        run_migrations(&pool).await.unwrap();
        
        pool
    }
    
    #[tokio::test]
    async fn test_database_connection() {
        let pool = setup_test_db().await;
        let db = Database::new(pool);
        
        assert!(db.health_check().await.is_ok());
    }
    
    #[tokio::test]
    async fn test_get_connection_info() {
        let pool = setup_test_db().await;
        let db = Database::new(pool);
        
        let info = db.get_connection_info().await.unwrap();
        assert!(!info.database_name.is_empty());
        assert!(!info.user.is_empty());
    }
}