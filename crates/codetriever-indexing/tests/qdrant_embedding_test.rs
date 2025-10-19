//! Integration test for indexer with Qdrant storage

#[path = "test_utils.rs"]
mod test_utils;

use codetriever_indexing::indexing::{Indexer, service::FileContent};
use codetriever_search::SearchProvider;
use std::{path::Path, sync::Arc};
use test_utils::{
    cleanup_test_storage, create_code_parser_with_tokenizer, create_test_embedding_service,
    create_test_repository, create_test_storage, test_config,
};

/// Read files from directory (reuse from index_rust_mini_redis_test pattern)
async fn read_directory_files(
    dir: &Path,
    base_dir: &Path,
    recursive: bool,
) -> Result<Vec<FileContent>, std::io::Error> {
    let mut files = Vec::new();

    let mut entries = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        if path.is_file() {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                let path_str = path
                    .strip_prefix(base_dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();

                let hash = codetriever_meta_data::hash_content(&content);

                files.push(FileContent {
                    path: path_str,
                    content,
                    hash,
                });
            }
        } else if recursive && path.is_dir() {
            let sub_files = Box::pin(read_directory_files(&path, base_dir, recursive)).await?;
            files.extend(sub_files);
        }
    }

    Ok(files)
}

#[test]
fn test_indexer_stores_chunks_in_qdrant() {
    test_utils::get_test_runtime().block_on(async {
        // Note: This test requires Qdrant to be running locally on port 6334
        // You can start it with: docker-compose -f docker/docker-compose.qdrant.yml up -d

        // Create all required dependencies
        let config = test_config();

        // create_test_storage handles collection creation automatically
        let storage = create_test_storage("indexer_integration")
            .await
            .expect("Failed to create storage");

        let embedding_service = create_test_embedding_service();
        let repository = create_test_repository().await;

        // Load tokenizer for accurate chunking
        let code_parser = create_code_parser_with_tokenizer(&embedding_service).await;
        let mut indexer = Indexer::new(
            embedding_service,
            Arc::new(storage.clone()) as Arc<dyn codetriever_vector_data::VectorStorage>,
            repository,
            code_parser,
            &config,
        );

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

        // Read files and use real index_file_content() API
        let files = read_directory_files(&test_path, &test_path, true)
            .await
            .expect("Failed to read directory");

        println!("Read {} files from test repo", files.len());

        // Use unique project ID per run to avoid DB state conflicts
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let project_id = format!("test-indexer-{timestamp}:main");

        let start = std::time::Instant::now();
        let result = indexer
            .index_file_content(&project_id, files)
            .await
            .expect("Failed to index files");
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
        let embedding_service = indexer.embedding_service();
        let vector_storage = indexer.vector_storage();

        // Create database client for search
        let db_config = codetriever_config::DatabaseConfig::from_env();
        let pools = codetriever_meta_data::PoolManager::new(
            &db_config,
            codetriever_meta_data::PoolConfig::default(),
        )
        .await
        .expect("Failed to create pool manager");
        let db_client = std::sync::Arc::new(codetriever_meta_data::DataClient::new(pools));

        let search_service =
            codetriever_search::SearchService::new(embedding_service, vector_storage, db_client);
        let correlation_id = codetriever_common::CorrelationId::new();
        let search_results = search_service
            .search(query, 5, &correlation_id)
            .await
            .expect("Failed to search");

        assert!(
            !search_results.is_empty(),
            "Should find results for 'redis connection'"
        );

        cleanup_test_storage(&storage)
            .await
            .expect("Failed to cleanup");
    })
}
