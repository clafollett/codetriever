//! Integration tests for `/status` endpoint using real Docker services
//!
//! Run with: `cargo test --test status_integration_test --features metal`
//! Requires: Docker services running (`PostgreSQL` + Qdrant)

use codetriever_api::routes::status::get_status;
use codetriever_config::DatabaseConfig;
use codetriever_meta_data::{DataClient, PoolConfig, PoolManager};
use codetriever_vector_data::{QdrantStorage, VectorStorage};
use std::time::SystemTime;

#[tokio::test]
async fn test_status_with_real_postgres_and_qdrant() {
    // Load config with safe defaults
    let db_config = DatabaseConfig::from_env();
    let qdrant_url =
        std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6334".to_string());

    // Initialize real PostgreSQL client
    let pools = match PoolManager::new(&db_config, PoolConfig::default()).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("⚠️  PostgreSQL not available: {e}");
            eprintln!("   Skipping integration test (requires Docker services)");
            return;
        }
    };
    let db_client = DataClient::new(pools);

    // Initialize real Qdrant client
    let vector_storage = match QdrantStorage::new(qdrant_url, "codetriever".to_string()).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("⚠️  Qdrant not available: {e}");
            eprintln!("   Skipping integration test (requires Docker services)");
            return;
        }
    };

    // Ensure collection exists
    if let Err(e) = vector_storage.ensure_collection().await {
        eprintln!("⚠️  Failed to create Qdrant collection: {e}");
        return;
    }

    let start_time = SystemTime::now();

    // Call get_status with real services
    let response = get_status(&db_client, &vector_storage, start_time).await;
    drop(vector_storage); // Early drop

    // Verify response structure
    assert_eq!(response.server.version, env!("CARGO_PKG_VERSION"));
    assert!(response.server.uptime_seconds < 2); // Should be very recent

    // Services should be connected (we just verified above)
    assert_eq!(
        response.services.postgres, "connected",
        "PostgreSQL should be connected"
    );
    assert!(
        response.services.qdrant == "connected" || response.services.qdrant == "no_collection",
        "Qdrant should be connected or have no collection"
    );

    // Stats should be non-negative (real data)
    assert!(
        response.index.total_files >= 0,
        "File count should be non-negative"
    );
    assert!(
        response.index.total_chunks >= 0,
        "Chunk count should be non-negative"
    );

    println!("✅ Integration test passed:");
    println!("   PostgreSQL: {}", response.services.postgres);
    println!("   Qdrant: {}", response.services.qdrant);
    println!("   Files: {}", response.index.total_files);
    println!("   Chunks: {}", response.index.total_chunks);

    // Note: Test data cleanup should be handled by test harness
    // For manual cleanup: just clean-test-data && just db-clean-test-data
}
