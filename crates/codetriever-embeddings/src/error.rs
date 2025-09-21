//! Error types for the codetriever-embeddings crate
//!
//! This module defines embedding-specific error types for ML operations,
//! model loading, tokenization, and inference.

use thiserror::Error;

/// Result type alias for embedding operations
pub type EmbeddingResult<T> = Result<T, EmbeddingError>;

/// Comprehensive error type for embedding operations
#[derive(Error, Debug)]
pub enum EmbeddingError {
    /// Model loading and initialization errors
    #[error("Model loading failed: {0}")]
    ModelLoad(String),

    /// Tokenization and text processing errors
    #[error("Tokenization failed: {0}")]
    Tokenization(String),

    /// ML inference and computation errors
    #[error("Inference failed: {0}")]
    Inference(String),

    /// Configuration and environment errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Embedding generation specific errors
    #[error("Embedding generation failed: {0}")]
    Embedding(String),

    /// Device and hardware errors (GPU/Metal/CPU)
    #[error("Device error: {0}")]
    Device(String),

    /// Network and download errors
    #[error("Network error: {0}")]
    Network(String),

    /// General I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic error for other cases
    #[error("Other error: {0}")]
    Other(String),
}

impl EmbeddingError {
    /// Create a configuration error
    pub fn config_error(msg: &str) -> Self {
        EmbeddingError::Config(msg.to_string())
    }

    /// Create an embedding generation error
    pub fn generation_error(msg: &str) -> Self {
        EmbeddingError::Embedding(msg.to_string())
    }

    /// Create a model loading error
    pub fn model_load_error(msg: &str) -> Self {
        EmbeddingError::ModelLoad(msg.to_string())
    }

    /// Create a tokenization error
    pub fn tokenization_error(msg: &str) -> Self {
        EmbeddingError::Tokenization(msg.to_string())
    }

    /// Create an inference error
    pub fn inference_error(msg: &str) -> Self {
        EmbeddingError::Inference(msg.to_string())
    }
}

// Note: codetriever_common::CommonError is a trait, not a concrete type.
// Concrete error types from codetriever_common would implement this trait.
// For now, we'll rely on the anyhow::Error conversion for common errors.

/// Convert from anyhow error to embedding error
impl From<anyhow::Error> for EmbeddingError {
    fn from(err: anyhow::Error) -> Self {
        EmbeddingError::Other(err.to_string())
    }
}
