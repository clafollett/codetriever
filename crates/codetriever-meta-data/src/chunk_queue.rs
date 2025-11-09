//! PostgreSQL-backed chunk processing queue
//!
//! Implements persistent, distributed chunk queue using SKIP LOCKED pattern for
//! concurrent worker processing. Replaces in-memory queue with crash-recoverable
//! `PostgreSQL` storage.

use crate::error::{DatabaseError, DatabaseErrorExt, DatabaseOperation, DatabaseResult};
use crate::models::ChunkQueueEntry;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Chunk queue operations for distributed embedding workers
#[async_trait]
pub trait ChunkQueue: Send + Sync {
    /// Enqueue chunks for processing
    ///
    /// Chunks are inserted with status='queued' and become immediately available
    /// for workers to claim.
    async fn enqueue_chunks(
        &self,
        job_id: Uuid,
        chunks: Vec<serde_json::Value>,
    ) -> DatabaseResult<()>;

    /// Dequeue chunks for processing (SKIP LOCKED pattern)
    ///
    /// Claims up to `batch_size` chunks atomically, setting:
    /// - status='processing'
    /// - `claimed_at=NOW()`
    /// - `claimed_by=worker_id`
    /// - `visible_after=NOW()` + `visibility_timeout`
    ///
    /// Returns chunk IDs and data for processing.
    async fn dequeue_chunks(
        &self,
        worker_id: &str,
        batch_size: i32,
        visibility_timeout_secs: i32,
    ) -> DatabaseResult<Vec<(Uuid, serde_json::Value)>>;

    /// Acknowledge successful chunk processing
    ///
    /// Marks chunks as status='completed', sets `completed_at` timestamp.
    /// Called after successful embedding + storage.
    async fn ack_chunks(&self, chunk_ids: &[Uuid]) -> DatabaseResult<()>;

    /// Requeue failed chunk for retry
    ///
    /// If `retry_count` < `max_retries`: status='queued', `visible_after=NULL`
    /// If `retry_count` >= `max_retries`: status='failed'
    async fn requeue_chunk(&self, chunk_id: Uuid, error: &str) -> DatabaseResult<()>;

    /// Check if all chunks for a job are completed
    ///
    /// Returns true when COUNT(*) WHERE status IN ('queued', 'processing') = 0
    async fn check_job_complete(&self, job_id: Uuid) -> DatabaseResult<bool>;

    /// Recover timed-out chunks (background cleanup task)
    ///
    /// Finds chunks with status='processing' AND `visible_after` < `NOW()`
    /// Resets them to status='queued' for retry.
    ///
    /// Returns count of recovered chunks.
    async fn recover_timed_out_chunks(&self) -> DatabaseResult<u64>;

    /// Get queue depth for a job (for monitoring)
    async fn get_queue_depth(&self, job_id: Uuid) -> DatabaseResult<QueueDepth>;
}

/// Queue depth statistics for monitoring
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueDepth {
    pub queued: i64,
    pub processing: i64,
    pub completed: i64,
    pub failed: i64,
}

/// `PostgreSQL` implementation of chunk queue
#[derive(Clone)]
pub struct PostgresChunkQueue {
    pool: PgPool,
}

impl PostgresChunkQueue {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ChunkQueue for PostgresChunkQueue {
    async fn enqueue_chunks(
        &self,
        job_id: Uuid,
        chunks: Vec<serde_json::Value>,
    ) -> DatabaseResult<()> {
        let operation = DatabaseOperation::Query {
            description: "enqueue_chunks".to_string(),
        };

        // Batch insert all chunks with status='queued'
        sqlx::query(
            r"
            INSERT INTO chunk_processing_queue (job_id, chunk_data, status)
            SELECT $1, chunk_data, 'queued'
            FROM UNNEST($2::jsonb[]) AS chunk_data
            ",
        )
        .bind(job_id)
        .bind(&chunks)
        .execute(&self.pool)
        .await
        .map_db_err(operation, None)?;

        Ok(())
    }

