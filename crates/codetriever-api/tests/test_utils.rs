//! Integration test utilities
//!
//! Provides helpers for setting up real application state for integration tests.
//! All integration tests use actual database and vector storage connections.
//!
//! Uses shared database pool and embedding service for efficiency,
//! but creates per-test Qdrant collections for isolation.
//!
//! **CRITICAL:** All tests share a single persistent Tokio runtime to prevent
//! "context is being shutdown" errors. Individual test runtimes would shut down
//! prematurely, killing shared resources.

#![allow(clippy::expect_used)] // Test code - expect is acceptable for setup

use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};

use codetriever_api::AppState;
use codetriever_config::ApplicationConfig;
use codetriever_indexing::IndexerService;
use codetriever_meta_data::{
    DataClient, DbFileRepository, PoolConfig, PoolManager, traits::FileRepository,
};
use codetriever_search::Search;
use codetriever_vector_data::{QdrantStorage, VectorStorage};

/// Standard test result type for all test functions
pub type TestResult = Result<(), Box<dyn std::error::Error>>;

// Re-export shared test utilities from codetriever-test-utils crate
// This provides a single runtime and atomic counter shared across ALL test crates
pub use codetriever_test_utils::{get_test_runtime, next_collection_counter};

/// Shared resources that are expensive to initialize
struct SharedResources {
    db_client: Arc<DataClient>,
    file_repository: Arc<dyn FileRepository>,
    embedding_service: Arc<dyn codetriever_embeddings::EmbeddingService>,
    config: ApplicationConfig,
}

/// Shared resources initialized once and reused across all tests
static SHARED_RESOURCES: OnceCell<SharedResources> = OnceCell::const_new();

/// Initialize shared resources (DB pool, embedding service)
async fn init_shared_resources() -> Result<SharedResources, Box<dyn std::error::Error>> {
    eprintln!("🔧 Initializing SharedResources (should only happen once!)");
    codetriever_common::initialize_environment();
    let config = ApplicationConfig::from_env();

    // Create shared database pool
    eprintln!("🔧 Creating database pools...");
    let pools = PoolManager::new(&config.database, PoolConfig::default()).await?;
    let db_client = Arc::new(DataClient::new(pools.clone()));
    eprintln!("✅ Database pools created");

    // Create shared file repository (uses same pools)
    eprintln!("🔧 Creating file repository...");
    let file_repository = Arc::new(DbFileRepository::new(pools)) as Arc<dyn FileRepository>;
    eprintln!("✅ File repository created");

    // Get SHARED embedding service from codetriever-test-utils
    // This is initialized ONCE across ALL test crates (prevents 28GB+ RAM usage)
    eprintln!("🔧 Getting shared embedding service...");
    let embedding_service = codetriever_test_utils::get_shared_embedding_service();
    eprintln!("✅ Embedding service obtained (shared across all tests)");

    eprintln!("✅ SharedResources initialized (shared embedding pool)");

    Ok(SharedResources {
        db_client,
        file_repository,
        embedding_service,
        config,
    })
}

/// Test fixture that owns `AppState` and cleans up Qdrant collection
pub struct TestAppState {
    state: AppState,
    collection_name: String,
    vector_storage: Arc<QdrantStorage>,
    created_at: std::time::Instant, // Track lifetime for debugging
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
        let lifetime = self.created_at.elapsed();
        let collection_name = self.collection_name.clone();
        eprintln!("🗑️  [DROP] TestAppState for {collection_name} (lived {lifetime:?})");

        // Spawn cleanup task on shared runtime
        // NOTE: We can't block_on from within Drop when Drop is called from an async context
        // The runtime will complete spawned tasks before shutdown
        let storage = Arc::clone(&self.vector_storage);
        let name_for_task = collection_name.clone();
        eprintln!("🧹 [DROP] Spawning cleanup task for {collection_name}");

        get_test_runtime().spawn(async move {
            eprintln!("🧹 [CLEANUP] Starting for {name_for_task}");
            match storage.drop_collection().await {
                Ok(_) => eprintln!("✅ [CLEANUP] Dropped collection: {name_for_task}"),
                Err(e) => eprintln!("⚠️  [CLEANUP] Failed to drop {name_for_task}: {e}"),
            }
            eprintln!("🏁 [CLEANUP] Finished for {name_for_task}");
        });

        eprintln!("🗑️  [DROP] Cleanup spawned for {collection_name}");
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

    // Create unique collection name: test_name + timestamp + counter
    // - test_name: easy to identify which test (e.g., "test_search_with_unicode")
    // - timestamp: ensures uniqueness across test runs (avoids collision with orphaned collections)
    // - counter: ensures uniqueness within same run (tests start at same millisecond)
    // Counter is shared across ALL test crates via codetriever-test-utils
    let counter = next_collection_counter();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_millis();

    // Get test name from thread
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .and_then(|name| name.split("::").last())
        .unwrap_or("unknown");

    // Example: test_search_with_unicode_1760197451942_0
    let collection_name = format!("{test_name}_{timestamp}_{counter}");
    eprintln!(
        "🔢 [DEBUG] Creating collection: name={test_name}, ts={timestamp}, counter={counter} → {collection_name}"
    );

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
    let search_service = Arc::new(Search::new(
        Arc::clone(&shared.embedding_service),
        Arc::clone(&vector_storage_trait),
        Arc::clone(&shared.db_client),
    )) as Arc<dyn codetriever_search::SearchService>;

    // Create indexer service directly (no factory!)
    let tokenizer = shared.embedding_service.provider().get_tokenizer().await;
    let code_parser = codetriever_parsing::CodeParser::new(
        tokenizer,
        shared.config.indexing.split_large_units,
        shared.config.indexing.max_chunk_tokens,
    );

    let indexer = codetriever_indexing::indexing::Indexer::new(
        Arc::clone(&shared.embedding_service),
        Arc::clone(&vector_storage_trait),
        Arc::clone(&shared.file_repository),
        code_parser,
        &shared.config,
    );
    let indexer_service = Arc::new(Mutex::new(indexer)) as Arc<Mutex<dyn IndexerService>>;

    let state = AppState::new(
        Arc::clone(&shared.db_client),
        vector_storage_trait,
        search_service,
        indexer_service,
    );

    let test_state = Arc::new(TestAppState {
        state,
        collection_name: collection_name.clone(),
        vector_storage: vector_storage_concrete,
        created_at: std::time::Instant::now(),
    });

    eprintln!("🏗️  [CREATED] TestAppState with collection: {collection_name}");
    Ok(test_state)
}
