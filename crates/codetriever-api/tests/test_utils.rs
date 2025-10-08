//! Integration test utilities
//!
//! Provides helpers for setting up real application state for integration tests.
//! All integration tests use actual database and vector storage connections.
//!
//! Uses shared database pool and embedding service for efficiency,
//! but creates per-test Qdrant collections for isolation.

#![allow(clippy::expect_used)] // Test code - expect is acceptable for setup

use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};

use codetriever_api::AppState;
use codetriever_config::{ApplicationConfig, Profile};
use codetriever_embeddings::DefaultEmbeddingService;
use codetriever_indexing::{ServiceConfig, ServiceFactory};
use codetriever_meta_data::{DataClient, PoolConfig, PoolManager};
use codetriever_search::SearchService;
use codetriever_vector_data::{QdrantStorage, VectorStorage};

/// Standard test result type for all test functions
pub type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Shared resources that are expensive to initialize
struct SharedResources {
    db_client: Arc<DataClient>,
    embedding_service: Arc<dyn codetriever_embeddings::EmbeddingService>,
    config: ApplicationConfig,
}

/// Shared resources initialized once and reused across all tests
static SHARED_RESOURCES: OnceCell<SharedResources> = OnceCell::const_new();

/// Initialize shared resources (DB pool, embedding service)
async fn init_shared_resources() -> Result<SharedResources, Box<dyn std::error::Error>> {
    codetriever_common::initialize_environment();
    let config = ApplicationConfig::with_profile(Profile::Development);

    // Create shared database pool
    let pools = PoolManager::new(&config.database, PoolConfig::default()).await?;
    let db_client = Arc::new(DataClient::new(pools));

    // Create shared embedding service
    let embedding_service = Arc::new(DefaultEmbeddingService::new(config.embedding.clone()))
        as Arc<dyn codetriever_embeddings::EmbeddingService>;

    // Warm up model once
    embedding_service.provider().ensure_ready().await?;

    Ok(SharedResources {
        db_client,
        embedding_service,
        config,
    })
}

/// Test fixture that owns `AppState` and cleans up Qdrant collection
pub struct TestAppState {
    state: AppState,
    collection_name: String,
    vector_storage: Arc<QdrantStorage>,
}

impl TestAppState {
    /// Get the underlying `AppState` for cloning to routers
    #[must_use]
    pub const fn state(&self) -> &AppState {
        &self.state
    }
}

impl Drop for TestAppState {
    fn drop(&mut self) {
        // Schedule async cleanup - spawn a task to delete the collection
        let collection_name = self.collection_name.clone();
        let storage = Arc::clone(&self.vector_storage);

        tokio::spawn(async move {
            if let Err(e) = storage.drop_collection().await {
                eprintln!("⚠️  Failed to cleanup collection {collection_name}: {e}");
            }
        });
    }
}

/// Create application state for integration testing with unique collection
///
/// Each test gets its own Qdrant collection for isolation.
/// Database pool and embedding service are shared across all tests for efficiency.
///
/// Returns `Arc<TestAppState>` so cleanup only happens when all references are dropped.
///
/// # Errors
/// Returns error if services cannot be initialized
///
/// # Panics
/// Panics if system time is before UNIX epoch (should never happen)
pub async fn app_state() -> Result<Arc<TestAppState>, Box<dyn std::error::Error>> {
    // Get or initialize shared resources
    let shared = SHARED_RESOURCES
        .get_or_try_init(|| async { init_shared_resources().await })
        .await?;

    // Create unique collection name for this test
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_millis();
    let collection_name = format!("test_api_{timestamp}");

    // Create per-test Qdrant collection
    let vector_storage_concrete = Arc::new(
        QdrantStorage::new(
            shared.config.vector_storage.url.clone(),
            collection_name.clone(),
        )
        .await?,
    );

    let vector_storage_trait = Arc::clone(&vector_storage_concrete) as Arc<dyn VectorStorage>;

    // Create search service with shared DB/embedding, but unique vector storage
    let search_service = Arc::new(SearchService::new(
        Arc::clone(&shared.embedding_service),
        Arc::clone(&vector_storage_trait),
        Arc::clone(&shared.db_client),
    )) as Arc<dyn codetriever_search::SearchProvider>;

    // Create indexer service
    let factory = ServiceFactory::new(ServiceConfig::from_env()?);
    let indexer = factory.indexer(
        Arc::clone(&shared.embedding_service),
        Arc::clone(&vector_storage_trait),
    )?;
    let indexer_service =
        Arc::new(Mutex::new(indexer)) as Arc<Mutex<dyn codetriever_indexing::IndexerService>>;

    let db_client =
        Arc::clone(&shared.db_client) as Arc<dyn codetriever_api::routes::status::DatabaseClient>;

    let state = AppState::new(
        db_client,
        vector_storage_trait,
        search_service,
        indexer_service,
    );

    Ok(Arc::new(TestAppState {
        state,
        collection_name,
        vector_storage: vector_storage_concrete,
    }))
}
