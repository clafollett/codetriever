use codetriever_common::CorrelationId;
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
