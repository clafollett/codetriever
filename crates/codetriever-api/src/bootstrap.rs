//! Application bootstrap and service initialization
//!
//! This module handles all service setup and dependency injection for the API server.
//! It separates configuration and initialization logic from the main entry point.

use codetriever_config::ApplicationConfig;
use codetriever_embeddings::DefaultEmbeddingService;
use codetriever_indexing::{BackgroundWorker, IndexerService, WorkerConfig};
use codetriever_meta_data::{DataClient, DbFileRepository, PoolConfig, PoolManager};
use codetriever_search::{Search, SearchService};
use codetriever_vector_data::QdrantStorage;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tracing::info;

use crate::AppState;

/// Bootstrap result type
pub type BootstrapResult<T> = Result<T, Box<dyn std::error::Error>>;

/// Type alias for search service handle
type SearchServiceHandle = Arc<dyn SearchService>;

/// Initialize database connection pools
///
/// # Errors
///
/// Returns error if database connection fails
pub async fn setup_database(config: &ApplicationConfig) -> BootstrapResult<Arc<DataClient>> {
    info!("Initializing database connection pool...");
    let pools = PoolManager::new(&config.database, PoolConfig::default()).await?;
    let db_client = Arc::new(DataClient::new(pools));
    Ok(db_client)
}

/// Initialize vector storage (Qdrant)
///
/// # Errors
///
/// Returns error if Qdrant connection fails
pub async fn setup_vector_storage(
    config: &ApplicationConfig,
) -> BootstrapResult<Arc<dyn codetriever_vector_data::VectorStorage>> {
    info!("Initializing vector storage...");
    let storage = Arc::new(
        QdrantStorage::new(
            config.vector_storage.url.clone(),
            config.vector_storage.namespace.clone(),
        )
        .await?,
    ) as Arc<dyn codetriever_vector_data::VectorStorage>;
    Ok(storage)
}

/// Initialize embedding service and ensure model is ready
///
/// # Errors
///
/// Returns error if embedding model fails to load
pub async fn setup_embedding_service(
    config: &ApplicationConfig,
) -> BootstrapResult<Arc<dyn codetriever_embeddings::EmbeddingService>> {
    info!("Initializing embedding service...");
    let embedding_service = Arc::new(DefaultEmbeddingService::new(config.embedding.clone()))
        as Arc<dyn codetriever_embeddings::EmbeddingService>;

    info!("Warming up embedding model (downloading if needed)...");
    embedding_service.provider().ensure_ready().await?;
    info!("Embedding model ready");

    Ok(embedding_service)
}

/// Initialize search service with all dependencies
///
/// # Errors
///
/// Returns error if search service initialization fails
pub fn setup_search_service(
    embedding_service: &Arc<dyn codetriever_embeddings::EmbeddingService>,
    vector_storage: &Arc<dyn codetriever_vector_data::VectorStorage>,
    db_client: &Arc<DataClient>,
) -> BootstrapResult<SearchServiceHandle> {
    info!("Initializing search service...");
    let search_service = Arc::new(Search::new(
        Arc::clone(embedding_service),
        Arc::clone(vector_storage),
        Arc::clone(db_client),
    )) as Arc<dyn SearchService>;
    Ok(search_service)
}

/// Initialize indexer service with all dependencies
///
/// # Errors
///
/// Returns error if indexer initialization or tokenizer loading fails
#[allow(clippy::type_complexity)]
pub fn setup_indexer_service(
    embedding_service: &Arc<dyn codetriever_embeddings::EmbeddingService>,
    vector_storage: &Arc<dyn codetriever_vector_data::VectorStorage>,
    pools: &PoolManager,
) -> BootstrapResult<Arc<dyn IndexerService>> {
    info!("Initializing indexer service...");

    // Create file repository for indexer
    let file_repository = Arc::new(DbFileRepository::new(pools.clone()))
        as Arc<dyn codetriever_meta_data::traits::FileRepository>;

    // Create indexer (handles job creation only, not file processing)
    // File processing is done by BackgroundWorker which has its own CodeParser
    let indexer = codetriever_indexing::indexing::Indexer::new(
        Arc::clone(embedding_service),
        Arc::clone(vector_storage),
        Arc::clone(&file_repository),
    );

    let indexer_service = Arc::new(indexer) as Arc<dyn IndexerService>;
    Ok(indexer_service)
}