    async fn dequeue_chunks(
        &self,
        worker_id: &str,
        batch_size: i32,
        visibility_timeout_secs: i32,
    ) -> DatabaseResult<Vec<(Uuid, serde_json::Value)>> {
        let operation = DatabaseOperation::Query {
            description: "dequeue_chunks".to_string(),
        };

        let now = Utc::now();
        #[allow(clippy::arithmetic_side_effects)]
        let visible_after = now + Duration::seconds(i64::from(visibility_timeout_secs));

        // SKIP LOCKED pattern: claim chunks atomically
        let rows = sqlx::query_as::<_, ChunkQueueEntry>(
            r"
            WITH claimed AS (
                SELECT chunk_processing_queue.id
                FROM chunk_processing_queue
                WHERE status = 'queued'
                  AND (visible_after IS NULL OR visible_after <= $1)
                ORDER BY created_at
                LIMIT $2
                FOR UPDATE SKIP LOCKED
            )
            UPDATE chunk_processing_queue
            SET status = 'processing',
                claimed_at = $1,
                claimed_by = $3,
                visible_after = $4
            FROM claimed
            WHERE chunk_processing_queue.id = claimed.id
            RETURNING chunk_processing_queue.id,
                      chunk_processing_queue.job_id,
                      chunk_processing_queue.chunk_data,
                      chunk_processing_queue.status,
                      chunk_processing_queue.claimed_at,
                      chunk_processing_queue.claimed_by,
                      chunk_processing_queue.visible_after,
                      chunk_processing_queue.retry_count,
                      chunk_processing_queue.max_retries,
                      chunk_processing_queue.last_error,
                      chunk_processing_queue.created_at,
                      chunk_processing_queue.completed_at
            ",
        )
        .bind(now)
        .bind(batch_size)
        .bind(worker_id)
        .bind(visible_after)
        .fetch_all(&self.pool)
        .await
        .map_db_err(operation, None)?;

        Ok(rows.into_iter().map(|r| (r.id, r.chunk_data)).collect())
    }

    async fn ack_chunks(&self, chunk_ids: &[Uuid]) -> DatabaseResult<()> {
        let operation = DatabaseOperation::Query {
            description: "ack_chunks".to_string(),
        };

        sqlx::query(
            r"
            UPDATE chunk_processing_queue
            SET status = 'completed',
                completed_at = NOW()
            WHERE id = ANY($1)
            ",
        )
        .bind(chunk_ids)
        .execute(&self.pool)
        .await
        .map_db_err(operation, None)?;

        Ok(())
    }

    async fn requeue_chunk(&self, chunk_id: Uuid, error: &str) -> DatabaseResult<()> {
        let operation = DatabaseOperation::Query {
            description: "requeue_chunk".to_string(),
        };

        sqlx::query(
            r"
            UPDATE chunk_processing_queue
            SET status = CASE
                    WHEN retry_count >= max_retries THEN 'failed'
                    ELSE 'queued'
                END,
                last_error = $2,
                visible_after = NULL,
                retry_count = retry_count + 1
            WHERE id = $1
            ",
        )
        .bind(chunk_id)
        .bind(error)
        .execute(&self.pool)
        .await
        .map_db_err(operation, None)?;

        Ok(())
    }

    async fn check_job_complete(&self, job_id: Uuid) -> DatabaseResult<bool> {
        let operation = DatabaseOperation::Query {
            description: "check_job_complete".to_string(),
        };

        let row = sqlx::query(
            r"
            SELECT COUNT(*) as count
            FROM chunk_processing_queue
            WHERE job_id = $1
              AND status IN ('queued', 'processing')
            ",
        )
        .bind(job_id)
        .fetch_one(&self.pool)
        .await
        .map_db_err(operation, None)?;

        let count: i64 = row.try_get("count").map_err(|e| {
            DatabaseError::query_failed(
                DatabaseOperation::Query {
                    description: "check_job_complete_get_count".to_string(),
                },
                e,
                None,
            )
        })?;

        Ok(count == 0)
    }

