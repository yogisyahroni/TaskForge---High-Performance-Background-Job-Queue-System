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

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::Secret;
    
    async fn setup_test_db() -> PgPool {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://postgres:password@localhost/test_taskforge".to_string());
        
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
}