//! Codetriever API Server
//!
//! HTTP API server for semantic code search with vector embeddings.

use codetriever_api::{AppState, routes};
use codetriever_config::ApplicationConfig;
use codetriever_embeddings::DefaultEmbeddingService;
use codetriever_indexing::{ServiceConfig, ServiceFactory};
use codetriever_meta_data::{DataClient, DbFileRepository, PoolConfig, PoolManager};
use codetriever_search::SearchService;
use codetriever_vector_data::QdrantStorage;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

type MainResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::main]
async fn main() -> MainResult {
    // Initialize environment (load .env, etc.)
    codetriever_common::initialize_environment();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    info!("Starting Codetriever API server...");

    // Load unified configuration with environment overrides
    let config = ApplicationConfig::from_env();
    info!(
        "Configuration loaded - API port: {}, Database: {}",
        config.api.port,
        config.database.safe_connection_string()
    );

    // Initialize connection pools once at startup
    info!("Initializing database connection pool...");
    let pools = PoolManager::new(&config.database, PoolConfig::default()).await?;
    let db_client_concrete = Arc::new(DataClient::new(pools.clone()));

    // Create file repository for indexer (shares same connection pools)
    let file_repository = Arc::new(DbFileRepository::new(pools))
        as Arc<dyn codetriever_meta_data::traits::FileRepository>;

    info!("Initializing vector storage...");
    let vector_storage = Arc::new(
        QdrantStorage::new(
            config.vector_storage.url.clone(),
            config.vector_storage.collection_name.clone(),
        )
        .await?,
    ) as Arc<dyn codetriever_vector_data::VectorStorage>;

    info!("Initializing embedding service...");
    let embedding_service = Arc::new(DefaultEmbeddingService::new(config.embedding.clone()))
        as Arc<dyn codetriever_embeddings::EmbeddingService>;

    info!("Warming up embedding model (downloading if needed)...");
    embedding_service.provider().ensure_ready().await?;
    info!("Embedding model ready");

    info!("Initializing search service...");
    let search_service = Arc::new(SearchService::new(
        Arc::clone(&embedding_service),
        Arc::clone(&vector_storage),
        Arc::clone(&db_client_concrete),
    )) as Arc<dyn codetriever_search::SearchProvider>;

    info!("Initializing indexer service...");
    let factory = ServiceFactory::new(ServiceConfig::from_env()?);
    let indexer = factory
        .indexer(
            Arc::clone(&embedding_service),
            Arc::clone(&vector_storage),
            Arc::clone(&file_repository),
        )
        .await?;
    let indexer_service =
        Arc::new(Mutex::new(indexer)) as Arc<Mutex<dyn codetriever_indexing::IndexerService>>;

    // Create application state with all services (cast db_client for AppState)
    let db_client = db_client_concrete as Arc<dyn codetriever_api::routes::status::DatabaseClient>;
    let state = AppState::new(db_client, vector_storage, search_service, indexer_service);
    info!("Application state initialized successfully");

    // Create router with state
    let app = routes::create_router(state);

    // Bind to address
    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    info!("Listening on {}", addr);
    info!("🚀 Codetriever API server starting on http://{addr}");

    // Start server using axum's serve function
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("✅ Server is ready to accept connections");
    axum::serve(listener, app).await?;

    Ok(())
}
