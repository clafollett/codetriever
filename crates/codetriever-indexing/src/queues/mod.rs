//! Queue abstractions for asynchronous indexing pipeline
//!
//! This module provides queue traits and implementations for the indexing pipeline:
//! 1. FileContentQueue: Holds files awaiting parsing
//! 2. ChunkQueue: Holds parsed chunks awaiting embedding
//!
//! Current implementation uses in-memory channels (fast, simple).
//! Future: PostgreSQL-backed queues for persistence (Issue #35).

use crate::indexing::service::FileContent;
use async_trait::async_trait;
use codetriever_parsing::CodeChunk;
use std::sync::Arc;
use tokio::sync::mpsc;

pub mod in_memory_queue;
pub mod postgres_queue;

// Re-export queue implementations
pub use in_memory_queue::{InMemoryChunkQueue, InMemoryFileQueue};
pub use postgres_queue::PostgresFileQueue;

/// Wrapper for chunks with their file generation metadata
///
/// Contains all context needed for embedding workers to process and store chunks
/// without additional database lookups.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ChunkWithMetadata {
    // Chunk data
    pub chunk: CodeChunk,
    pub generation: i64,
    pub file_chunk_index: usize, // Stable index within the file (assigned by parser)

    // Job context (needed for progress tracking and storage)
    pub job_id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub repository_id: String,
    pub branch: String,
    pub vector_namespace: String,

    // Commit metadata (needed for ChunkStorageContext)
    pub repository_url: String,
    pub commit_sha: String,
    pub commit_message: String,
    pub commit_date: chrono::DateTime<chrono::Utc>,
    pub author: String,
}

/// Result type for queue operations
pub type QueueResult<T> = Result<T, QueueError>;

/// Queue operation errors
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("Queue is closed")]
    Closed,

    #[error("Queue is full")]
    Full,

    #[error("Queue operation failed: {0}")]
    Operation(String),
}

/// Queue for file content awaiting parsing
#[async_trait]
pub trait FileContentQueue: Send + Sync {
    /// Add a file to the queue
    async fn push(&self, file: FileContent) -> QueueResult<()>;

    /// Take a file from the queue (blocks if empty)
    async fn pop(&self) -> QueueResult<FileContent>;

    /// Close the queue (signals no more items coming)
    fn close(&self);

    /// Get current queue length
    fn len(&self) -> usize;

    /// Check if queue is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Queue for parsed chunks awaiting embedding
#[async_trait]
pub trait ChunkQueue: Send + Sync {
    /// Add chunks to the queue (blocks if full - back pressure!)
    async fn push_batch(&self, chunks: Vec<ChunkWithMetadata>) -> QueueResult<()>;

    /// Take up to N chunks from the queue (blocks if empty, returns fewer if available)
    async fn pop_batch(&self, max_count: usize) -> QueueResult<Vec<ChunkWithMetadata>>;

    /// Close the queue (signals no more items coming)
    fn close(&self);

    /// Get current queue length
    fn len(&self) -> usize;

    /// Check if queue is empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// Type aliases to reduce complexity warnings
type FileQueueSender = Arc<tokio::sync::Mutex<Option<mpsc::UnboundedSender<FileContent>>>>;
type FileQueueReceiver = Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<FileContent>>>;
type ChunkQueueSender = Arc<tokio::sync::Mutex<Option<mpsc::Sender<ChunkWithMetadata>>>>;
type ChunkQueueReceiver = Arc<tokio::sync::Mutex<mpsc::Receiver<ChunkWithMetadata>>>;
