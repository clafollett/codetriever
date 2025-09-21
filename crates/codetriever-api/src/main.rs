//! Codetriever API Server
//!
//! HTTP API server for semantic code search with vector embeddings.

use codetriever_api::routes;
use codetriever_config::{ApplicationConfig, Profile};
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

    // Load unified configuration with environment overrides
    let config = ApplicationConfig::with_profile(Profile::Development);
    info!(
        "Configuration loaded - API port: {}, Database: {}",
        config.api.port,
        config.database.safe_connection_string()
    );

    // Create router
    let app = routes::create_router();

    // Bind to address
    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    info!("Listening on {}", addr);
    println!("🚀 Codetriever API server starting on http://{addr}");

    // Start server using axum's serve function
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("✅ Server is ready to accept connections");
    axum::serve(listener, app).await?;

    Ok(())
}
