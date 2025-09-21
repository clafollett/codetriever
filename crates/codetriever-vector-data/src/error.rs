//! Error types for vector data storage operations

use thiserror::Error;

/// Result type alias for vector data operations
pub type VectorDataResult<T> = Result<T, VectorDataError>;

/// Errors that can occur during vector storage operations
#[derive(Error, Debug)]
pub enum VectorDataError {
    /// Storage backend is unavailable or connection failed
    #[error("Storage unavailable: {0}")]
    StorageUnavailable(String),

    /// Vector dimension mismatch (e.g., query vector wrong size)
    #[error("Vector dimension mismatch: {0}")]
    VectorDimensionMismatch(String),

    /// Collection/index operations failed
    #[error("Collection operation failed: {0}")]
    CollectionError(String),

    /// Storage backend specific error
    #[error("Storage error: {0}")]
    Storage(String),

    /// Serialization/deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Generic error for other issues
    #[error("Other error: {0}")]
    Other(String),
}

impl From<anyhow::Error> for VectorDataError {
    fn from(err: anyhow::Error) -> Self {
        VectorDataError::Other(err.to_string())
    }
}

impl From<serde_json::Error> for VectorDataError {
    fn from(err: serde_json::Error) -> Self {
        VectorDataError::Serialization(err.to_string())
    }
}
