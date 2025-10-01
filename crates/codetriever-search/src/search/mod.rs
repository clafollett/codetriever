//! Search service module for querying indexed code

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use codetriever_common::CorrelationId;
use codetriever_vector_data::CodeChunk;
use thiserror::Error;

/// Search-specific error types with correlation ID support
#[derive(Error, Debug)]
pub enum SearchError {
    #[error("Embedding generation failed for query '{query}' (correlation: {correlation_id})")]
    EmbeddingFailed {
        query: String,
        correlation_id: CorrelationId,
    },

    #[error("Vector storage unavailable (correlation: {correlation_id})")]
    StorageUnavailable { correlation_id: CorrelationId },

    #[error("Database timeout during search (correlation: {correlation_id})")]
    DatabaseTimeout { correlation_id: CorrelationId },

    #[error("Database connection failed during initialization: {message}")]
    DatabaseConnectionFailed { message: String },

    #[error(
        "Search timeout after {timeout_ms}ms for query '{query}' (correlation: {correlation_id})"
    )]
    SearchTimeout {
        query: String,
        timeout_ms: u64,
        correlation_id: CorrelationId,
    },

    #[error("Embedding error: {0}")]
    EmbeddingError(#[from] codetriever_embeddings::EmbeddingError),

    #[error("Vector storage error: {0}")]
    VectorDataError(#[from] codetriever_vector_data::VectorDataError),

    #[error("Metadata error: {0}")]
    MetaDataError(#[from] codetriever_meta_data::MetaDataError),

    #[error("Parsing error: {0}")]
    ParsingError(#[from] codetriever_parsing::ParsingError),
}

/// Result type for search operations
pub type SearchResult<T> = std::result::Result<T, SearchError>;

pub mod service;

pub use service::SearchService;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

/// Repository metadata for search results
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

/// Result from a search operation including similarity score and repository metadata
#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub chunk: CodeChunk,
    pub similarity: f32,
    /// Repository metadata populated from database
    pub repository_metadata: Option<RepositoryMetadata>,
}

/// Trait for search operations with correlation ID support
#[async_trait]
pub trait SearchProvider: Send + Sync {
    /// Search for code chunks matching the query
    async fn search(
        &self,
        query: &str,
        limit: usize,
        correlation_id: &CorrelationId,
    ) -> SearchResult<Vec<SearchMatch>>;
}
