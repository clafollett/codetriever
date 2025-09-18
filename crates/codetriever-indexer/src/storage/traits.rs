//! Storage abstraction traits for vector databases
//!
//! This module provides trait abstractions for vector storage backends,
//! enabling pluggable storage implementations and better testability.

use crate::{Result, parsing::CodeChunk};
use async_trait::async_trait;
use uuid::Uuid;

/// Search result with similarity score from storage
#[derive(Debug, Clone)]
pub struct StorageSearchResult {
    pub chunk: CodeChunk,
    pub similarity: f32,
}

/// Trait for vector storage backends
///
/// This trait abstracts vector database operations, allowing different
/// implementations (Qdrant, Pinecone, Weaviate, etc.) to be used interchangeably.
#[async_trait]
pub trait VectorStorage: Send + Sync {
    /// Store code chunks with their embeddings
    ///
    /// Returns the number of chunks successfully stored
    async fn store_chunks(&self, chunks: &[CodeChunk]) -> Result<usize>;

    /// Store chunks with predetermined IDs (for deterministic storage)
    ///
    /// This is useful for generation-based versioning where chunk IDs
    /// need to be predictable for deletion operations.
    async fn store_chunks_with_ids(
        &self,
        repository_id: &str,
        branch: &str,
        chunks: &[CodeChunk],
        generation: i64,
    ) -> Result<Vec<Uuid>>;

    /// Search for similar code chunks
    ///
    /// Returns chunks ordered by similarity to the query embedding with their scores
    async fn search(
        &self,
        query_embedding: Vec<f32>,
        limit: usize,
    ) -> Result<Vec<StorageSearchResult>>;

    /// Delete chunks by their IDs
    ///
    /// Used for atomic replacement when files are updated
    async fn delete_chunks(&self, chunk_ids: &[Uuid]) -> Result<()>;

    /// Check if the storage collection exists
    async fn collection_exists(&self) -> Result<bool>;

    /// Create the storage collection if it doesn't exist
    async fn ensure_collection(&self) -> Result<()>;

    /// Drop the entire collection
    ///
    /// WARNING: This deletes all data in the collection
    async fn drop_collection(&self) -> Result<bool>;

    /// Get storage statistics
    async fn get_stats(&self) -> Result<StorageStats>;
}

/// Statistics about the vector storage
#[derive(Debug, Clone)]
pub struct StorageStats {
    /// Total number of vectors stored
    pub vector_count: usize,
    /// Storage size in bytes (if available)
    pub storage_bytes: Option<u64>,
    /// Collection name
    pub collection_name: String,
    /// Storage backend type (e.g., "qdrant", "pinecone")
    pub storage_type: String,
}

/// Configuration for vector storage backends
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Storage backend URL
    pub url: String,
    /// Collection/index name
    pub collection_name: String,
    /// Additional backend-specific configuration
    pub extra_config: Option<serde_json::Value>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:6334".to_string(),
            collection_name: "codetriever".to_string(),
            extra_config: None,
        }
    }
}
