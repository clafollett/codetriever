//! Integration test utilities
//!
//! Provides helpers for setting up real application state for integration tests.
//! All integration tests use actual database and vector storage connections.
//!
//! Uses a shared test fixture - `AppState` is initialized once and reused across
//! all integration tests for performance and to avoid race conditions.

use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};

use codetriever_api::AppState;
use codetriever_config::{ApplicationConfig, Profile};
use codetriever_embeddings::DefaultEmbeddingService;
use codetriever_indexing::{ServiceConfig, ServiceFactory};
use codetriever_meta_data::{DataClient, PoolConfig, PoolManager};
use codetriever_search::SearchService;
use codetriever_vector_data::QdrantStorage;

/// Standard test result type for all test functions
pub type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Shared application state for all integration tests
///
/// Initialized once on first access, then reused. This:
/// - Prevents race conditions during Qdrant collection creation
/// - Improves test performance (no repeated pool/service creation)
/// - Mirrors production behavior (one `AppState` for app lifetime)
static SHARED_APP_STATE: OnceCell<AppState> = OnceCell::const_new();

/// Initialize shared application state (internal helper)
async fn init_app_state() -> Result<AppState, Box<dyn std::error::Error>> {
    // Load environment variables from .env file
    codetriever_common::initialize_environment();

    let config = ApplicationConfig::with_profile(Profile::Development);

    // Initialize real services
    let pools = PoolManager::new(&config.database, PoolConfig::default()).await?;
    let db_client_concrete = Arc::new(DataClient::new(pools));

    let vector_storage = Arc::new(
        QdrantStorage::new(
            config.vector_storage.url.clone(),
            config.vector_storage.collection_name.clone(),
        )
        .await?,
    ) as Arc<dyn codetriever_vector_data::VectorStorage>;

    let embedding_service = Arc::new(DefaultEmbeddingService::new(config.embedding.clone()))
        as Arc<dyn codetriever_embeddings::EmbeddingService>;

    let search_service = Arc::new(SearchService::new(
        Arc::clone(&embedding_service),
        Arc::clone(&vector_storage),
        Arc::clone(&db_client_concrete),
    )) as Arc<dyn codetriever_search::SearchProvider>;

    let factory = ServiceFactory::new(ServiceConfig::from_env()?);
    let indexer = factory.indexer(Arc::clone(&embedding_service), Arc::clone(&vector_storage))?;
    let indexer_service =
        Arc::new(Mutex::new(indexer)) as Arc<Mutex<dyn codetriever_indexing::IndexerService>>;

    let db_client = db_client_concrete as Arc<dyn codetriever_api::routes::status::DatabaseClient>;

    Ok(AppState::new(
        db_client,
        vector_storage,
        search_service,
        indexer_service,
    ))
}

/// Get shared application state for integration testing
///
/// Returns a reference to the shared `AppState`, initializing it on first call.
/// All subsequent calls return the same instance for fast, consistent testing.
///
/// # Errors
/// Returns error if services cannot be initialized on first call
pub async fn app_state() -> Result<&'static AppState, Box<dyn std::error::Error>> {
    SHARED_APP_STATE
        .get_or_try_init(|| async { init_app_state().await })
        .await
}
