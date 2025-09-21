//! Trait abstractions for embedding generation
//!
//! This module provides trait abstractions for embedding providers,
//! enabling pluggable implementations and better testability.

use crate::EmbeddingResult;
use async_trait::async_trait;

/// Trait for embedding generation providers
///
/// This trait abstracts embedding generation operations, allowing different
/// providers (local models, API services) to be used interchangeably.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embeddings for a batch of texts
    ///
    /// Returns a vector of embeddings, one for each input text.
    /// The dimensionality depends on the model being used.
    async fn embed_batch(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>>;

    /// Get the dimensionality of embeddings produced by this provider
    fn embedding_dimension(&self) -> usize;

    /// Get the maximum number of tokens this provider can handle
    fn max_tokens(&self) -> usize;

    /// Get the name/description of the embedding model
    fn model_name(&self) -> &str;

    /// Check if the model is ready for use
    async fn is_ready(&self) -> bool;

    /// Ensure the model is loaded and ready
    async fn ensure_ready(&self) -> EmbeddingResult<()>;
}

// EmbeddingConfig removed - now using codetriever_config::EmbeddingConfig
// This eliminates configuration duplication as specified by architect

/// Service for managing embedding generation
///
/// This service coordinates embedding generation, handling batching,
/// caching, and provider management.
#[async_trait]
pub trait EmbeddingService: Send + Sync {
    /// Generate embeddings for code chunks using zero-copy string references
    ///
    /// This is an optimized version that avoids cloning strings when the caller
    /// already has string references available.
    async fn generate_embeddings(&self, texts: Vec<&str>) -> EmbeddingResult<Vec<Vec<f32>>>;

    /// Get the embedding provider being used
    fn provider(&self) -> &dyn EmbeddingProvider;

    /// Get service statistics
    async fn get_stats(&self) -> EmbeddingStats;
}

/// Statistics about embedding generation
#[derive(Debug, Clone, Default)]
pub struct EmbeddingStats {
    /// Total number of embeddings generated
    pub total_embeddings: usize,

    /// Total number of batches processed
    pub total_batches: usize,

    /// Average batch processing time in milliseconds
    pub avg_batch_time_ms: f64,

    /// Model name being used
    pub model_name: String,

    /// Model dimension
    pub embedding_dimension: usize,
}
