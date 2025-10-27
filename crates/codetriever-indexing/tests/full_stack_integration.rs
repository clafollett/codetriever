//! Full-stack integration test that verifies data in both PostgreSQL and Qdrant
//!
//! Run with: cargo test --test full_stack_integration -- --test-threads=1

use codetriever_common::CorrelationId;
use codetriever_config::DatabaseConfig;
use codetriever_meta_data::{
    generate_chunk_id,
    pool_manager::{PoolConfig, PoolManager},
    repository::DbFileRepository,
};
use codetriever_parsing::CodeChunk;
use codetriever_vector_data::{QdrantStorage, VectorStorage};
use sqlx::PgPool;
use std::sync::Arc;

async fn get_connection_pool() -> anyhow::Result<PgPool> {
    // Initialize environment for tests (loads .env)
    codetriever_common::initialize_environment();

    // Use DatabaseConfig to get connection details from environment
    let config = DatabaseConfig::from_env();

    // Just connect to the existing database that was set up by 'just init'
    // No need for a separate test database
    let pool = config.create_pool().await?;

    Ok(pool)
}

async fn cleanup_test_data(pool: &PgPool, repo_id: &str, branch: &str) -> anyhow::Result<()> {
    // Clean up test data
    sqlx::query("DELETE FROM project_branches WHERE repository_id = $1 AND branch = $2")
        .bind(repo_id)
        .bind(branch)
        .execute(pool)
        .await?;
    Ok(())
}

