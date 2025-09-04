//! Shared test utilities for integration tests
//!
//! This module provides common testing utilities used across multiple test files.
//! Functions are only compiled into test binaries that actually use them.

use codetriever_indexer::{config::Config, storage::QdrantStorage};

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
    QdrantStorage::new(test_qdrant_url(), test_collection_name(test_name))
        .await
        .map_err(|e| format!("Failed to create storage: {e}"))
}

/// Get default test configuration
#[allow(unused)]
pub fn test_config() -> Config {
    Config::default()
}
