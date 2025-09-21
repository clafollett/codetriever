//! Codetriever embedding generation crate
//!
//! This crate provides ML-based embedding generation for semantic code search.
//! It handles the conversion of code text into high-dimensional vectors using
//! transformer models like Jina embeddings.

pub mod embedding;
pub mod error;

// Re-export main types
pub use embedding::{
    DefaultEmbeddingService, EmbeddingConfig, EmbeddingModel, EmbeddingProvider, EmbeddingService,
    EmbeddingStats,
};
pub use error::{EmbeddingError, EmbeddingResult};
