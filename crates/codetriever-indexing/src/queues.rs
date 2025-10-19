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

/// Wrapper for chunks with their file generation metadata
#[derive(Clone, Debug)]
pub struct ChunkWithMetadata {
    pub chunk: CodeChunk,
    pub generation: i64,
    pub file_chunk_index: usize, // Stable index within the file (assigned by parser)
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

/// In-memory file content queue (unbounded - accepts all incoming files)
pub struct InMemoryFileQueue {
    tx: FileQueueSender,
    rx: FileQueueReceiver,
    len: Arc<std::sync::atomic::AtomicUsize>,
}

impl InMemoryFileQueue {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            tx: Arc::new(tokio::sync::Mutex::new(Some(tx))),
            rx: Arc::new(tokio::sync::Mutex::new(rx)),
            len: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl FileContentQueue for InMemoryFileQueue {
    async fn push(&self, file: FileContent) -> QueueResult<()> {
        let tx_guard = self.tx.lock().await;
        if let Some(tx) = tx_guard.as_ref() {
            tx.send(file).map_err(|_| QueueError::Closed)?;
            self.len.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        } else {
            Err(QueueError::Closed)
        }
    }

    async fn pop(&self) -> QueueResult<FileContent> {
        let mut rx = self.rx.lock().await;
        let file = rx.recv().await.ok_or(QueueError::Closed)?;
        self.len.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        Ok(file)
    }

    fn close(&self) {
        // Drop the sender to close the channel
        // Use try_lock since this is called from sync context
        if let Ok(mut tx) = self.tx.try_lock() {
            *tx = None;
        }
    }

    fn len(&self) -> usize {
        self.len.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// In-memory chunk queue (bounded - applies back pressure when full)
pub struct InMemoryChunkQueue {
    tx: ChunkQueueSender,
    rx: ChunkQueueReceiver,
    capacity: usize,
    len: Arc<std::sync::atomic::AtomicUsize>,
}

impl InMemoryChunkQueue {
    /// Create a new bounded chunk queue
    ///
    /// # Arguments
    /// * `capacity` - Maximum chunks queue can hold before blocking producers
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity);
        Self {
            tx: Arc::new(tokio::sync::Mutex::new(Some(tx))),
            rx: Arc::new(tokio::sync::Mutex::new(rx)),
            capacity,
            len: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Get queue capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[async_trait]
impl ChunkQueue for InMemoryChunkQueue {
    async fn push_batch(&self, chunks: Vec<ChunkWithMetadata>) -> QueueResult<()> {
        let count = chunks.len();
        let tx_guard = self.tx.lock().await;
        if let Some(tx) = tx_guard.as_ref() {
            for chunk in chunks {
                // This blocks if queue is full (back pressure!)
                tx.send(chunk).await.map_err(|_| QueueError::Closed)?;
            }
            self.len
                .fetch_add(count, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        } else {
            Err(QueueError::Closed)
        }
    }

    fn close(&self) {
        if let Ok(mut tx) = self.tx.try_lock() {
            *tx = None;
        }
    }

    async fn pop_batch(&self, max_count: usize) -> QueueResult<Vec<ChunkWithMetadata>> {
        let mut rx = self.rx.lock().await;
        let mut batch = Vec::with_capacity(max_count);

        // Get first chunk (blocking)
        if let Some(chunk) = rx.recv().await {
            batch.push(chunk);
        } else {
            return Err(QueueError::Closed);
        }

        // Try to get more chunks without blocking (up to max_count)
        while batch.len() < max_count {
            match rx.try_recv() {
                Ok(chunk) => batch.push(chunk),
                Err(mpsc::error::TryRecvError::Empty) => break, // No more ready
                Err(mpsc::error::TryRecvError::Disconnected) => break, // Channel closed
            }
        }

        let batch_len = batch.len();
        self.len
            .fetch_sub(batch_len, std::sync::atomic::Ordering::Relaxed);

        Ok(batch)
    }

    fn len(&self) -> usize {
        self.len.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Default for InMemoryFileQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_file_queue_push_pop() {
        let queue = InMemoryFileQueue::new();
        let file = FileContent {
            path: "test.rs".to_string(),
            content: "fn main() {}".to_string(),
            hash: "abc123".to_string(),
        };

        queue.push(file.clone()).await.unwrap();
        assert_eq!(queue.len(), 1);

        let popped = queue.pop().await.unwrap();
        assert_eq!(popped.path, "test.rs");
        assert_eq!(queue.len(), 0);
    }

    #[tokio::test]
    async fn test_chunk_queue_batching() {
        let queue = InMemoryChunkQueue::new(100);

        let chunks: Vec<ChunkWithMetadata> = (0..10)
            .map(|i| ChunkWithMetadata {
                chunk: CodeChunk {
                    file_path: "test.rs".to_string(),
                    content: format!("chunk {i}"),
                    start_line: i,
                    end_line: i + 1,
                    byte_start: i * 100,
                    byte_end: (i + 1) * 100,
                    language: "rust".to_string(),
                    kind: Some("function".to_string()),
                    name: Some(format!("fn_{i}")),
                    token_count: Some(50),
                    embedding: None,
                },
                generation: 1,
                file_chunk_index: i,
            })
            .collect();

        queue.push_batch(chunks).await.unwrap();

        // Pop 5 chunks
        let batch = queue.pop_batch(5).await.unwrap();
        assert_eq!(batch.len(), 5);
        assert_eq!(batch[0].chunk.content, "chunk 0");

        // Pop remaining 5
        let batch2 = queue.pop_batch(10).await.unwrap();
        assert_eq!(batch2.len(), 5); // Only 5 left
    }

    #[tokio::test]
    async fn test_chunk_queue_back_pressure() {
        let queue = InMemoryChunkQueue::new(2); // Small capacity

        let chunks: Vec<ChunkWithMetadata> = vec![
            ChunkWithMetadata {
                chunk: CodeChunk {
                    file_path: "test.rs".to_string(),
                    content: "chunk1".to_string(),
                    start_line: 1,
                    end_line: 2,
                    byte_start: 0,
                    byte_end: 100,
                    language: "rust".to_string(),
                    kind: None,
                    name: None,
                    token_count: Some(50),
                    embedding: None,
                },
                generation: 1,
                file_chunk_index: 0,
            };
            5 // Try to push 5 chunks into capacity-2 queue
        ];

        // This should work but block until space available
        let push_handle = tokio::spawn({
            let queue = Arc::new(queue);
            let chunks = chunks.clone();
            async move { queue.push_batch(chunks).await }
        });

        // Give it time to fill queue
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Push should still be blocked (queue full)
        assert!(!push_handle.is_finished());

        // Note: This test would need a consumer to complete
        // For now, just verify the blocking behavior exists
    }
}
