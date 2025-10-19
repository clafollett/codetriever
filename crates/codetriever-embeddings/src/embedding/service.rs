//! Concrete implementation of the EmbeddingService
//!
//! This module provides the default embedding service implementation
//! that uses the existing EmbeddingModel.

use super::model::EmbeddingModel;
use super::pool::EmbeddingModelPool;
use super::traits::{EmbeddingProvider, EmbeddingService, EmbeddingStats};
use crate::EmbeddingResult;
use async_trait::async_trait;
use codetriever_config::EmbeddingConfig; // Use unified configuration
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

// Global provider counter for debugging
static PROVIDER_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Default implementation of EmbeddingProvider using model pool with batching
///
/// Uses a pool of embedding models for parallel inference without lock contention.
pub struct DefaultEmbeddingProvider {
    pool: EmbeddingModelPool,
    config: EmbeddingConfig,
    provider_id: String, // Unique ID for debugging
}

impl DefaultEmbeddingProvider {
    /// Create a new embedding provider with the given configuration
    ///
    /// Initializes a pool of embedding models for parallel inference
    pub fn new(config: EmbeddingConfig) -> Self {
        let provider_id = format!(
            "provider-{}",
            PROVIDER_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );
        tracing::debug!("Creating DefaultEmbeddingProvider {provider_id}");

        let pool = EmbeddingModelPool::new(
            config.model.id.clone(),
            config.model.max_tokens,
            config.performance.pool_size,
            config.performance.indexer_batch_size,
            Duration::from_millis(config.performance.batch_timeout_ms),
        );

        Self {
            pool,
            config,
            provider_id,
        }
    }

    /// Create from an existing EmbeddingModel (for testing/compatibility)
    ///
    /// Creates a pool with a single model instance
    pub fn from_model(_model: EmbeddingModel, config: EmbeddingConfig) -> Self {
        let provider_id = format!(
            "provider-{}",
            PROVIDER_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );
        tracing::debug!("Creating DefaultEmbeddingProvider {provider_id} (from_model)");

        // For compatibility, create single-model pool
        let pool = EmbeddingModelPool::new(
            config.model.id.clone(),
            config.model.max_tokens,
            1, // Single model
            config.performance.indexer_batch_size,
            Duration::from_millis(config.performance.batch_timeout_ms),
        );

        Self {
            pool,
            config,
            provider_id,
        }
    }
}

impl Drop for DefaultEmbeddingProvider {
    fn drop(&mut self) {
        tracing::debug!("Dropping DefaultEmbeddingProvider: {}", self.provider_id);
    }
}

#[async_trait]
impl EmbeddingProvider for DefaultEmbeddingProvider {
    #[tracing::instrument(skip(self, texts), fields(text_count = texts.len(), elapsed_ms))]
    async fn embed_batch(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        let start = std::time::Instant::now();

        // Convert to owned strings for pool (required for async channel transfer)
        let owned_texts: Vec<String> = texts.iter().map(|s| s.to_string()).collect();

        // Submit to pool - will be batched with other concurrent requests
        let result = self.pool.embed(owned_texts).await;

        tracing::Span::current().record("elapsed_ms", start.elapsed().as_millis() as u64);

        result
    }

    fn embedding_dimension(&self) -> usize {
        768 // Jina v2 embeddings are 768-dimensional
    }

    fn max_tokens(&self) -> usize {
        self.config.model.max_tokens
    }

    fn model_name(&self) -> &str {
        &self.config.model.id
    }

    async fn is_ready(&self) -> bool {
        // Pool is always ready - models load lazily on first use
        // We could check if at least one worker is ready, but it's not critical
        true
    }

    async fn ensure_ready(&self) -> EmbeddingResult<()> {
        // Warm up the pool by submitting a test request
        // This triggers lazy loading in at least one worker
        let _ = self.embed_batch(&["test"]).await?;
        Ok(())
    }

    async fn get_tokenizer(&self) -> Option<std::sync::Arc<tokenizers::Tokenizer>> {
        // Delegate to pool - this will load tokenizer lazily on first call
        self.pool.get_tokenizer().await.ok().flatten()
    }
}

// Global service counter for debugging
static SERVICE_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Default implementation of EmbeddingService
///
/// Provider is Arc-shared to ensure pool stays alive across all users
/// NO service-level batching - provider (pool) handles batching internally
pub struct DefaultEmbeddingService {
    provider: Arc<dyn EmbeddingProvider>,
    stats: Arc<RwLock<EmbeddingStats>>,
    service_id: String, // Unique ID for debugging
}

