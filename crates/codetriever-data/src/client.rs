//! Database client combining pool manager and repository

use anyhow::Result;

use crate::config::DatabaseConfig;
use crate::pool_manager::{PoolConfig, PoolManager};
use crate::repository::DbFileRepository;

/// Database client combining pool manager and repository
pub struct DataClient {
    pools: PoolManager,
    repository: DbFileRepository,
}

impl DataClient {
    /// Create new data client from pool manager
    pub fn new(pools: PoolManager) -> Self {
        let repository = DbFileRepository::new(pools.clone());
        Self { pools, repository }
    }

    /// Initialize with config using optimized pools
    pub async fn initialize(config: &DatabaseConfig) -> Result<Self> {
        let pool_config = PoolConfig::default();
        let pools = PoolManager::new(&config.url, pool_config).await?;
        Ok(Self::new(pools))
    }

    /// Get repository for database operations
    pub fn repository(&self) -> &DbFileRepository {
        &self.repository
    }

    /// Get pool manager
    pub fn pools(&self) -> &PoolManager {
        &self.pools
    }
}
