//! Application bootstrap and service initialization
//!
//! This module handles all service setup and dependency injection for the API server.
//! It separates configuration and initialization logic from the main entry point.

use codetriever_config::ApplicationConfig;
use codetriever_embeddings::DefaultEmbeddingService;
use codetriever_indexing::IndexerService;
use codetriever_meta_data::{DataClient, DbFileRepository, PoolConfig, PoolManager};
use codetriever_search::{Search, SearchService};
use codetriever_vector_data::QdrantStorage;
use std::sync::Arc;
use tokio::sync::Mutex;
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
            config.vector_storage.collection_name.clone(),
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
pub async fn setup_indexer_service(
    config: &ApplicationConfig,
    embedding_service: Arc<dyn codetriever_embeddings::EmbeddingService>,
    vector_storage: Arc<dyn codetriever_vector_data::VectorStorage>,
    pools: &PoolManager,
) -> BootstrapResult<Arc<Mutex<dyn IndexerService>>> {
    info!("Initializing indexer service...");

    // Create file repository for indexer
    let file_repository = Arc::new(DbFileRepository::new(pools.clone()))
        as Arc<dyn codetriever_meta_data::traits::FileRepository>;

    // Load tokenizer from embedding service for accurate chunking
    let tokenizer = embedding_service.provider().get_tokenizer().await;

    // Create CodeParser with tokenizer
    let code_parser = codetriever_parsing::CodeParser::new(
        tokenizer,
        config.indexing.split_large_units,
        config.indexing.max_chunk_tokens,
    );

    // Create indexer directly (no factory!)
    let indexer = codetriever_indexing::indexing::Indexer::new(
        Arc::clone(&embedding_service),
        Arc::clone(&vector_storage),
        Arc::clone(&file_repository),
        code_parser,
        config,
    );

    let indexer_service = Arc::new(Mutex::new(indexer)) as Arc<Mutex<dyn IndexerService>>;
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
    let indexer_service = setup_indexer_service(
        config,
        Arc::clone(&embedding_service),
        Arc::clone(&vector_storage),
        &pools,
    )
    .await?;

    // 6. Create application state
    let state = AppState::new(db_client, vector_storage, search_service, indexer_service);

    info!("Application state initialized successfully");
    Ok(state)
}