impl DefaultEmbeddingService {
    /// Create a new embedding service with the default provider
    pub fn new(config: EmbeddingConfig) -> Self {
        let service_id = format!(
            "service-{}",
            SERVICE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );
        tracing::debug!("Creating DefaultEmbeddingService {service_id}");

        let model_name = config.model.id.clone();
        let provider = Arc::new(DefaultEmbeddingProvider::new(config));

        let stats = Arc::new(RwLock::new(EmbeddingStats {
            model_name,
            embedding_dimension: provider.embedding_dimension(),
            ..Default::default()
        }));

        Self {
            provider,
            stats,
            service_id,
        }
    }

    /// Create with a custom provider (for testing)
    pub fn with_provider(provider: Arc<dyn EmbeddingProvider>) -> Self {
        let service_id = format!(
            "service-{}",
            SERVICE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );
        tracing::debug!("Creating DefaultEmbeddingService {service_id} (with_provider)");

        let stats = Arc::new(RwLock::new(EmbeddingStats {
            model_name: provider.model_name().to_string(),
            embedding_dimension: provider.embedding_dimension(),
            ..Default::default()
        }));

        Self {
            provider,
            stats,
            service_id,
        }
    }
}

impl Drop for DefaultEmbeddingService {
    fn drop(&mut self) {
        tracing::debug!("Dropping DefaultEmbeddingService: {}", self.service_id);
    }
}

#[async_trait]
impl EmbeddingService for DefaultEmbeddingService {
    async fn generate_embeddings(&self, texts: Vec<&str>) -> EmbeddingResult<Vec<Vec<f32>>> {
        use std::time::Instant;

        // NOTE: ensure_ready() is called ONCE at indexer startup, not on every batch!
        // Calling it here would double our embedding count (warmup embed per batch).
        // The pool handles lazy loading - first call will load models automatically.

        // NO service-level batching! Just pass all texts to provider.
        // The pool handles batching internally (see pool.rs worker task).
        // This eliminates redundant batching overhead.
        let start = Instant::now();
        let embeddings = self.provider.embed_batch(&texts).await?;

        // Update stats
        let elapsed = start.elapsed().as_millis() as f64;
        let mut stats = self.stats.write().await;
        stats.total_embeddings += texts.len();
        stats.total_batches += 1;
        stats.avg_batch_time_ms = (stats.avg_batch_time_ms * (stats.total_batches as f64 - 1.0)
            + elapsed)
            / stats.total_batches as f64;

        Ok(embeddings)
    }

    fn provider(&self) -> &dyn EmbeddingProvider {
        self.provider.as_ref()
    }

    async fn get_stats(&self) -> EmbeddingStats {
        self.stats.read().await.clone()
    }
}

/// Mock implementation for testing
#[cfg(test)]
pub struct MockEmbeddingProvider {
    dimension: usize,
    fail: bool,
}

#[cfg(test)]
impl MockEmbeddingProvider {
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            fail: false,
        }
    }

    pub fn with_failure(mut self) -> Self {
        self.fail = true;
        self
    }
}

#[cfg(test)]
#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed_batch(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
        if self.fail {
            return Err(crate::EmbeddingError::Other(
                "Mock embedding failure".into(),
            ));
        }

        // Return mock embeddings
        Ok(texts.iter().map(|_| vec![0.1; self.dimension]).collect())
    }

    fn embedding_dimension(&self) -> usize {
        self.dimension
    }

    fn max_tokens(&self) -> usize {
        8192
    }

    fn model_name(&self) -> &str {
        "mock-embedding-model"
    }

    async fn is_ready(&self) -> bool {
        true
    }

    async fn ensure_ready(&self) -> EmbeddingResult<()> {
        Ok(())
    }

    async fn get_tokenizer(&self) -> Option<std::sync::Arc<tokenizers::Tokenizer>> {
        // Mock doesn't have a real tokenizer
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embedding_service_batching() {
        let provider = Arc::new(MockEmbeddingProvider::new(768));
        let service = DefaultEmbeddingService::with_provider(provider);

        let texts = vec!["text1", "text2", "text3", "text4", "text5"];

        let embeddings = service.generate_embeddings(texts).await.unwrap();
        assert_eq!(embeddings.len(), 5);
        assert_eq!(embeddings[0].len(), 768);

        let stats = service.get_stats().await;
        assert_eq!(stats.total_embeddings, 5);
        assert_eq!(stats.total_batches, 1); // NO service-level batching - single provider call
    }

    #[tokio::test]
    async fn test_embedding_service_error_handling() {
        let provider = Arc::new(MockEmbeddingProvider::new(768).with_failure());
        let service = DefaultEmbeddingService::with_provider(provider);

        let texts = vec!["text1"];
        let result = service.generate_embeddings(texts).await;

        assert!(result.is_err());
    }
}
