//! Service factory for clean dependency injection
//!
//! This module provides a comprehensive factory pattern for constructing all services
//! with proper dependency injection. Each service is constructed independently with
//! its own dependencies, eliminating circular dependencies and architectural debt.

use crate::{IndexerResult, indexing::Indexer};
use codetriever_embeddings::EmbeddingService;
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
    pub fn indexer(
        &self,
        embedding_service: Arc<dyn EmbeddingService>,
        vector_storage: Arc<dyn VectorStorage>,
    ) -> IndexerResult<Indexer> {
        let mut indexer = Indexer::new();
        indexer.set_embedding_service(embedding_service);
        indexer.set_storage_arc(vector_storage);
        Ok(indexer)
    }

    /// Get configuration
    pub fn config(&self) -> &ServiceConfig {
        &self.config
    }
}
