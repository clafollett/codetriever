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

    /// Get database size in megabytes
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn get_database_size_mb(&self) -> Result<f64, crate::DatabaseError> {
        self.repository.get_database_size_mb().await
    }

    /// Get most recent indexed timestamp across all project branches
    ///
    /// Returns `None` if no branches have been indexed yet
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn get_last_indexed_timestamp(
        &self,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, crate::DatabaseError> {
        self.repository.get_last_indexed_timestamp().await
    }

    /// Get full file content by path
    ///
    /// Returns `None` if file is not in the index
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    /// Returns tuple of (`repository_id`, branch, content) if found
    pub async fn get_file_content(
        &self,
        repository_id: Option<&str>,
        branch: Option<&str>,
        file_path: &str,
    ) -> Result<Option<(String, String, String)>, crate::DatabaseError> {
        self.repository
            .get_file_content(repository_id, branch, file_path)
            .await
    }

    /// Enqueue a file for indexing (persistent queue)
    ///
    /// # Errors
    ///
    /// Returns error if database insert fails
    pub async fn enqueue_file(
        &self,
        job_id: &uuid::Uuid,
        tenant_id: &uuid::Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        file_content: &str,
        content_hash: &str,
    ) -> Result<(), crate::DatabaseError> {
        self.repository
            .enqueue_file(
                job_id,
                tenant_id,
                repository_id,
                branch,
                file_path,
                file_content,
                content_hash,
            )
            .await
    }

    /// Dequeue next file from global queue (atomic operation)
    ///
    /// Returns file from ANY tenant - `tenant_id` is in the returned file payload.
    /// Returns `None` if no files available in queue.
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn dequeue_file(
        &self,
    ) -> Result<Option<crate::models::DequeuedFile>, crate::DatabaseError> {
        self.repository.dequeue_file().await
    }

    /// Get queue depth for a job
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn get_queue_depth(&self, job_id: &uuid::Uuid) -> Result<i64, crate::DatabaseError> {
        self.repository.get_queue_depth(job_id).await
    }
}