    async fn recover_timed_out_chunks(&self) -> DatabaseResult<u64> {
        let operation = DatabaseOperation::Query {
            description: "recover_timed_out_chunks".to_string(),
        };

        let now = Utc::now();

        let result = sqlx::query(
            r"
            UPDATE chunk_processing_queue
            SET status = 'queued',
                claimed_at = NULL,
                claimed_by = NULL,
                visible_after = NULL
            WHERE status = 'processing'
              AND visible_after < $1
            ",
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map_db_err(operation, None)?;

        Ok(result.rows_affected())
    }

    async fn get_queue_depth(&self, job_id: Uuid) -> DatabaseResult<QueueDepth> {
        let operation = DatabaseOperation::Query {
            description: "get_queue_depth".to_string(),
        };

        let row = sqlx::query(
            r"
            SELECT
                COUNT(*) FILTER (WHERE status = 'queued') as queued,
                COUNT(*) FILTER (WHERE status = 'processing') as processing,
                COUNT(*) FILTER (WHERE status = 'completed') as completed,
                COUNT(*) FILTER (WHERE status = 'failed') as failed
            FROM chunk_processing_queue
            WHERE job_id = $1
            ",
        )
        .bind(job_id)
        .fetch_one(&self.pool)
        .await
        .map_db_err(operation, None)?;

        Ok(QueueDepth {
            queued: row.try_get("queued").unwrap_or(0),
            processing: row.try_get("processing").unwrap_or(0),
            completed: row.try_get("completed").unwrap_or(0),
            failed: row.try_get("failed").unwrap_or(0),
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::significant_drop_tightening
)]
mod tests {
    use super::*;
    use serde_json::json;

    type MockChunkStore = std::sync::Arc<std::sync::Mutex<Vec<MockChunk>>>;

    /// Mock implementation for unit testing
    struct MockChunkQueue {
        chunks: MockChunkStore,
    }

    #[derive(Clone, Debug)]
    struct MockChunk {
        id: Uuid,
        job_id: Uuid,
        data: serde_json::Value,
        status: String,
        claimed_by: Option<String>,
        retry_count: i32,
        max_retries: i32,
    }

    impl MockChunkQueue {
        fn new() -> Self {
            Self {
                chunks: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl ChunkQueue for MockChunkQueue {
        async fn enqueue_chunks(
            &self,
            job_id: Uuid,
            chunks: Vec<serde_json::Value>,
        ) -> DatabaseResult<()> {
            let mut store = self.chunks.lock().unwrap();
            for data in chunks {
                store.push(MockChunk {
                    id: Uuid::new_v4(),
                    job_id,
                    data,
                    status: "queued".to_string(),
                    claimed_by: None,
                    retry_count: 0,
                    max_retries: 3,
                });
            }
            Ok(())
        }

        async fn dequeue_chunks(
            &self,
            worker_id: &str,
            batch_size: i32,
            _visibility_timeout_secs: i32,
        ) -> DatabaseResult<Vec<(Uuid, serde_json::Value)>> {
            let mut store = self.chunks.lock().unwrap();
            let mut claimed = Vec::new();

            for chunk in store.iter_mut() {
                #[allow(clippy::cast_sign_loss)]
                if chunk.status == "queued" && claimed.len() < batch_size as usize {
                    chunk.status = "processing".to_string();
                    chunk.claimed_by = Some(worker_id.to_string());
                    claimed.push((chunk.id, chunk.data.clone()));
                }
            }

            Ok(claimed)
        }

        async fn ack_chunks(&self, chunk_ids: &[Uuid]) -> DatabaseResult<()> {
            let mut store = self.chunks.lock().unwrap();
            for chunk in store.iter_mut() {
                if chunk_ids.contains(&chunk.id) {
                    chunk.status = "completed".to_string();
                }
            }
            Ok(())
        }

        async fn requeue_chunk(&self, chunk_id: Uuid, _error: &str) -> DatabaseResult<()> {
            let mut store = self.chunks.lock().unwrap();
            if let Some(chunk) = store.iter_mut().find(|c| c.id == chunk_id) {
                #[allow(clippy::arithmetic_side_effects)]
                {
                    chunk.retry_count += 1;
                }
                if chunk.retry_count >= chunk.max_retries {
                    chunk.status = "failed".to_string();
                } else {
                    chunk.status = "queued".to_string();
                    chunk.claimed_by = None;
                }
            }
            Ok(())
        }

        async fn check_job_complete(&self, job_id: Uuid) -> DatabaseResult<bool> {
            let store = self.chunks.lock().unwrap();
            let pending = store
                .iter()
                .filter(|c| {
                    c.job_id == job_id && (c.status == "queued" || c.status == "processing")
                })
                .count();
            Ok(pending == 0)
        }

        async fn recover_timed_out_chunks(&self) -> DatabaseResult<u64> {
            // Mock doesn't implement timeout recovery
            Ok(0)
        }

        async fn get_queue_depth(&self, job_id: Uuid) -> DatabaseResult<QueueDepth> {
            let store = self.chunks.lock().unwrap();
            #[allow(clippy::cast_possible_wrap)]
            let queued = store
                .iter()
                .filter(|c| c.job_id == job_id && c.status == "queued")
                .count() as i64;
            #[allow(clippy::cast_possible_wrap)]
            let processing = store
                .iter()
                .filter(|c| c.job_id == job_id && c.status == "processing")
                .count() as i64;
            #[allow(clippy::cast_possible_wrap)]
            let completed = store
                .iter()
                .filter(|c| c.job_id == job_id && c.status == "completed")
                .count() as i64;
            #[allow(clippy::cast_possible_wrap)]
            let failed = store
                .iter()
                .filter(|c| c.job_id == job_id && c.status == "failed")
                .count() as i64;

            Ok(QueueDepth {
                queued,
                processing,
                completed,
                failed,
            })
        }
    }

    // ========== UNIT TESTS ==========

    #[tokio::test]
    async fn test_enqueue_chunks() {
        let queue = MockChunkQueue::new();
        let job_id = Uuid::new_v4();
        let chunks = vec![
            json!({"content": "chunk1"}),
            json!({"content": "chunk2"}),
            json!({"content": "chunk3"}),
        ];

        queue.enqueue_chunks(job_id, chunks).await.unwrap();

        let depth = queue.get_queue_depth(job_id).await.unwrap();
        assert_eq!(depth.queued, 3);
        assert_eq!(depth.processing, 0);
        assert_eq!(depth.completed, 0);
    }

    #[tokio::test]
    async fn test_dequeue_chunks() {
        let queue = MockChunkQueue::new();
        let job_id = Uuid::new_v4();
        let chunks = vec![json!({"content": "chunk1"}), json!({"content": "chunk2"})];

        queue.enqueue_chunks(job_id, chunks).await.unwrap();

        // Dequeue 1 chunk
        let claimed = queue.dequeue_chunks("worker-1", 1, 300).await.unwrap();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].1, json!({"content": "chunk1"}));

        let depth = queue.get_queue_depth(job_id).await.unwrap();
        assert_eq!(depth.queued, 1);
        assert_eq!(depth.processing, 1);
    }

    #[tokio::test]
    async fn test_concurrent_dequeue_no_overlap() {
        let queue = MockChunkQueue::new();
        let job_id = Uuid::new_v4();
        let chunks = vec![
            json!({"content": "chunk1"}),
            json!({"content": "chunk2"}),
            json!({"content": "chunk3"}),
        ];

        queue.enqueue_chunks(job_id, chunks).await.unwrap();

        // Two workers dequeue concurrently
        let claimed1 = queue.dequeue_chunks("worker-1", 2, 300).await.unwrap();
        let claimed2 = queue.dequeue_chunks("worker-2", 2, 300).await.unwrap();

        // Worker 1 gets 2 chunks, worker 2 gets 1 chunk
        assert_eq!(claimed1.len(), 2);
        assert_eq!(claimed2.len(), 1);

        // Verify no overlap (all chunks are unique)
        let all_ids: Vec<Uuid> = claimed1
            .iter()
            .chain(claimed2.iter())
            .map(|(id, _)| *id)
            .collect();
        let unique_ids: std::collections::HashSet<_> = all_ids.iter().collect();
        assert_eq!(
            all_ids.len(),
            unique_ids.len(),
            "Workers claimed overlapping chunks!"
        );
    }

    #[tokio::test]
    async fn test_ack_chunks() {
        let queue = MockChunkQueue::new();
        let job_id = Uuid::new_v4();
        let chunks = vec![json!({"content": "chunk1"})];

        queue.enqueue_chunks(job_id, chunks).await.unwrap();
        let claimed = queue.dequeue_chunks("worker-1", 1, 300).await.unwrap();
        let chunk_id = claimed[0].0;

        // Ack the chunk
        queue.ack_chunks(&[chunk_id]).await.unwrap();

        let depth = queue.get_queue_depth(job_id).await.unwrap();
        assert_eq!(depth.queued, 0);
        assert_eq!(depth.processing, 0);
        assert_eq!(depth.completed, 1);
    }

    #[tokio::test]
    async fn test_requeue_chunk_success() {
        let queue = MockChunkQueue::new();
        let job_id = Uuid::new_v4();
        queue
            .enqueue_chunks(job_id, vec![json!({"content": "chunk1"})])
            .await
            .unwrap();

        let claimed = queue.dequeue_chunks("worker-1", 1, 300).await.unwrap();
        let chunk_id = claimed[0].0;

        // Requeue on failure (retry 1)
        queue
            .requeue_chunk(chunk_id, "embedding failed")
            .await
            .unwrap();

        let depth = queue.get_queue_depth(job_id).await.unwrap();
        assert_eq!(depth.queued, 1, "Chunk should be requeued");
        assert_eq!(depth.processing, 0);
        assert_eq!(
            depth.failed, 0,
            "Should not be failed yet (retry_count < max)"
        );
    }

    #[tokio::test]
    async fn test_requeue_chunk_exceeds_max_retries() {
        let queue = MockChunkQueue::new();
        let job_id = Uuid::new_v4();
        queue
            .enqueue_chunks(job_id, vec![json!({"content": "chunk1"})])
            .await
            .unwrap();

        let claimed = queue.dequeue_chunks("worker-1", 1, 300).await.unwrap();
        let chunk_id = claimed[0].0;

        // Retry 3 times (exceeds max_retries=3)
        for _ in 0..3 {
            queue
                .requeue_chunk(chunk_id, "persistent error")
                .await
                .unwrap();
        }

        let depth = queue.get_queue_depth(job_id).await.unwrap();
        assert_eq!(
            depth.failed, 1,
            "Chunk should be marked failed after max retries"
        );
        assert_eq!(depth.queued, 0);
    }

    #[tokio::test]
    async fn test_check_job_complete() {
        let queue = MockChunkQueue::new();
        let job_id = Uuid::new_v4();
        let chunks = vec![json!({"content": "chunk1"}), json!({"content": "chunk2"})];

        queue.enqueue_chunks(job_id, chunks).await.unwrap();

        // Job not complete (chunks still queued)
        assert!(!queue.check_job_complete(job_id).await.unwrap());

        // Process first chunk
        let claimed = queue.dequeue_chunks("worker-1", 2, 300).await.unwrap();
        queue.ack_chunks(&[claimed[0].0]).await.unwrap();

        // Still not complete (1 chunk processing)
        assert!(!queue.check_job_complete(job_id).await.unwrap());

        // Process second chunk
        queue.ack_chunks(&[claimed[1].0]).await.unwrap();

        // Now complete
        assert!(queue.check_job_complete(job_id).await.unwrap());
    }

    #[tokio::test]
    async fn test_queue_depth_tracking() {
        let queue = MockChunkQueue::new();
        let job_id = Uuid::new_v4();
        let chunks = vec![
            json!({"content": "chunk1"}),
            json!({"content": "chunk2"}),
            json!({"content": "chunk3"}),
        ];

        queue.enqueue_chunks(job_id, chunks).await.unwrap();

        // Initial: 3 queued
        let depth = queue.get_queue_depth(job_id).await.unwrap();
        assert_eq!(
            depth,
            QueueDepth {
                queued: 3,
                processing: 0,
                completed: 0,
                failed: 0
            }
        );

        // Claim 2 chunks
        let claimed = queue.dequeue_chunks("worker-1", 2, 300).await.unwrap();
        let depth = queue.get_queue_depth(job_id).await.unwrap();
        assert_eq!(
            depth,
            QueueDepth {
                queued: 1,
                processing: 2,
                completed: 0,
                failed: 0
            }
        );

        // Complete 1 chunk
        queue.ack_chunks(&[claimed[0].0]).await.unwrap();
        let depth = queue.get_queue_depth(job_id).await.unwrap();
        assert_eq!(
            depth,
            QueueDepth {
                queued: 1,
                processing: 1,
                completed: 1,
                failed: 0
            }
        );

        // Fail 1 chunk (exceeds retries)
        for _ in 0..3 {
            queue.requeue_chunk(claimed[1].0, "error").await.unwrap();
        }
        let depth = queue.get_queue_depth(job_id).await.unwrap();
        assert_eq!(
            depth,
            QueueDepth {
                queued: 1,
                processing: 0,
                completed: 1,
                failed: 1
            }
        );
    }
}
