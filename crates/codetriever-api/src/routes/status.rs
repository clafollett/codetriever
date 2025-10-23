use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::time::SystemTime;
use utoipa::ToSchema;

use codetriever_meta_data::DataClient;
use codetriever_vector_data::VectorStorage;

use crate::state::AppState;

/// Server start time (initialized once on first access)
static SERVER_START_TIME: LazyLock<SystemTime> = LazyLock::new(SystemTime::now);

// Removed DatabaseClient trait - unnecessary abstraction layer
// DataClient is used directly throughout the API

/// Server status information
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({
    "server": {
        "version": "0.1.0",
        "uptime_seconds": 3600
    },
    "services": {
        "postgres": "connected",
        "qdrant": "connected"
    },
    "index": {
        "total_files": 1234,
        "total_chunks": 5678,
        "db_size_mb": 125.4,
        "last_indexed_at": "2025-10-19T14:30:00Z"
    }
}))]
pub struct StatusResponse {
    pub server: ServerInfo,
    pub services: ServiceHealth,
    pub index: IndexInfo,
}

/// Server information
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ServerInfo {
    /// API version
    pub version: String,
    /// Server uptime in seconds
    pub uptime_seconds: u64,
}

/// Service health status
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ServiceHealth {
    /// `PostgreSQL` connection status ("connected", "disconnected")
    pub postgres: String,
    /// Qdrant connection status ("connected", "`no_collection`", "disconnected")
    pub qdrant: String,
}

/// Index statistics
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IndexInfo {
    /// Total number of indexed files
    pub total_files: i64,
    /// Total number of code chunks
    pub total_chunks: i64,
    /// Database size in megabytes (`PostgreSQL` only)
    pub db_size_mb: f64,
    /// Most recent indexed timestamp (ISO 8601 format)
    pub last_indexed_at: Option<String>,
}

/// GET /status - Comprehensive system health monitoring (with `DataClient`)
///
/// Returns current system health including service connectivity and index stats
///
/// # Arguments
///
/// * `db_client` - `DataClient` for index statistics
/// * `vector_storage` - Qdrant storage for health checks
/// * `start_time` - Server start time for uptime calculation
///
/// # Returns
///
/// - Server information (version, uptime)
/// - Service health (`PostgreSQL`, Qdrant connectivity)
/// - Index statistics (files, chunks counts)
pub async fn get_status_with_client(
    db_client: &DataClient,
    vector_storage: &impl VectorStorage,
    start_time: std::time::SystemTime,
) -> StatusResponse {
    // Calculate uptime
    let uptime = start_time.elapsed().map(|d| d.as_secs()).unwrap_or(0);

    // Check service health
    let postgres_health = check_postgres_health_client(db_client).await;
    let qdrant_health = check_qdrant_health(vector_storage).await;

    // Get index statistics
    let (total_files, total_chunks, db_size_mb, last_indexed_at) =
        get_index_stats_client(db_client).await;

    StatusResponse {
        server: ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: uptime,
        },
        services: ServiceHealth {
            postgres: postgres_health,
            qdrant: qdrant_health,
        },
        index: IndexInfo {
            total_files,
            total_chunks,
            db_size_mb,
            last_indexed_at,
        },
    }
}

async fn check_postgres_health_client(client: &DataClient) -> String {
    // Simple health check - try to count project branches
    match client.count_project_branches().await {
        Ok(_) => "connected".to_string(),
        Err(_) => "disconnected".to_string(),
    }
}

async fn check_qdrant_health<T: VectorStorage + ?Sized>(storage: &T) -> String {
    // Check if collection exists
    match storage.collection_exists().await {
        Ok(true) => "connected".to_string(),
        Ok(false) => "no_collection".to_string(),
        Err(_) => "disconnected".to_string(),
    }
}

async fn get_index_stats_client(client: &DataClient) -> (i64, i64, f64, Option<String>) {
    let files = client.count_indexed_files().await.unwrap_or(0);
    let chunks = client.count_chunks().await.unwrap_or(0);
    let db_size_mb = client.get_database_size_mb().await.unwrap_or(0.0);
    let last_indexed_at = client
        .get_last_indexed_timestamp()
        .await
        .unwrap_or(None)
        .map(|dt| dt.to_rfc3339());
    (files, chunks, db_size_mb, last_indexed_at)
}

/// Axum handler for GET /status endpoint
///
/// Uses shared application state to avoid creating pools on every request
pub async fn status_handler(State(state): State<AppState>) -> Json<StatusResponse> {
    // Check PostgreSQL health and get stats
    let postgres_status = check_postgres_health_client(&state.db_client).await;
    let (total_files, total_chunks, db_size_mb, last_indexed_at) =
        get_index_stats_client(&state.db_client).await;

    // Check Qdrant health
    let qdrant_status = check_qdrant_health(state.vector_storage.as_ref()).await;

    // Calculate server uptime
    let uptime = SERVER_START_TIME
        .elapsed()
        .map(|d| d.as_secs())
        .unwrap_or(0);

    Json(StatusResponse {
        server: ServerInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: uptime,
        },
        services: ServiceHealth {
            postgres: postgres_status,
            qdrant: qdrant_status,
        },
        index: IndexInfo {
            total_files,
            total_chunks,
            db_size_mb,
            last_indexed_at,
        },
    })
}

/// Create status routes with application state
///
/// # Arguments
/// * `state` - Shared application state with database and vector storage clients
///
/// # Returns
/// A stateless router with state baked in (ready to merge with other routers)
pub fn routes(state: AppState) -> axum::Router {
    use axum::routing::get;
    axum::Router::new()
        .route("/api/status", get(status_handler))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_response_serialization() -> Result<(), serde_json::Error> {
        let response = StatusResponse {
            server: ServerInfo {
                version: "0.1.0".to_string(),
                uptime_seconds: 100,
            },
            services: ServiceHealth {
                postgres: "connected".to_string(),
                qdrant: "connected".to_string(),
            },
            index: IndexInfo {
                total_files: 10,
                total_chunks: 50,
                db_size_mb: 125.4,
                last_indexed_at: Some("2025-10-19T14:30:00Z".to_string()),
            },
        };

        let json = serde_json::to_string(&response)?;
        assert!(json.contains("version"));
        assert!(json.contains("uptime_seconds"));
        assert!(json.contains("total_files"));
        assert!(json.contains("db_size_mb"));
        assert!(json.contains("last_indexed_at"));
        Ok(())
    }

    // TODO: Re-enable once we refactor to use trait-based DI consistently
    // These tests are disabled because AppState.db_client is now concrete DataClient
    // Will be fixed when we update AppState to use Arc<dyn FileRepository>

    /*
    #[tokio::test]
    async fn test_status_handler_with_app_state() {
        use axum::extract::State;

        // Use the test helper to create mock state
        let state = crate::test_utils::mock_app_state();

        // Call handler with state
        let Json(response) = status_handler(State(state)).await;

        // Verify response structure
        assert_eq!(response.server.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(response.services.postgres, "connected");
        assert_eq!(response.index.total_files, 0);
        assert_eq!(response.index.total_chunks, 0);
        assert!((response.index.db_size_mb - 1.0).abs() < f64::EPSILON);
        assert_eq!(response.index.last_indexed_at, None);
    }
    */
}
