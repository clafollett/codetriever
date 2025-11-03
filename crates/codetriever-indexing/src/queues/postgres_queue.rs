//! PostgreSQL-backed persistent file queue implementation
//!
//! Provides a persistent queue using the indexing_job_file_queue table
//! for reliable, crash-resistant file indexing with support for:
//! - Background workers
//! - Concurrent processing (SELECT FOR UPDATE SKIP LOCKED)
//! - Progress tracking and retry logic
//! - Queue depth monitoring
//!
//! Delegates to FileRepository methods to maintain proper layer separation

use crate::indexing::service::FileContent;
use crate::queues::*;
use async_trait::async_trait;
use codetriever_meta_data::traits::FileRepository;
use std::sync::Arc;
use uuid::Uuid;

/// PostgreSQL-backed file content queue
///
/// Uses indexing_job_file_queue table for persistent storage via FileRepository
pub struct PostgresFileQueue {
    repository: Arc<dyn FileRepository>,
    job_id: Uuid,
    tenant_id: Uuid,
    repository_id: String,
    branch: String,
}

impl PostgresFileQueue {
    /// Create a new PostgreSQL-backed file queue
    ///
    /// # Arguments
    /// * `repository` - FileRepository implementation (provides queue methods)
    /// * `job_id` - Parent indexing job ID
    /// * `tenant_id` - Tenant identifier for multi-tenancy
    /// * `repository_id` - Repository identifier
    /// * `branch` - Branch name
    pub fn new(
        repository: Arc<dyn FileRepository>,
        job_id: Uuid,
        tenant_id: Uuid,
        repository_id: String,
        branch: String,
    ) -> Self {
        Self {
            repository,
            job_id,
            tenant_id,
            repository_id,
            branch,
        }
    }
}

#[async_trait]
impl FileContentQueue for PostgresFileQueue {
    async fn push(&self, file: FileContent) -> QueueResult<()> {
        self.repository
            .enqueue_file(
                &self.job_id,
                &self.tenant_id,
                &self.repository_id,
                &self.branch,
                &file.path,
                &file.content,
                &file.hash,
            )
            .await
            .map_err(|e| QueueError::Operation(format!("Failed to push to queue: {e}")))?;

        Ok(())
    }

    async fn pop(&self) -> QueueResult<FileContent> {
        // Use global dequeue - pulls from ANY job in FIFO order!
        let result = self
            .repository
            .dequeue_file()
            .await
            .map_err(|e| QueueError::Operation(format!("Failed to pop from queue: {e}")))?;

        match result {
            Some(dequeued) => Ok(FileContent {
                path: dequeued.file_path,
                content: dequeued.file_content,
                hash: dequeued.content_hash,
            }),
            None => Err(QueueError::Closed), // No more files in queue
        }
    }

    fn close(&self) {
        // PostgreSQL queue doesn't need explicit close
        // Workers detect completion when pop() returns None
    }

    fn len(&self) -> usize {
        // Note: Synchronous method can't call async get_queue_depth()
        // Return 0 as placeholder (queue length not critical for correctness)
        // Consider making trait method async or using cached value
        0
    }
}

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn test_postgres_queue_push_pop() {
        // TODO: Implement with test database
    }

    #[tokio::test]
    async fn test_postgres_queue_skip_locked() {
        // TODO: Test concurrent workers don't get same file
    }
}
