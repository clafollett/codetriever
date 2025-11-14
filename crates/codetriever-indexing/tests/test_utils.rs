//! Shared test utilities for integration tests
//!
//! This module provides common testing utilities used across multiple test files.
//! Functions are only compiled into test binaries that actually use them.

use codetriever_config::{ApplicationConfig, DatabaseConfig};
use codetriever_embeddings::EmbeddingService;
use codetriever_meta_data::{
    DataClient,
    pool_manager::{PoolConfig, PoolManager},
    repository::DbFileRepository,
    traits::FileRepository,
};
use codetriever_vector_data::{QdrantStorage, VectorStorage};
use std::sync::{Arc, OnceLock as OnceCell};

// Re-export shared test utilities from codetriever-test-utils
#[allow(unused_imports)]
pub use codetriever_test_utils::{get_shared_embedding_service, get_test_runtime};

/// Create a unique tenant in the database
///
/// Returns the tenant_id for use in tests
#[allow(dead_code)]
pub async fn create_test_tenant(
    repository: &std::sync::Arc<dyn codetriever_meta_data::traits::FileRepository>,
) -> uuid::Uuid {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let tenant_name = format!("test_tenant_{timestamp}");

    repository
        .create_tenant(&tenant_name)
        .await
        .expect("Failed to create tenant")
}

use codetriever_indexing::{
    BackgroundWorker, WorkerConfig,
    indexing::{IndexerService, service::FileContent},
};
use codetriever_meta_data::models::{CommitContext, JobStatus};
use codetriever_parsing::CodeParser;
use uuid::Uuid;

/// Create test commit context with default values
pub fn test_commit_context() -> CommitContext {
    CommitContext {
        repository_url: "https://github.com/test/repo".to_string(),
        commit_sha: "abc123def456".to_string(),
        commit_message: "Test commit message".to_string(),
        commit_date: chrono::Utc::now(),
        author: "Test Author <test@example.com>".to_string(),
    }
}

/// Index files using async job pattern (production flow)
///
/// This helper manages the complete async indexing flow:
/// 1. Creates BackgroundWorker
/// 2. Starts indexing job
/// 3. Polls until complete
/// 4. Returns job result
///
/// Note: Some test files don't use this helper, causing dead_code warnings.
/// This is expected for shared test utilities.
#[allow(dead_code, clippy::too_many_arguments)]
pub async fn index_files_async(
    indexer: &Arc<dyn IndexerService>,
    repository: Arc<dyn FileRepository>,
    embedding_service: Arc<dyn EmbeddingService>,
    qdrant_url: String,
    vector_namespace: String,
    code_parser: Arc<CodeParser>,
    config: &ApplicationConfig,
    tenant_id: Uuid,
    project_id: &str,
    files: Vec<FileContent>,
) -> (Uuid, codetriever_meta_data::models::IndexingJob) {
    // Create background worker with dynamic storage routing
    // Create PostgreSQL chunk queue
    let db_config = codetriever_meta_data::DatabaseConfig::from_env();
    let chunk_queue_pool = db_config
        .create_pool()
        .await
        .expect("Failed to create chunk queue pool");
    let chunk_queue = Arc::new(codetriever_meta_data::PostgresChunkQueue::new(
        chunk_queue_pool,
    ));

    let worker = BackgroundWorker::new(
        Arc::clone(&repository),
        Arc::clone(&embedding_service),
        qdrant_url,
        code_parser,
        WorkerConfig::from_app_config(config),
        chunk_queue,
    );

    // Get shutdown handle before moving worker
    let shutdown = worker.shutdown_handle();

    // Spawn worker in background
    let worker_handle = tokio::spawn(async move {
        worker.run().await;
    });

    // Start indexing job with commit context
    let commit_context = test_commit_context();
    let correlation_id = codetriever_common::CorrelationId::new(); // Generate test correlation ID
    let job_id = indexer
        .start_indexing_job(
            &vector_namespace,
            tenant_id,
            project_id,
            files,
            &commit_context,
            &correlation_id,
        )
        .await
        .expect("Failed to start indexing job");

    // Poll for completion
    let result = loop {
        let job_status = indexer
            .get_job_status(&job_id)
            .await
            .expect("Failed to get status")
            .expect("Job should exist");

        match job_status.status {
            JobStatus::Completed => break (job_id, job_status),
            JobStatus::Failed => panic!("Job failed: {:?}", job_status.error_message),
            _ => tokio::time::sleep(tokio::time::Duration::from_millis(100)).await,
        }
    };

    // Shutdown worker gracefully after job completes
    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = tokio::time::timeout(tokio::time::Duration::from_secs(5), worker_handle).await;

    result
}

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

/// Shared test database resources (ONE pool for ALL tests - prevents exhaustion!)
struct SharedDbResources {
    repository: Arc<dyn FileRepository>,
    #[allow(dead_code)]
    db_client: Arc<DataClient>,
}

static SHARED_DB_RESOURCES: OnceCell<SharedDbResources> = OnceCell::new();

/// Get shared database repository (ONE pool for ALL tests!)
///
/// All tests across ALL test files share the SAME database pool to prevent
/// connection exhaustion. DO NOT create new pools in tests!
pub fn get_shared_test_repository() -> Arc<dyn FileRepository> {
    let resources = SHARED_DB_RESOURCES.get_or_init(|| {
        eprintln!("🗄️  Initializing SHARED database pool (ONE time for ALL tests!)");
        codetriever_common::initialize_environment();

        let db_config = DatabaseConfig::from_env();

        // Use block_in_place to allow async work from within sync initialization
        // This prevents "Cannot start a runtime from within a runtime" errors
        let pools = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(PoolManager::new(&db_config, PoolConfig::default()))
        })
        .expect("Failed to create shared pool manager");

        let repository = Arc::new(DbFileRepository::new(pools.clone()));
        let db_client = Arc::new(DataClient::new(pools));

        SharedDbResources {
            repository,
            db_client,
        }
    });

    Arc::clone(&resources.repository)
}

/// Get shared database client (reuses same pool as repository!)
///
/// Exposed for tests that need DataClient (like Search service integration tests)
#[allow(dead_code)]
pub fn get_shared_db_client() -> Arc<DataClient> {
    // Ensure repository is initialized first (which creates the shared resources)
    let _ = get_shared_test_repository();

    let resources = SHARED_DB_RESOURCES
        .get()
        .expect("DB resources should be initialized by get_shared_test_repository");

    Arc::clone(&resources.db_client)
}

/// Create test repository (actually returns shared singleton - prevents pool exhaustion!)
pub async fn create_test_repository() -> Arc<dyn FileRepository> {
    get_shared_test_repository()
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

    let indexer = codetriever_indexing::indexing::Indexer::new(
        embedding_service,
        Arc::new(storage.clone()) as Arc<dyn VectorStorage>,
        repository,
    );

    Ok((indexer, storage))
}
