//! Concrete implementation of the EmbeddingService
//!
//! This module provides the default embedding service implementation
//! that uses the existing EmbeddingModel.

use super::model::EmbeddingModel;
use super::traits::{EmbeddingConfig, EmbeddingProvider, EmbeddingService, EmbeddingStats};
use crate::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// Default implementation of EmbeddingProvider using the existing model
pub struct DefaultEmbeddingProvider {
    model: Arc<Mutex<EmbeddingModel>>,
    config: EmbeddingConfig,
}

impl DefaultEmbeddingProvider {
    /// Create a new embedding provider with the given configuration
    pub fn new(config: EmbeddingConfig) -> Self {
        let model = EmbeddingModel::new(config.model_id.clone(), config.max_tokens);
        Self {
            model: Arc::new(Mutex::new(model)),
            config,
        }
    }

    /// Create from an existing EmbeddingModel
    pub fn from_model(model: EmbeddingModel, config: EmbeddingConfig) -> Self {
        Self {
            model: Arc::new(Mutex::new(model)),
            config,
        }
    }
}

#[async_trait]
impl EmbeddingProvider for DefaultEmbeddingProvider {
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        // Convert &[&str] to Vec<String> for the existing API
        let text_strings: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        let mut model = self.model.lock().await;
        model.embed(text_strings).await
    }

    fn embedding_dimension(&self) -> usize {
        768 // Jina v2 embeddings are 768-dimensional
    }

    fn max_tokens(&self) -> usize {
        self.config.max_tokens
    }

    fn model_name(&self) -> &str {
        &self.config.model_id
    }

    async fn is_ready(&self) -> bool {
        let mut model = self.model.lock().await;
        // Call ensure_model_loaded to check and load if necessary
        model.ensure_model_loaded().await.is_ok()
    }

    async fn ensure_ready(&self) -> Result<()> {
        if !self.is_ready().await {
            // The model loads on first use, so trigger a dummy embedding
            let _ = self.embed("test").await?;
        }
        Ok(())
    }
}

/// Default implementation of EmbeddingService
pub struct DefaultEmbeddingService {
    provider: Box<dyn EmbeddingProvider>,
    stats: Arc<RwLock<EmbeddingStats>>,
    batch_size: usize,
}

impl DefaultEmbeddingService {
    /// Create a new embedding service with the default provider
    pub fn new(config: EmbeddingConfig) -> Self {
        let batch_size = config.batch_size;
        let model_name = config.model_id.clone();
        let provider = Box::new(DefaultEmbeddingProvider::new(config));

        let stats = Arc::new(RwLock::new(EmbeddingStats {
            model_name,
            embedding_dimension: provider.embedding_dimension(),
            ..Default::default()
        }));

        Self {
            provider,
            stats,
            batch_size,
        }
    }

    /// Create with a custom provider
    pub fn with_provider(provider: Box<dyn EmbeddingProvider>, batch_size: usize) -> Self {
        let stats = Arc::new(RwLock::new(EmbeddingStats {
            model_name: provider.model_name().to_string(),
            embedding_dimension: provider.embedding_dimension(),
            ..Default::default()
        }));

        Self {
            provider,
            stats,
            batch_size,
        }
    }
}

#[async_trait]
impl EmbeddingService for DefaultEmbeddingService {
    async fn generate_embeddings(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        use std::time::Instant;

        // Ensure provider is ready
        self.provider.ensure_ready().await?;

        let mut all_embeddings = Vec::with_capacity(texts.len());

        // Process in batches
        for batch in texts.chunks(self.batch_size) {
            let start = Instant::now();

            // Convert to &[&str] for the provider API
            let text_refs: Vec<&str> = batch.iter().map(|s| s.as_str()).collect();
            let embeddings = self.provider.embed_batch(&text_refs).await?;

            all_embeddings.extend(embeddings);

            // Update stats
            let elapsed = start.elapsed().as_millis() as f64;
            let mut stats = self.stats.write().await;
            stats.total_embeddings += batch.len();
            stats.total_batches += 1;

            // Update running average
            let prev_avg = stats.avg_batch_time_ms;
            let n = stats.total_batches as f64;
            stats.avg_batch_time_ms = (prev_avg * (n - 1.0) + elapsed) / n;
        }

        Ok(all_embeddings)
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
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if self.fail {
            return Err(crate::Error::Other("Mock embedding failure".into()));
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

    async fn ensure_ready(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embedding_service_batching() {
        let provider = Box::new(MockEmbeddingProvider::new(768));
        let service = DefaultEmbeddingService::with_provider(provider, 2);

        let texts = vec![
            "text1".to_string(),
            "text2".to_string(),
            "text3".to_string(),
            "text4".to_string(),
            "text5".to_string(),
        ];

        let embeddings = service.generate_embeddings(texts).await.unwrap();
        assert_eq!(embeddings.len(), 5);
        assert_eq!(embeddings[0].len(), 768);

        let stats = service.get_stats().await;
        assert_eq!(stats.total_embeddings, 5);
        assert_eq!(stats.total_batches, 3); // 5 texts with batch size 2 = 3 batches
    }

    #[tokio::test]
    async fn test_embedding_service_error_handling() {
        let provider = Box::new(MockEmbeddingProvider::new(768).with_failure());
        let service = DefaultEmbeddingService::with_provider(provider, 2);

        let texts = vec!["text1".to_string()];
        let result = service.generate_embeddings(texts).await;

        assert!(result.is_err());
    }
}
