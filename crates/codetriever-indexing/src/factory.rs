//! Service factory for clean dependency injection
//!
//! This module provides a comprehensive factory pattern for constructing all services
//! with proper dependency injection. Each service is constructed independently with
//! its own dependencies, eliminating circular dependencies and architectural debt.

use crate::{IndexerResult, indexing::Indexer};
use codetriever_config::ApplicationConfig;
use codetriever_embeddings::EmbeddingService;
use codetriever_meta_data::traits::FileRepository;
use codetriever_vector_data::VectorStorage;
use std::sync::Arc;

/// Configuration for indexing orchestration services
#[derive(Debug, Clone)]
pub struct ServiceConfig {
    /// Whether to use mock services for testing
    pub use_mocks: bool,
}

impl ServiceConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> IndexerResult<Self> {
        let use_mocks =
            std::env::var("USE_MOCKS").unwrap_or_else(|_| "false".to_string()) == "true";

        Ok(Self { use_mocks })
    }

    /// Create test configuration with mocks
    pub fn for_testing() -> Self {
        Self { use_mocks: true }
    }
}

/// Factory for constructing indexing orchestration services
pub struct ServiceFactory {
    config: ServiceConfig,
}

impl ServiceFactory {
    /// Create factory from configuration
    pub fn new(config: ServiceConfig) -> Self {
        Self { config }
    }

    /// Create factory from environment variables
    pub fn from_env() -> IndexerResult<Self> {
        let config = ServiceConfig::from_env()?;
        Ok(Self::new(config))
    }

    /// Create factory for testing with mocks
    pub fn for_testing() -> Self {
        let config = ServiceConfig::for_testing();
        Self::new(config)
    }

    /// Create Indexer with injected dependencies
    ///
    /// All dependencies are required - no defaults, no fallbacks.
    /// Factory coordinates tokenizer loading for clean dependency injection.
    pub async fn indexer(
        &self,
        embedding_service: Arc<dyn EmbeddingService>,
        vector_storage: Arc<dyn VectorStorage>,
        repository: Arc<dyn FileRepository>,
    ) -> IndexerResult<Indexer> {
        let config = ApplicationConfig::from_env();

        // Load tokenizer from embedding service for accurate chunking
        let tokenizer = embedding_service.provider().get_tokenizer().await;

        // DEBUG: Check if tokenizer loaded
        tracing::debug!("Tokenizer loaded: {}", tokenizer.is_some());
        tracing::debug!("split_large_units: {}", config.indexing.split_large_units);
        tracing::debug!("max_chunk_tokens: {}", config.indexing.max_chunk_tokens);

        // Create CodeParser with tokenizer (no lazy loading!)
        let code_parser = codetriever_parsing::CodeParser::new(
            tokenizer,
            config.indexing.split_large_units,
            config.indexing.max_chunk_tokens, // Use chunk size, not model max
        );

        Ok(Indexer::new(
            embedding_service,
            vector_storage,
            repository,
            code_parser,
            &config,
        ))
    }

    /// Get configuration
    pub fn config(&self) -> &ServiceConfig {
        &self.config
    }
}
