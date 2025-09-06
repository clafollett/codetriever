//! Integration test for indexer with Qdrant storage

#[path = "test_utils.rs"]
mod test_utils;

use codetriever_indexer::indexing::Indexer;
use std::path::Path;
use test_utils::{cleanup_test_storage, create_test_storage, test_config};

#[tokio::test]
async fn test_indexer_stores_chunks_in_qdrant() {
    // Note: This test requires Qdrant to be running locally on port 6334
    // You can start it with: docker-compose -f docker/docker-compose.qdrant.yml up -d

    // Create indexer with Qdrant storage
    let config = test_config();
    let storage = create_test_storage("indexer_integration")
        .await
        .expect("Failed to create storage");

    let mut indexer = Indexer::with_config_and_storage(&config, storage.clone());

    // Index a small test repo (mini-redis has ~30 Rust files)
    // Use CARGO_MANIFEST_DIR to find the workspace root reliably
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let test_path = Path::new(manifest_dir)
        .parent() // go up from crates/codetriever-api
        .unwrap()
        .parent() // go up from crates
        .unwrap()
        .join("test-repos/rust-mini-redis/src");

    if !test_path.exists() {
        println!("Test repo not found at {test_path:?}");
        return;
    }

    let start = std::time::Instant::now();
    let result = indexer
        .index_directory(&test_path, true) // Set recursive to true!
        .await
        .expect("Failed to index directory");
    let duration = start.elapsed();

    println!("Indexing stats:");
    println!("  Files indexed: {}", result.files_indexed);
    println!("  Chunks created: {}", result.chunks_created);
    println!("  Chunks stored: {}", result.chunks_stored);
    println!("  Time taken: {duration:.2?}");
    println!(
        "  Speed: {:.2} chunks/sec",
        result.chunks_created as f64 / duration.as_secs_f64()
    );

    assert!(result.files_indexed > 0, "Should index at least one file");
    assert!(result.chunks_created > 0, "Should create chunks");
    assert!(
        result.chunks_stored > 0,
        "Chunks should be stored in Qdrant"
    );

    // Verify we can search for the indexed content
    let query = "redis connection";
    let search_results = indexer.search(query, 5).await.expect("Failed to search");

    assert!(
        !search_results.is_empty(),
        "Should find results for 'redis connection'"
    );

    cleanup_test_storage(&storage)
        .await
        .expect("Failed to cleanup");
}