/// Initialize all services and create application state
///
/// This is the main bootstrap function that orchestrates all service initialization
/// in the correct dependency order.
///
/// # Errors
///
/// Returns error if any service initialization fails
pub async fn initialize_app_state(config: &ApplicationConfig) -> BootstrapResult<AppState> {
    // 1. Database (needed by most services)
    let db_client = setup_database(config).await?;

    // 2. Vector storage (needed by search and indexer)
    let vector_storage = setup_vector_storage(config).await?;

    // 3. Embedding service (needed by search and indexer)
    let embedding_service = setup_embedding_service(config).await?;

    // 4. Search service (depends on embeddings, vector storage, database)
    let search_service = setup_search_service(&embedding_service, &vector_storage, &db_client)?;

    // 5. Indexer service (depends on embeddings, vector storage, database)
    // Note: Indexer needs PoolManager, so we reconstruct it here
    // TODO: Refactor to avoid reconstructing pools
    let pools = PoolManager::new(&config.database, PoolConfig::default()).await?;
    let indexer_service = setup_indexer_service(&embedding_service, &vector_storage, &pools)?;

    // 6. Spawn background worker for processing indexing jobs
    let _shutdown_handle =
        spawn_background_worker(config, Arc::clone(&embedding_service), &pools).await?;

    // 7. Create application state
    let state = AppState::new(
        Arc::clone(&db_client),
        Arc::clone(&vector_storage),
        Arc::clone(&search_service),
        Arc::clone(&indexer_service),
        config.vector_storage.namespace.clone(),
    );

    info!("Application state initialized successfully");
    Ok(state)
}

/// Spawn background worker for processing indexing jobs
///
/// Creates a background worker thread that continuously processes jobs from the
/// `PostgreSQL` queue. The worker runs independently of HTTP requests and can be
/// easily extracted into a separate daemon binary later.
///
/// # Returns
///
/// Returns a shutdown handle that can be used for graceful shutdown
///
/// # Errors
///
/// Returns error if tokenizer loading fails
#[allow(clippy::significant_drop_tightening)] // Worker is spawned, not dropped
pub async fn spawn_background_worker(
    config: &ApplicationConfig,
    embedding_service: Arc<dyn codetriever_embeddings::EmbeddingService>,
    pools: &PoolManager,
) -> BootstrapResult<Arc<AtomicBool>> {
    info!("🚀 Spawning background indexing worker...");

    // Create file repository for worker
    let file_repository = Arc::new(DbFileRepository::new(pools.clone()))
        as Arc<dyn codetriever_meta_data::traits::FileRepository>;

    // Load tokenizer for worker's code parser
    let tokenizer = embedding_service.provider().get_tokenizer().await;
    let code_parser = Arc::new(codetriever_parsing::CodeParser::new(
        tokenizer,
        config.indexing.split_large_units,
        config.indexing.max_chunk_tokens,
    ));

    // Create worker config
    let worker_config = WorkerConfig::from_app_config(config);

    // Create background worker with dynamic storage routing
    let worker = BackgroundWorker::new(
        file_repository,
        Arc::clone(&embedding_service),
        config.vector_storage.url.clone(),
        code_parser,
        worker_config,
    );

    let shutdown_handle = worker.shutdown_handle();

    // Spawn worker in background thread (worker is moved here)
    tokio::spawn(async move {
        worker.run().await;
    });

    info!("✅ Background indexing worker started");

    Ok(shutdown_handle)
}
