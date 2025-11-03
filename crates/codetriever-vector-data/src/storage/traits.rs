//! Storage abstraction traits for vector databases
//!
//! This module provides trait abstractions for vector storage backends,
//! enabling pluggable storage implementations and better testability.

use crate::VectorDataResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use codetriever_common::CorrelationId;
use codetriever_parsing::CodeChunk;
use uuid::Uuid;

/// Context for storing chunks with all metadata (tenant, repository, commit info)
///
/// This struct contains all the metadata needed to properly store chunks in Qdrant
/// with full context for multi-tenancy, repository tracking, and Git commit information.
#[derive(Debug, Clone)]
pub struct ChunkStorageContext {
    /// Tenant ID for multi-tenancy isolation
    pub tenant_id: Uuid,
    /// Repository identifier (e.g., "github.com/user/repo")
    pub repository_id: String,
    /// Branch name (e.g., "main", "develop")
    pub branch: String,
    /// Generation number for versioning
    pub generation: i64,
    /// Repository URL (e.g., "https://github.com/user/repo")
    pub repository_url: Option<String>,
    /// Git commit SHA
    pub commit_sha: Option<String>,
    /// Git commit message
    pub commit_message: Option<String>,
    /// Git commit timestamp
    pub commit_date: Option<DateTime<Utc>>,
    /// Git commit author
    pub author: Option<String>,
}

/// Repository metadata extracted from storage payload
#[derive(Debug, Clone)]
pub struct RepositoryMetadata {
    pub repository_id: String,
    pub repository_url: Option<String>,
    pub branch: String,
    pub commit_sha: Option<String>,
    pub commit_message: Option<String>,
    pub commit_date: Option<DateTime<Utc>>,
    pub author: Option<String>,
}

/// Search result with similarity score and metadata from storage
#[derive(Debug, Clone)]
pub struct StorageSearchResult {
    pub chunk: CodeChunk,
    pub similarity: f32,
    /// Repository and commit metadata (extracted from Qdrant payload)
    pub metadata: RepositoryMetadata,
}

/// Trait for vector storage backends
///
/// This trait abstracts vector database operations, allowing different
/// implementations (Qdrant, Pinecone, Weaviate, etc.) to be used interchangeably.
#[async_trait]
pub trait VectorStorage: Send + Sync {
    /// Store code chunks with their embeddings and full metadata
    ///
    /// Stores chunks in vector database with complete context including tenant,
    /// repository, and Git commit information. All metadata is stored in the
    /// payload so search results are complete without additional enrichment queries.
    ///
    /// Returns the chunk IDs that were stored
    async fn store_chunks(
        &self,
        context: &ChunkStorageContext,
        chunks: &[CodeChunk],
        correlation_id: &CorrelationId,
    ) -> VectorDataResult<Vec<Uuid>>;

    /// Search for similar code chunks
    ///
    /// Returns chunks ordered by similarity to the query embedding with their scores
    async fn search(
        &self,
        query_embedding: Vec<f32>,
        limit: usize,
        correlation_id: &CorrelationId,
    ) -> VectorDataResult<Vec<StorageSearchResult>>;

    /// Delete chunks by their IDs
    ///
    /// Used for atomic replacement when files are updated
    async fn delete_chunks(&self, chunk_ids: &[Uuid]) -> VectorDataResult<()>;

    /// Check if the storage collection exists
    async fn collection_exists(&self) -> VectorDataResult<bool>;

    /// Create the storage collection if it doesn't exist
    async fn ensure_collection(&self) -> VectorDataResult<()>;

    /// Drop the entire collection
    ///
    /// WARNING: This deletes all data in the collection
    async fn drop_collection(&self) -> VectorDataResult<bool>;

    /// Get storage statistics
    async fn get_stats(&self) -> VectorDataResult<StorageStats>;
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