#[test]
fn test_full_stack_indexing_with_postgres_and_qdrant() {
    use codetriever_config::ApplicationConfig;
    use codetriever_embeddings::DefaultEmbeddingService;
    use codetriever_indexing::{
        BackgroundWorker, WorkerConfig,
        indexing::{Indexer, IndexerService, service::FileContent},
    };
    use codetriever_meta_data::models::JobStatus;
    use codetriever_parsing::CodeParser;

    codetriever_test_utils::get_test_runtime().block_on(async {
        // Setup
        let pool = get_connection_pool()
            .await
            .expect("Failed to setup test database");
        let pool_config = PoolConfig::default();
        let db_config = DatabaseConfig::from_env();
        let pools = PoolManager::new(&db_config, pool_config)
            .await
            .expect("Failed to create pool manager");
        let repository = Arc::new(DbFileRepository::new(pools))
            as Arc<dyn codetriever_meta_data::traits::FileRepository>;

        let qdrant_url =
            std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6334".to_string());
        let storage = QdrantStorage::new(qdrant_url, "test_full_stack".to_string())
            .await
            .expect("Failed to create Qdrant storage");

        // Create all required dependencies
        let config = ApplicationConfig::from_env();
        let embedding_service = Arc::new(DefaultEmbeddingService::new(config.embedding.clone()))
            as Arc<dyn codetriever_embeddings::EmbeddingService>;
        let vector_storage = Arc::new(storage.clone()) as Arc<dyn VectorStorage>;

        // Create indexer (handles job creation)
        let indexer = Arc::new(Indexer::new(
            Arc::clone(&embedding_service),
            Arc::clone(&vector_storage),
            Arc::clone(&repository),
        )) as Arc<dyn IndexerService>;

        // Create background worker for file processing
        let tokenizer = embedding_service.provider().get_tokenizer().await;
        let code_parser = Arc::new(CodeParser::new(
            tokenizer,
            config.indexing.split_large_units,
            config.indexing.max_chunk_tokens,
        ));

        let worker = BackgroundWorker::new(
            Arc::clone(&repository),
            Arc::clone(&embedding_service),
            Arc::clone(&vector_storage),
            code_parser,
            WorkerConfig::from_app_config(&config),
        );

        // Spawn worker in background
        let _worker_handle = tokio::spawn(async move {
            worker.run().await;
        });

        let test_repo = "test_repo";
        let test_branch = "main";
        let test_file = "src/main.rs";

        // Clean up any existing test data
        cleanup_test_data(&pool, test_repo, test_branch)
            .await
            .expect("Failed to cleanup");

        // Create test content
        let test_content = r#"
/// Main entry point for the application
fn main() {
    println!("Hello from full-stack test!");
}

/// Helper function to process data
pub fn process_data(input: &str) -> String {
    input.to_uppercase()
}
"#;

        let project_id = format!("{test_repo}:{test_branch}");
        let file = FileContent {
            path: test_file.to_string(),
            content: test_content.to_string(),
            hash: String::new(), // Will be computed by indexer
        };

        // Start indexing job (async pattern)
        let job_id = indexer
            .start_indexing_job(&project_id, vec![file])
            .await
            .expect("Failed to start indexing job");

        // Poll for completion
        let mut attempts = 0;
        let job_status = loop {
            attempts += 1;
            let status = indexer
                .get_job_status(&job_id)
                .await
                .expect("Failed to get status")
                .expect("Job should exist");

            match status.status {
                JobStatus::Completed => break status,
                JobStatus::Failed => panic!("Job failed: {:?}", status.error_message),
                _ => {
                    if attempts > 100 {
                        panic!("Job timed out after 100 attempts");
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        };

        println!(
            "Indexed {} files with {} chunks",
            job_status.files_processed, job_status.chunks_created
        );

        // Verify data in PostgreSQL
        let files: Vec<(String,)> = sqlx::query_as(
            "SELECT file_path FROM indexed_files WHERE repository_id = $1 AND branch = $2",
        )
        .bind(test_repo)
        .bind(test_branch)
        .fetch_all(&pool)
        .await
        .expect("Failed to query files");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, test_file);

        // Verify chunks in PostgreSQL
        let chunks: Vec<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT chunk_id FROM code_chunks WHERE repository_id = $1 AND branch = $2",
        )
        .bind(test_repo)
        .bind(test_branch)
        .fetch_all(&pool)
        .await
        .expect("Failed to query chunks");

        assert!(chunks.len() >= 2, "Should have at least 2 chunks");

        // Verify chunks in Qdrant using search
        let correlation_id = CorrelationId::new();
        let query_embedding = embedding_service
            .generate_embeddings(vec!["process data"])
            .await
            .expect("Failed to generate query embedding");

        let search_results = storage
            .search(query_embedding[0].clone(), 5, &correlation_id)
            .await
            .expect("Failed to search");

        assert!(!search_results.is_empty(), "Should find chunks in Qdrant");

        // Clean up
        storage
            .drop_collection()
            .await
            .expect("Failed to drop test collection");

        cleanup_test_data(&pool, test_repo, test_branch)
            .await
            .expect("Failed to cleanup");

        println!("\n🎉 Full-stack integration test passed!");
    })
}

#[test]
fn test_uuid_based_chunk_deletion() {
    codetriever_test_utils::get_test_runtime().block_on(async {
        // Setup
        let pool = get_connection_pool()
            .await
            .expect("Failed to setup test database");
        // Create pool manager from the test pool
        let pool_config = PoolConfig::default();
        let db_config = DatabaseConfig::from_env();
        let pools = PoolManager::new(&db_config, pool_config)
            .await
            .expect("Failed to create pool manager");
        let _repository = Arc::new(DbFileRepository::new(pools));

        let qdrant_url =
            std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6334".to_string());
        let storage = QdrantStorage::new(qdrant_url, "test_uuid_deletion".to_string())
            .await
            .expect("Failed to create Qdrant storage");

        let test_repo = "test_deletion";
        let test_branch = "main";
        let test_file = "test.rs";

        // Clean up any existing test data
        cleanup_test_data(&pool, test_repo, test_branch)
            .await
            .expect("Failed to cleanup");

        // Create test chunks with known UUIDs using byte ranges
        let generation = 1i64;
        let chunk1_id = generate_chunk_id(test_repo, test_branch, test_file, generation, 0, 100);
        let chunk2_id = generate_chunk_id(test_repo, test_branch, test_file, generation, 100, 200);

        println!("Generated chunk IDs:");
        println!("  Chunk 1: {chunk1_id}");
        println!("  Chunk 2: {chunk2_id}");

        // Store chunks in Qdrant with deterministic IDs
        let chunks = vec![
            CodeChunk {
                file_path: test_file.to_string(),
                content: "fn test1() {}".to_string(),
                start_line: 1,
                end_line: 1,
                byte_start: 0,
                byte_end: 100,
                language: "rust".to_string(),
                embedding: Some(vec![0.1; 768]),
                token_count: Some(5),
                kind: Some("function".to_string()),
                name: Some("test1".to_string()),
            },
            CodeChunk {
                file_path: test_file.to_string(),
                content: "fn test2() {}".to_string(),
                start_line: 2,
                end_line: 2,
                byte_start: 100,
                byte_end: 200,
                language: "rust".to_string(),
                embedding: Some(vec![0.2; 768]),
                token_count: Some(5),
                kind: Some("function".to_string()),
                name: Some("test2".to_string()),
            },
        ];

        let correlation_id = CorrelationId::new();

        let stored_ids = storage
            .store_chunks(test_repo, test_branch, &chunks, generation, &correlation_id)
            .await
            .expect("Failed to store chunks with IDs");

        assert_eq!(stored_ids.len(), 2);
        assert_eq!(stored_ids[0], chunk1_id);
        assert_eq!(stored_ids[1], chunk2_id);
        println!("✅ Stored 2 chunks with deterministic UUIDs");

        // Verify chunks exist in Qdrant
        let search_results = storage
            .search(vec![0.15; 768], 10, &correlation_id)
            .await
            .expect("Failed to search");

        assert!(search_results.len() >= 2, "Should find at least 2 chunks");
        println!("✅ Verified chunks exist in Qdrant");

        // Delete the chunks using their UUIDs
        storage
            .delete_chunks(&stored_ids)
            .await
            .expect("Failed to delete chunks");

        println!("✅ Deleted chunks using UUIDs: {stored_ids:?}");

        // Verify chunks are deleted
        let search_after_delete = storage
            .search(vec![0.15; 768], 10, &correlation_id)
            .await
            .expect("Failed to search after delete");

        let remaining = search_after_delete
            .iter()
            .filter(|c| c.chunk.file_path == test_file)
            .count();

        assert_eq!(remaining, 0, "No chunks should remain after deletion");
        println!("✅ Verified chunks were deleted from Qdrant");

        // Clean up
        storage
            .drop_collection()
            .await
            .expect("Failed to drop test collection");

        cleanup_test_data(&pool, test_repo, test_branch)
            .await
            .expect("Failed to cleanup");

        println!("\n🎉 UUID-based deletion test passed!");
    })
}
