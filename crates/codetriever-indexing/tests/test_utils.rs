//! Shared test utilities for integration tests
//!
//! This module provides common testing utilities used across multiple test files.
//! Functions are only compiled into test binaries that actually use them.

use codetriever_config::{ApplicationConfig, DatabaseConfig};
use codetriever_embeddings::EmbeddingService;
use codetriever_meta_data::{
    pool_manager::{PoolConfig, PoolManager},
    repository::DbFileRepository,
    traits::FileRepository,
};
use codetriever_vector_data::{QdrantStorage, VectorStorage};
use std::sync::Arc;

// Re-export shared test utilities from codetriever-test-utils
#[allow(unused_imports)]
pub use codetriever_test_utils::{get_shared_embedding_service, get_test_runtime};

/// Get the Qdrant URL for testing, defaulting to localhost
/// Can be overridden with QDRANT_TEST_URL environment variable
fn test_qdrant_url() -> String {
    std::env::var("QDRANT_TEST_URL").unwrap_or_else(|_| "http://localhost:6334".to_string())
}

/// Get a unique collection name for testing to avoid conflicts
///
/// Uses shared atomic counter from codetriever-test-utils to prevent
/// collisions across all test crates running in parallel.
fn test_collection_name(base: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let counter = codetriever_test_utils::next_collection_counter();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();

    format!("test_{base}_{timestamp}_{counter}")
}

/// Get a unique project ID for testing to avoid database state conflicts
///
/// PostgreSQL database is shared across test runs. Using static project IDs
/// causes files to be marked "Unchanged" on subsequent runs, leading to
/// `files_indexed = 0` failures.
///
/// This function ensures each test run gets a unique project ID.
#[allow(unused)]
pub fn test_project_id(base: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let counter = codetriever_test_utils::next_collection_counter();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();

    format!("test_{base}_{timestamp}_{counter}")
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
    ApplicationConfig::from_env()
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

/// Get shared embedding service for testing
///
/// **IMPORTANT**: Uses centralized shared service from codetriever-test-utils.
/// All tests across ALL crates share the SAME embedding service to prevent
/// loading multiple 4GB models and exhausting RAM.
///
/// DO NOT create new embedding services in tests!
#[allow(unused)]
pub fn create_test_embedding_service() -> Arc<dyn EmbeddingService> {
    get_shared_embedding_service()
}

/// Create CodeParser with tokenizer loaded from embedding service
///
/// This ensures proper chunking by loading the tokenizer from the embedding model.
/// Without this, CodeParser::default() creates a parser without tokenizer,
/// causing large files to create only 1 truncated chunk instead of proper splitting.
#[allow(unused)]
pub async fn create_code_parser_with_tokenizer(
    embedding_service: &Arc<dyn EmbeddingService>,
) -> codetriever_parsing::CodeParser {
    let config = test_config();

    // Load tokenizer from embedding service
    let tokenizer = embedding_service.provider().get_tokenizer().await;

    // Create CodeParser with loaded tokenizer
    codetriever_parsing::CodeParser::new(
        tokenizer,
        config.indexing.split_large_units,
        config.indexing.max_chunk_tokens,
    )
}

/// Create REAL file repository for integration testing
#[allow(unused)]
pub async fn create_test_repository() -> Arc<dyn FileRepository> {
    // Initialize environment to load database config
    codetriever_common::initialize_environment();

    // Create REAL database connection pool
    let db_config = DatabaseConfig::from_env();
    let pools = PoolManager::new(&db_config, PoolConfig::default())
        .await
        .expect("Failed to create pool manager for test repository");

    Arc::new(DbFileRepository::new(pools))
}

/// Create a fully configured indexer for integration tests with REAL dependencies
///
/// This ensures the CodeParser has a tokenizer loaded for proper chunking.
/// Without tokenizer, large files would create only 1 truncated chunk instead of splitting.
#[allow(unused)]
pub async fn create_test_indexer(
    test_name: &str,
) -> Result<(codetriever_indexing::indexing::Indexer, QdrantStorage), String> {
    let config = test_config();
    let storage = create_test_storage(test_name).await?;
    let embedding_service = create_test_embedding_service();
    let repository = create_test_repository().await;

    // Load tokenizer from embedding service for accurate chunking
    let code_parser = create_code_parser_with_tokenizer(&embedding_service).await;

    let indexer = codetriever_indexing::indexing::Indexer::new(
        embedding_service,
        Arc::new(storage.clone()) as Arc<dyn VectorStorage>,
        repository,
        code_parser,
        &config,
    );

    Ok((indexer, storage))
}
