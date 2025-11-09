//! Integration test using real Rust codebase (mini-redis)
//!
//! Tests parsing, chunking, and embedding using actual production code
//! to validate the entire indexing pipeline with realistic input.

#[path = "test_utils.rs"]
mod test_utils;

use codetriever_indexing::indexing::service::FileContent;
use codetriever_indexing::{BackgroundWorker, Indexer, IndexerService, WorkerConfig};
use codetriever_meta_data::models::JobStatus;
use codetriever_search::SearchService;
use codetriever_vector_data::VectorStorage;
use std::sync::Arc;
use std::{path::Path, time::Duration};
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

        // Create unique tenant for this test
        let tenant_id = test_utils::create_test_tenant(&repository).await;

        let indexer = Arc::new(Indexer::new(
            embedding_service.clone(),
            Arc::new(storage.clone()) as Arc<dyn VectorStorage>,
            repository.clone(),
        )) as Arc<dyn IndexerService>;

        // Load tokenizer for accurate chunking (used by BackgroundWorker)
        let code_parser = Arc::new(create_code_parser_with_tokenizer(&embedding_service).await);

        // Create background worker for processing (tests the real production flow!)
        let worker = BackgroundWorker::new(
            repository,
            embedding_service.clone(),
            config.vector_storage.url.clone(),
            code_parser,
            WorkerConfig::from_app_config(&config),
        );

        // Get shutdown handle before moving worker
        let shutdown = worker.shutdown_handle();

        // Spawn worker in background (simulates production daemon)
        let worker_handle = tokio::spawn(async move {
            worker.run().await;
        });

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
            codetriever_search::Search::new(embedding_service, vector_storage, db_client);

        // Track timing
        let test_start = std::time::Instant::now();

        // Check if already indexed
        let correlation_id = codetriever_common::CorrelationId::new();
        let test_result = search_service
            .search(&tenant_id, test_queries[0], 1, &correlation_id)
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
            tracing::info!("📝 Starting async indexing job (production flow)...");

            // Use unique project ID per run to avoid "Unchanged" detection from DB
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            let unique_project_id = format!("mini-redis-{timestamp}:main");

            // Build commit context for mini-redis repo
            let commit_context = test_utils::test_commit_context();

            // Start indexing job (enqueues files, returns immediately - production API!)
            let job_id = indexer
                .start_indexing_job(
                    storage.collection_name(),
                    tenant_id,
                    &unique_project_id,
                    files,
                    &commit_context,
                )
                .await
                .expect("Failed to start indexing job");

            tracing::info!(job_id = %job_id, "Job created, waiting for completion...");

            // Wait for job completion by polling status
            let mut attempts = 0;
            loop {
                let job_status = indexer
                    .get_job_status(&job_id)
                    .await
                    .expect("Failed to get status")
                    .expect("Job should exist");

                match job_status.status {
                    JobStatus::Completed => {
                        eprintln!(
                            "✅ Job completed in {:.2}s: {} files, {} chunks",
                            test_start.elapsed().as_secs_f64(),
                            job_status.files_processed,
                            job_status.chunks_created
                        );
                        break;
                    }
                    JobStatus::Failed => {
                        panic!("Job failed: {:?}", job_status.error_message);
                    }
                    _ => {
                        // Still processing - wait
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        attempts += 1;
                        if attempts > 600 {
                            // 60 seconds timeout
                            panic!("Job timed out after 60 seconds");
                        }
                    }
                }
            }
        }

        // Now run test queries to verify search works
        let search_start = std::time::Instant::now();
        tracing::info!("Running test queries");

        for query in test_queries {
            tracing::debug!("Query: \"{query}\"");

            let correlation_id = codetriever_common::CorrelationId::new();
            let results = search_service
                .search(&tenant_id, query, 3, &correlation_id)
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

        eprintln!(
            "✅ Search phase completed in {:.2}s",
            search_start.elapsed().as_secs_f64()
        );

        // Shutdown worker before cleanup
        tracing::info!("🛑 Shutting down worker...");
        shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = tokio::time::timeout(tokio::time::Duration::from_secs(5), worker_handle).await;

        cleanup_test_storage(&storage)
            .await
            .expect("Failed to cleanup");

        tracing::info!("✅ Mini-redis indexing and search test complete!");
    })
}
