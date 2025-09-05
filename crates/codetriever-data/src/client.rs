//! Database client combining pool and repository

use anyhow::Result;
use sqlx::PgPool;

use crate::config::DatabaseConfig;
use crate::pool::initialize_database;
use crate::repository::DbFileRepository;

/// Database client combining pool and repository
pub struct DataClient {
    pool: PgPool,
    repository: DbFileRepository,
}

impl DataClient {
    /// Create new data client from pool
    pub fn new(pool: PgPool) -> Self {
        let repository = DbFileRepository::new(pool.clone());
        Self { pool, repository }
    }

    /// Initialize with config
    pub async fn initialize(config: &DatabaseConfig) -> Result<Self> {
        let pool = initialize_database(config).await?;
        Ok(Self::new(pool))
    }

    /// Get repository for database operations
    pub fn repository(&self) -> &DbFileRepository {
        &self.repository
    }

    /// Get underlying pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
