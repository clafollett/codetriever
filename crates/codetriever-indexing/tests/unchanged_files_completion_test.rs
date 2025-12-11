// Test that jobs complete even when all files are unchanged
//
// Run with: cargo test --test unchanged_files_completion_test -- --nocapture

mod test_utils;

use codetriever_config::{ApplicationConfig, DatabaseConfig};
use codetriever_embeddings::{DefaultEmbeddingService, EmbeddingService};
use codetriever_indexing::indexing::{Indexer, IndexerService, service::FileContent};
use codetriever_meta_data::{
    models::JobStatus,
    pool_manager::{PoolConfig, PoolManager},
    repository::DbFileRepository,
    traits::FileRepository,
};
use codetriever_vector_data::{QdrantStorage, VectorStorage};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

/// Helper to create FileContent with hash calculated from content (matches worker behavior)
fn file_content(path: &str, content: &str) -> FileContent {
    let hash = codetriever_meta_data::hash_content(content);

    FileContent {
        path: path.to_string(),
        content: content.to_string(),
        hash,
    }
}

/// Test that re-indexing unchanged files completes the job correctly
#[test]
fn test_unchanged_files_job_completion() {
    // Initialize test environment
    codetriever_common::initialize_environment();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let test_id = format!("test-unchanged-{}", Uuid::new_v4());
        let config = ApplicationConfig::from_env();

        // Setup database pools
        let db_config = DatabaseConfig::from_env();
        let pool_config = PoolConfig::default();
        let pools = PoolManager::new(&db_config, pool_config)
            .await
            .expect("Failed to create pool manager");

        let repository = Arc::new(DbFileRepository::new(pools)) as Arc<dyn FileRepository>;

        // Setup Qdrant
        let qdrant_storage = Arc::new(
            QdrantStorage::new(config.vector_storage.url.clone(), test_id.clone())
                .await
                .expect("Failed to create Qdrant storage"),
        );

        // Setup embedding service and code parser
        let embedding_service = Arc::new(DefaultEmbeddingService::new(config.embedding.clone()));
        let tokenizer = embedding_service.provider().get_tokenizer().await;
        let code_parser = Arc::new(codetriever_parsing::CodeParser::new(
            tokenizer,
            config.indexing.split_large_units,
            config.indexing.max_chunk_tokens,
        ));

        // Create indexer
        let indexer = Arc::new(Indexer::new(
            Arc::clone(&embedding_service) as Arc<dyn codetriever_embeddings::EmbeddingService>,
            Arc::clone(&qdrant_storage) as Arc<dyn VectorStorage>,
            Arc::clone(&repository),
        ));

        // Create tenant
        let tenant_id = test_utils::create_test_tenant(&repository).await;
        let repository_id = &test_id;
        let branch = "main";

        // Test files
        let files = vec![
            file_content("src/lib.rs", "fn main() { println!(\"Hello\"); }"),
            file_content("src/utils.rs", "pub fn helper() -> i32 { 42 }"),
        ];

        println!("\n=== FIRST INDEX (new files) ===");

        // First index using the async helper
        let (_job1_id, job1) = test_utils::index_files_async(
            &(Arc::clone(&indexer) as Arc<dyn IndexerService>),
            Arc::clone(&repository) as Arc<dyn FileRepository>,
            Arc::clone(&embedding_service) as Arc<dyn codetriever_embeddings::EmbeddingService>,
            config.vector_storage.url.clone(),
            test_id.clone(),
            Arc::clone(&code_parser),
            &config,
            tenant_id,
            &format!("{repository_id}:{branch}"),
            files.clone(),
        )
        .await;

        assert_eq!(
            job1.status,
            JobStatus::Completed,
            "First job should complete"
        );
        assert_eq!(job1.files_processed, 2, "First job should process 2 files");
        assert!(job1.chunks_created > 0, "First job should create chunks");

        println!(
            "First index: {} files, {} chunks created",
            job1.files_processed, job1.chunks_created
        );

        println!("\n=== SECOND INDEX (unchanged files) ===");

        // Second index - same files, same content, same hash
        let (_job2_id, job2) = test_utils::index_files_async(
            &(Arc::clone(&indexer) as Arc<dyn IndexerService>),
            Arc::clone(&repository) as Arc<dyn FileRepository>,
            Arc::clone(&embedding_service) as Arc<dyn codetriever_embeddings::EmbeddingService>,
            config.vector_storage.url.clone(),
            test_id.clone(),
            Arc::clone(&code_parser),
            &config,
            tenant_id,
            &format!("{repository_id}:{branch}"),
            files, // Same files!
        )
        .await;

        // THIS IS THE BUG WE'RE TESTING!
        assert_eq!(
            job2.status,
            JobStatus::Completed,
            "Second job should complete even with all unchanged files"
        );
        assert_eq!(
            job2.files_processed, 2,
            "Second job should process 2 files (skipped but counted)"
        );
        assert_eq!(
            job2.chunks_created, 0,
            "Second job should create 0 new chunks (all unchanged)"
        );

        println!(
            "Second index: {} files processed, {} chunks created (all unchanged)",
            job2.files_processed, job2.chunks_created
        );

        println!("\n✅ Test passed: Job completes correctly with all unchanged files");

        // Cleanup
        qdrant_storage.drop_collection().await.ok();
    });
}

