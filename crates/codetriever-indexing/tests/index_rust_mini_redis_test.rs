//! Integration test using real Rust codebase (mini-redis)
//!
//! Tests parsing, chunking, and embedding using actual production code
//! to validate the entire indexing pipeline with realistic input.

#[path = "test_utils.rs"]
mod test_utils;

use codetriever_indexing::indexing::{Indexer, service::FileContent};
use codetriever_search::SearchProvider;
use codetriever_vector_data::VectorStorage;
use std::{path::Path, sync::Arc};
use test_utils::{
    cleanup_test_storage, create_code_parser_with_tokenizer, create_test_embedding_service,
    create_test_repository, create_test_storage, test_config,
};

/// Initialize tracing for performance profiling
/// Shows elapsed_ms for instrumented functions
///
/// Control log level with RUST_LOG environment variable:
/// - RUST_LOG=off       - No logs
/// - RUST_LOG=info      - Just important stuff
/// - RUST_LOG=debug     - Show all timing data
fn init_tracing() {
    use tracing_subscriber::{fmt::format::FmtSpan, layer::SubscriberExt, util::SubscriberInitExt};

    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_level(true)
                .with_thread_ids(false)
                .with_span_events(FmtSpan::CLOSE) // Show span fields when span closes
                .compact(),
        )
        .with(
            // Respect RUST_LOG env var (no hardcoded overrides!)
            tracing_subscriber::EnvFilter::from_default_env(),
        )
        .try_init();
}

/// Recursively read all files from a directory and return as FileContent
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
            // Read file content
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
            // Recursively read subdirectories using Box::pin to avoid infinite size
            let sub_files = Box::pin(read_directory_files(&path, base_dir, recursive)).await?;
            files.extend(sub_files);
        }
    }

    Ok(files)
}

#[test]
fn test_index_rust_mini_redis() {
    // Initialize tracing to show timing data
    init_tracing();

    test_utils::get_test_runtime().block_on(async {
        tracing::info!("🦀 Testing indexing with real Rust codebase (mini-redis)");

        // Note: This test requires Qdrant to be running locally on port 6334
        // You can start it with: docker-compose -f docker/docker-compose.qdrant.yml up -d

        // Create all required dependencies
        let config = test_config();

        // create_test_storage handles collection creation automatically
        let storage = create_test_storage("mini_redis_index")
            .await
            .expect("Failed to create storage");

        let embedding_service = create_test_embedding_service();
        let repository = create_test_repository().await;

        // Load tokenizer for accurate chunking
        let code_parser = create_code_parser_with_tokenizer(&embedding_service).await;
        let mut indexer = Indexer::new(
            embedding_service.clone(),
            Arc::new(storage.clone()) as Arc<dyn VectorStorage>,
            repository,
            code_parser,
            &config,
        );

        // Test queries to verify search works
        let test_queries = vec![
            "parse command from client",
            "redis connection handling",
            "async tokio spawn",
            "error handling and logging",
            "pub struct Connection",
            "impl Display",
            "fn new",
            "mutex lock deadlock",
            "tcp socket accept",
            "hash map insert",
        ];

        // Create search service
        let vector_storage = Arc::new(storage.clone()) as Arc<dyn VectorStorage>;
        let db_config = codetriever_config::DatabaseConfig::from_env();
        let pools = codetriever_meta_data::PoolManager::new(
            &db_config,
            codetriever_meta_data::PoolConfig::default(),
        )
        .await
        .expect("Failed to create pool manager");
        let db_client = Arc::new(codetriever_meta_data::DataClient::new(pools));

        let search_service =
            codetriever_search::SearchService::new(embedding_service, vector_storage, db_client);

        // Check if already indexed
        let correlation_id = codetriever_common::CorrelationId::new();
        let test_result = search_service
            .search(test_queries[0], 1, &correlation_id)
            .await;

        if test_result.is_err() || test_result.unwrap().is_empty() {
            tracing::info!("📂 Index is empty, reading mini-redis source files...");

            let manifest_dir = env!("CARGO_MANIFEST_DIR");
            let test_path = Path::new(manifest_dir)
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("test-repos/rust-mini-redis/src");

            // Read all files from directory
            let files = read_directory_files(&test_path, &test_path, true)
                .await
                .expect("Failed to read directory");

            tracing::info!("📄 Read {} files from {:?}", files.len(), test_path);
            tracing::info!("📝 Indexing using index_file_content() (real API)...");

            // Use unique project ID per run to avoid "Unchanged" detection from DB
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            let unique_project_id = format!("mini-redis-{timestamp}:main");

            // Index using the REAL API (not legacy index_directory)
            let result = indexer
                .index_file_content(&unique_project_id, files)
                .await
                .expect("Failed to index");

            tracing::info!(
                "✅ Indexed {} files, {} chunks created, {} stored",
                result.files_indexed,
                result.chunks_created,
                result.chunks_stored
            );
        }

        // Now run test queries to verify search works
        tracing::info!("Running test queries");

        for query in test_queries {
            tracing::debug!("Query: \"{query}\"");

            let correlation_id = codetriever_common::CorrelationId::new();
            let results = search_service
                .search(query, 3, &correlation_id)
                .await
                .expect("Search failed");

            if results.is_empty() {
                tracing::debug!("  No results found for: {query}");
            } else {
                for (i, result) in results.iter().enumerate() {
                    tracing::debug!(
                        "  Result #{} from {}:{}-{} (score: {:.3})",
                        i + 1,
                        result.chunk.file_path,
                        result.chunk.start_line,
                        result.chunk.end_line,
                        result.similarity
                    );

                    // Show first 3 lines of the chunk
                    let preview: Vec<&str> = result.chunk.content.lines().take(3).collect();
                    for line in preview {
                        tracing::trace!("    | {line}");
                    }
                    if result.chunk.content.lines().count() > 3 {
                        tracing::trace!("    | ...");
                    }
                }
            }
        }

        cleanup_test_storage(&storage)
            .await
            .expect("Failed to cleanup");

        tracing::info!("✅ Mini-redis indexing and search test complete!");
    })
}
