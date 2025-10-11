//! Shared test utilities for integration tests
//!
//! This module provides common testing utilities used across multiple test files.
//! Functions are only compiled into test binaries that actually use them.

use codetriever_config::{ApplicationConfig, DatabaseConfig, Profile};
use codetriever_embeddings::{DefaultEmbeddingService, EmbeddingService};
use codetriever_meta_data::{
    pool_manager::{PoolConfig, PoolManager},
    repository::DbFileRepository,
    traits::FileRepository,
};
use codetriever_vector_data::{QdrantStorage, VectorStorage};
use std::sync::Arc;

/// Get the Qdrant URL for testing, defaulting to localhost
/// Can be overridden with QDRANT_TEST_URL environment variable
fn test_qdrant_url() -> String {
    std::env::var("QDRANT_TEST_URL").unwrap_or_else(|_| "http://localhost:6334".to_string())
}

/// Get a unique collection name for testing to avoid conflicts
/// Includes timestamp to ensure uniqueness
fn test_collection_name(base: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("test_{base}_{timestamp}")
}

/// Check if HuggingFace token is available for model downloads
fn has_hf_token() -> bool {
    std::env::var("HF_TOKEN").is_ok() || std::env::var("HUGGING_FACE_HUB_TOKEN").is_ok()
}

/// Skip test if HF token is not available
#[allow(unused)]
pub fn skip_without_hf_token() -> Option<()> {
    if !has_hf_token() {
        println!("Skipping test - HF_TOKEN or HUGGING_FACE_HUB_TOKEN not set");
        return None;
    }
    Some(())
}

/// Create a test storage instance with a unique collection name
#[allow(unused)]
pub async fn create_test_storage(test_name: &str) -> Result<QdrantStorage, String> {
    // Initialize environment to load .env file (includes QDRANT_API_KEY)
    codetriever_common::initialize_environment();

    QdrantStorage::new(test_qdrant_url(), test_collection_name(test_name))
        .await
        .map_err(|e| format!("Failed to create storage: {e}"))
}

/// Get default test configuration
#[allow(unused)]
pub fn test_config() -> ApplicationConfig {
    ApplicationConfig::with_profile(Profile::Test)
}

/// Clean up test storage by dropping the collection
#[allow(unused)]
pub async fn cleanup_test_storage(storage: &QdrantStorage) -> Result<(), String> {
    storage
        .drop_collection()
        .await
        .map_err(|e| format!("Failed to drop collection: {e}"))?;
    Ok(())
}

/// Create embedding service for testing
#[allow(unused)]
pub fn create_test_embedding_service() -> Arc<dyn EmbeddingService> {
    let config = test_config();
    Arc::new(DefaultEmbeddingService::new(config.embedding.clone()))
}

/// Create REAL file repository for integration testing
#[allow(unused)]
pub async fn create_test_repository() -> Arc<dyn FileRepository> {
    // Initialize environment to load database config
    codetriever_common::initialize_environment();

    // Create REAL database connection pool
    let db_config = DatabaseConfig::for_profile(Profile::Test);
    let pools = PoolManager::new(&db_config, PoolConfig::default())
        .await
        .expect("Failed to create pool manager for test repository");

    Arc::new(DbFileRepository::new(pools))
}

/// Create a fully configured indexer for integration tests with REAL dependencies
#[allow(unused)]
pub async fn create_test_indexer(
    test_name: &str,
) -> Result<(codetriever_indexing::indexing::Indexer, QdrantStorage), String> {
    let config = test_config();
    let storage = create_test_storage(test_name).await?;
    let embedding_service = create_test_embedding_service();
    let repository = create_test_repository().await;
    let code_parser = codetriever_parsing::CodeParser::default();

    let indexer = codetriever_indexing::indexing::Indexer::new(
        embedding_service,
        Arc::new(storage.clone()) as Arc<dyn VectorStorage>,
        repository,
        code_parser,
        &config,
    );

    Ok((indexer, storage))
}