/// Test mixed scenario: some files changed, some unchanged
#[test]
fn test_mixed_changed_unchanged_files() {
    codetriever_common::initialize_environment();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let test_id = format!("test-mixed-{}", Uuid::new_v4());
        let config = ApplicationConfig::from_env();

        // Setup
        let db_config = DatabaseConfig::from_env();
        let pool_config = PoolConfig::default();
        let pools = PoolManager::new(&db_config, pool_config)
            .await
            .expect("Failed to create pool manager");
        let repository = Arc::new(DbFileRepository::new(pools)) as Arc<dyn FileRepository>;
        let qdrant_storage = Arc::new(
            QdrantStorage::new(config.vector_storage.url.clone(), test_id.clone())
                .await
                .expect("Failed to create Qdrant"),
        );
        let embedding_service = Arc::new(DefaultEmbeddingService::new(config.embedding.clone()));
        let tokenizer = embedding_service.provider().get_tokenizer().await;
        let code_parser = Arc::new(codetriever_parsing::CodeParser::new(
            tokenizer,
            config.indexing.split_large_units,
            config.indexing.max_chunk_tokens,
        ));

        let indexer = Arc::new(Indexer::new(
            Arc::clone(&embedding_service) as Arc<dyn codetriever_embeddings::EmbeddingService>,
            Arc::clone(&qdrant_storage) as Arc<dyn VectorStorage>,
            Arc::clone(&repository),
        ));

        // Create tenant
        let tenant_id = test_utils::create_test_tenant(&repository).await;
        let repository_id = &test_id;
        let branch = "main";

        // Initial files
        let initial_files = vec![
            file_content("src/lib.rs", "fn main() { println!(\"Hello\"); }"),
            file_content("src/utils.rs", "pub fn helper() -> i32 { 42 }"),
        ];

        println!("\n=== FIRST INDEX ===");
        println!("Initial file 0: {}", initial_files[0].content);
        println!("Initial file 1: {}", initial_files[1].content);

        let (_, job1) = test_utils::index_files_async(
            &(Arc::clone(&indexer) as Arc<dyn IndexerService>),
            Arc::clone(&repository) as Arc<dyn FileRepository>,
            Arc::clone(&embedding_service) as Arc<dyn codetriever_embeddings::EmbeddingService>,
            config.vector_storage.url.clone(),
            test_id.clone(),
            Arc::clone(&code_parser),
            &config,
            tenant_id,
            &format!("{repository_id}:{branch}"),
            initial_files,
        )
        .await;

        let initial_chunks = job1.chunks_created;

        // Debug: Check what's in indexed_files table
        let db_config = DatabaseConfig::from_env();
        let debug_pool = db_config.create_pool().await.expect("Failed to create debug pool");
        let indexed_files = sqlx::query(
            "SELECT file_path, content_hash FROM indexed_files WHERE repository_id = $1 AND branch = $2 ORDER BY file_path"
        )
        .bind(repository_id)
        .bind(branch)
        .fetch_all(&debug_pool)
        .await
        .expect("Failed to query indexed_files");

        println!("\nIndexed files after FIRST index:");
        for row in &indexed_files {
            let path: String = row.get("file_path");
            let hash: String = row.get("content_hash");
            println!("  {path} -> {hash}");
        }

        // Second index: one file changed, one unchanged
        let mixed_files = vec![
            file_content("src/lib.rs", "fn main() { println!(\"Hello, World!\"); }"), // CHANGED content
            file_content("src/utils.rs", "pub fn helper() -> i32 { 42 }"), // UNCHANGED content
        ];

        println!("\n=== SECOND INDEX (1 changed, 1 unchanged) ===");
        println!("Total files to index: {}", mixed_files.len());
        println!(
            "Changed file: path={}, hash={}, content={}",
            mixed_files[0].path, mixed_files[0].hash, mixed_files[0].content
        );
        println!(
            "Unchanged file: path={}, hash={}, content={}",
            mixed_files[1].path, mixed_files[1].hash, mixed_files[1].content
        );

        let (job2_id, job2) = test_utils::index_files_async(
            &(Arc::clone(&indexer) as Arc<dyn IndexerService>),
            Arc::clone(&repository) as Arc<dyn FileRepository>,
            Arc::clone(&embedding_service) as Arc<dyn codetriever_embeddings::EmbeddingService>,
            config.vector_storage.url.clone(),
            test_id.clone(),
            Arc::clone(&code_parser),
            &config,
            tenant_id,
            &format!("{repository_id}:{branch}"),
            mixed_files,
        )
        .await;

        // Debug: Check file queue
        let queued_files = sqlx::query(
            "SELECT file_path, status FROM indexing_job_file_queue WHERE job_id = $1 ORDER BY file_path"
        )
        .bind(job2_id)
        .fetch_all(&debug_pool)
        .await
        .expect("Failed to query file queue");

        println!("\nFile queue for job2:");
        for row in queued_files {
            let path: String = row.get("file_path");
            let status: String = row.get("status");
            println!("  {path} -> {status}");
        }

        println!(
            "Job2 result: files_processed={}, chunks_created={}, status={:?}",
            job2.files_processed, job2.chunks_created, job2.status
        );

        assert_eq!(
            job2.status,
            JobStatus::Completed,
            "Mixed job should complete"
        );
        assert_eq!(
            job2.files_processed, 2,
            "Mixed job should process 2 files (1 changed, 1 unchanged)"
        );
        assert!(
            job2.chunks_created > 0 && job2.chunks_created < initial_chunks,
            "Mixed job should create fewer chunks than initial (only 1 file changed)"
        );

        println!(
            "Second index: {} files, {} new chunks",
            job2.files_processed, job2.chunks_created
        );
        println!("✅ Test passed: Mixed changed/unchanged files handled correctly");

        // Cleanup
        qdrant_storage.drop_collection().await.ok();
    });
}
