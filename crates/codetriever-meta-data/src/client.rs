//! Database client combining pool manager and repository

use anyhow::Result;

use crate::pool_manager::{PoolConfig, PoolManager};
use crate::repository::DbFileRepository;
use codetriever_config::DatabaseConfig;

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
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database connection fails
    /// - Pool creation fails due to invalid configuration
    /// - Network connectivity issues prevent connection
    pub async fn initialize(config: &DatabaseConfig) -> Result<Self> {
        let pool_config = PoolConfig::default();
        let pools = PoolManager::new(config, pool_config).await?;
        Ok(Self::new(pools))
    }

    /// Get repository for database operations
    pub const fn repository(&self) -> &DbFileRepository {
        &self.repository
    }

    /// Get pool manager
    pub const fn pools(&self) -> &PoolManager {
        &self.pools
    }

    /// Count total project branches
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn count_project_branches(&self) -> Result<i64, crate::DatabaseError> {
        self.repository.count_project_branches().await
    }

    /// Count total indexed files
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn count_indexed_files(&self) -> Result<i64, crate::DatabaseError> {
        self.repository.count_indexed_files().await
    }

    /// Count total chunks
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn count_chunks(&self) -> Result<i64, crate::DatabaseError> {
        self.repository.count_chunks().await
    }
}
