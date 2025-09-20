//! Structured error types for the indexer crate
//!
//! This module provides comprehensive error handling with contextual information,
//! correlation ID support, and structured error variants that preserve context
//! throughout the indexing and search pipeline.

use codetriever_common::CommonError;
use std::time::Duration;
use thiserror::Error;

/// Correlation ID type for tracking operations across service boundaries
pub type CorrelationId = String;

/// Structured error enum with rich context for debugging and monitoring
#[derive(Error, Debug)]
pub enum Error {
    // ========== Common Error Variants ==========
    #[error("IO error: {message}")]
    Io {
        message: String,
        #[source]
        source: Option<std::io::Error>,
    },

    #[error("Configuration error: {message}")]
    Configuration { message: String },

    #[error("Parsing error: {message}")]
    Parse { message: String },

    // ========== Search Operation Errors ==========
    #[error(
        "Search timeout after {timeout_duration:?} for query: '{query}' (correlation_id: {correlation_id})"
    )]
    SearchTimeout {
        query: String,
        timeout_duration: Duration,
        correlation_id: CorrelationId,
    },

    // ========== Vector Storage Errors ==========
    #[error(
        "Vector storage {operation} failed on collection '{collection}' (correlation_id: {correlation_id})"
    )]
    VectorStorageError {
        operation: String,
        collection: String,
        correlation_id: CorrelationId,
        #[source]
        cause: Box<dyn std::error::Error + Send + Sync>,
    },

    // ========== Embedding Generation Errors ==========
    #[error(
        "Embedding generation failed for {text_count} texts using model '{model}' (correlation_id: {correlation_id})"
    )]
    EmbeddingGenerationFailed {
        text_count: usize,
        model: String,
        correlation_id: CorrelationId,
        #[source]
        cause: Box<dyn std::error::Error + Send + Sync>,
    },

    // ========== Indexing Operation Errors ==========
    #[error(
        "Indexing failed for file '{file_path}' in repository '{repository_id}' (correlation_id: {correlation_id})"
    )]
    IndexingFailed {
        file_path: String,
        repository_id: String,
        correlation_id: CorrelationId,
        #[source]
        cause: Box<dyn std::error::Error + Send + Sync>,
    },

    // ========== Legacy Variants (for gradual migration) ==========
    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Other error: {0}")]
    Other(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl Error {
    // ========== Structured Error Constructors ==========

    /// Create a search timeout error with full context
    pub fn search_timeout(
        query: impl Into<String>,
        timeout_duration: Duration,
        correlation_id: impl Into<CorrelationId>,
    ) -> Self {
        Self::SearchTimeout {
            query: query.into(),
            timeout_duration,
            correlation_id: correlation_id.into(),
        }
    }

    /// Create a vector storage error with operation context
    pub fn vector_storage_error(
        operation: impl Into<String>,
        collection: impl Into<String>,
        correlation_id: impl Into<CorrelationId>,
        cause: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::VectorStorageError {
            operation: operation.into(),
            collection: collection.into(),
            correlation_id: correlation_id.into(),
            cause: Box::new(cause),
        }
    }

    /// Create an embedding generation error with model context
    pub fn embedding_generation_failed(
        text_count: usize,
        model: impl Into<String>,
        correlation_id: impl Into<CorrelationId>,
        cause: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::EmbeddingGenerationFailed {
            text_count,
            model: model.into(),
            correlation_id: correlation_id.into(),
            cause: Box::new(cause),
        }
    }

    /// Create an indexing failure error with file context
    pub fn indexing_failed(
        file_path: impl Into<String>,
        repository_id: impl Into<String>,
        correlation_id: impl Into<CorrelationId>,
        cause: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::IndexingFailed {
            file_path: file_path.into(),
            repository_id: repository_id.into(),
            correlation_id: correlation_id.into(),
            cause: Box::new(cause),
        }
    }

    /// Extract correlation ID from error if available
    pub fn correlation_id(&self) -> Option<&str> {
        match self {
            Self::SearchTimeout { correlation_id, .. } => Some(correlation_id),
            Self::VectorStorageError { correlation_id, .. } => Some(correlation_id),
            Self::EmbeddingGenerationFailed { correlation_id, .. } => Some(correlation_id),
            Self::IndexingFailed { correlation_id, .. } => Some(correlation_id),
            _ => None,
        }
    }

    /// Check if this error is retryable based on its type
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::SearchTimeout { .. } | Self::VectorStorageError { .. }
        )
    }
}

// Implement the CommonError trait (for backward compatibility)
impl CommonError for Error {
    fn io_error(msg: impl Into<String>) -> Self {
        Self::Io {
            message: msg.into(),
            source: None,
        }
    }

    fn config_error(msg: impl Into<String>) -> Self {
        Self::Configuration {
            message: msg.into(),
        }
    }

    fn parse_error(msg: impl Into<String>) -> Self {
        Self::Parse {
            message: msg.into(),
        }
    }

    fn other_error(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}

// Standard library error conversions
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io {
            message: e.to_string(),
            source: Some(e),
        }
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
