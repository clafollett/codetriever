//! Codetriever API Server
//!
//! HTTP API server for semantic code search with vector embeddings.

use codetriever_api::{bootstrap, routes};
use codetriever_config::ApplicationConfig;
use std::net::SocketAddr;
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

    // Load configuration
    let config = ApplicationConfig::from_env();
    info!(
        "Configuration loaded - API port: {}, Database: {}",
        config.api.port,
        config.database.safe_connection_string()
    );

    // Bootstrap all services (database, vector storage, embeddings, search, indexer)
    let state = bootstrap::initialize_app_state(&config).await?;

    // Create router with state
    let app = routes::create_router(state);

    // Bind to address and start server
    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    info!("Listening on {}", addr);
    info!("🚀 Codetriever API server starting on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("✅ Server is ready to accept connections");
    axum::serve(listener, app).await?;

    Ok(())
}
