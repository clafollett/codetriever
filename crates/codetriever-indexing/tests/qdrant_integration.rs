//! Integration tests for Qdrant storage
//!
//! These tests require a running Qdrant instance.
//! Run with: cargo test --test qdrant_integration -- --ignored

use codetriever_common::CorrelationId;
use codetriever_parsing::CodeChunk;
use codetriever_vector_data::{QdrantStorage, VectorStorage};

#[tokio::test]
async fn test_delete_chunks_removes_points_from_collection() {
    // Initialize environment to load .env file (includes QDRANT_API_KEY)
    codetriever_common::initialize_environment();

    // This test requires a running Qdrant instance
    // The Rust client uses gRPC port 6334
    let qdrant_url =
        std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6334".to_string());

    let storage = QdrantStorage::new(qdrant_url, "test_delete_chunks".to_string())
        .await
        .expect("Failed to create storage");

    // Create and store test chunks
    let chunks = vec![
        CodeChunk {
            file_path: "test.rs".to_string(),
            content: "test content 1".to_string(),
            start_line: 1,
            end_line: 5,
            byte_start: 0,
            byte_end: 14,
            language: "rust".to_string(),
            embedding: Some(vec![0.1; 768]),
            token_count: Some(10),
            kind: Some("function".to_string()),
            name: Some("test_fn1".to_string()),
        },
        CodeChunk {
            file_path: "test.rs".to_string(),
            content: "test content 2".to_string(),
            start_line: 6,
            end_line: 10,
            byte_start: 14,
            byte_end: 28,
            language: "rust".to_string(),
            embedding: Some(vec![0.2; 768]),
            token_count: Some(10),
            kind: Some("function".to_string()),
            name: Some("test_fn2".to_string()),
        },
    ];

    let correlation_id = CorrelationId::new();

    // Store chunks with deterministic IDs
    let chunk_ids = storage
        .store_chunks("test_repo", "main", &chunks, 1, &correlation_id)
        .await
        .expect("Failed to store chunks with IDs");

    assert_eq!(chunk_ids.len(), 2, "Should have stored 2 chunks");

    // Search to verify they exist
    let results = storage
        .search(vec![0.1; 768], 10, &correlation_id)
        .await
        .expect("Search failed");
    assert!(results.len() >= 2, "Should find at least 2 chunks");

    // Now delete the chunks using their UUIDs
    storage
        .delete_chunks(&chunk_ids)
        .await
        .expect("Failed to delete chunks");

    // Search again to verify they're gone
    let results_after = storage
        .search(vec![0.1; 768], 10, &correlation_id)
        .await
        .expect("Search failed");
    assert!(
        results_after.len() < results.len(),
        "Should have fewer chunks after deletion"
    );

    // Clean up - drop the test collection
    storage
        .drop_collection()
        .await
        .expect("Failed to drop test collection");
}

#[tokio::test]
async fn test_store_and_search_chunks() {
    // Initialize environment to load .env file (includes QDRANT_API_KEY)
    codetriever_common::initialize_environment();

    let qdrant_url =
        std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6334".to_string());

    let storage = QdrantStorage::new(qdrant_url, "test_search".to_string())
        .await
        .expect("Failed to create storage");

    let chunks = vec![CodeChunk {
        file_path: "search_test.rs".to_string(),
        content: "fn hello_world() { println!(\"Hello\"); }".to_string(),
        start_line: 1,
        end_line: 1,
        byte_start: 0,
        byte_end: 40,
        language: "rust".to_string(),
        embedding: Some(vec![0.5; 768]),
        token_count: Some(8),
        kind: Some("function".to_string()),
        name: Some("hello_world".to_string()),
    }];

    let correlation_id = CorrelationId::new();

    // Store chunk
    let chunk_ids = storage
        .store_chunks("test_repo", "main", &chunks, 1, &correlation_id)
        .await
        .expect("Failed to store chunks");
    assert_eq!(chunk_ids.len(), 1);

    // Search for it
    let results = storage
        .search(vec![0.5; 768], 5, &correlation_id)
        .await
        .expect("Search failed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].chunk.name, Some("hello_world".to_string()));
    // Verify we got a similarity score
    assert!(results[0].similarity > 0.0);

    // Clean up
    storage
        .drop_collection()
        .await
        .expect("Failed to drop test collection");
}
