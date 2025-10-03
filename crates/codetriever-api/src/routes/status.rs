use async_trait::async_trait;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::time::SystemTime;
use utoipa::ToSchema;

use codetriever_meta_data::{DataClient, DatabaseResult};
use codetriever_vector_data::VectorStorage;

/// Server start time (initialized once on first access)
static SERVER_START_TIME: LazyLock<SystemTime> = LazyLock::new(SystemTime::now);

/// Trait for database clients that can provide status information
#[async_trait]
pub trait DatabaseClient: Send + Sync {
    /// Count total project branches
    async fn count_project_branches(&self) -> DatabaseResult<i64>;
    /// Count total indexed files
    async fn count_indexed_files(&self) -> DatabaseResult<i64>;
    /// Count total chunks
    async fn count_chunks(&self) -> DatabaseResult<i64>;
}

#[async_trait]
impl DatabaseClient for DataClient {
    async fn count_project_branches(&self) -> DatabaseResult<i64> {
        self.count_project_branches().await
    }

    async fn count_indexed_files(&self) -> DatabaseResult<i64> {
        self.count_indexed_files().await
    }

    async fn count_chunks(&self) -> DatabaseResult<i64> {
        self.count_chunks().await
    }
}

#[async_trait]
impl DatabaseClient for codetriever_meta_data::MockDataClient {
    async fn count_project_branches(&self) -> DatabaseResult<i64> {
        self.count_project_branches()
    }

    async fn count_indexed_files(&self) -> DatabaseResult<i64> {
        self.count_indexed_files()
    }

    async fn count_chunks(&self) -> DatabaseResult<i64> {
        self.count_chunks()
    }
}

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
        "total_chunks": 5678
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
}

/// GET /status - Comprehensive system health monitoring
///
/// Returns current system health including service connectivity and index stats
///
/// # Arguments
///
/// * `db_client` - `PostgreSQL` client for index statistics
/// * `vector_storage` - Qdrant storage for health checks
/// * `start_time` - Server start time for uptime calculation
///
/// # Returns
///
/// - Server information (version, uptime)
/// - Service health (`PostgreSQL`, Qdrant connectivity)
/// - Index statistics (files, chunks counts)
pub async fn get_status<C>(
    db_client: &C,
    vector_storage: &impl VectorStorage,
    start_time: std::time::SystemTime,
) -> StatusResponse
where
    C: DatabaseClient,
{
    // Calculate uptime
    let uptime = start_time.elapsed().map(|d| d.as_secs()).unwrap_or(0);

    // Check service health
    let postgres_health = check_postgres_health(db_client).await;
    let qdrant_health = check_qdrant_health(vector_storage).await;

    // Get index statistics
    let (total_files, total_chunks) = get_index_stats(db_client).await;

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
        },
    }
}

async fn check_postgres_health<C: DatabaseClient>(client: &C) -> String {
    // Simple health check - try to count project branches
    match client.count_project_branches().await {
        Ok(_) => "connected".to_string(),
        Err(_) => "disconnected".to_string(),
    }
}

async fn check_qdrant_health<T: VectorStorage>(storage: &T) -> String {
    // Check if collection exists
    match storage.collection_exists().await {
        Ok(true) => "connected".to_string(),
        Ok(false) => "no_collection".to_string(),
        Err(_) => "disconnected".to_string(),
    }
}

async fn get_index_stats<C: DatabaseClient>(client: &C) -> (i64, i64) {
    let files = client.count_indexed_files().await.unwrap_or(0);
    let chunks = client.count_chunks().await.unwrap_or(0);
    (files, chunks)
}

/// Axum handler for GET /status endpoint
///
/// Creates clients on-demand and returns status
pub async fn status_handler() -> Json<StatusResponse> {
    use codetriever_config::{ApplicationConfig, Profile};
    use codetriever_meta_data::{DataClient, PoolConfig, PoolManager};
    use codetriever_vector_data::QdrantStorage;

    // Get config
    let config = ApplicationConfig::with_profile(Profile::Development);

    // Create PostgreSQL client
    let pools = PoolManager::new(&config.database, PoolConfig::default())
        .await
        .ok();
    let db_client = pools.map(DataClient::new);

    // Collect PostgreSQL stats early and drop client
    let (postgres_status, total_files, total_chunks) = if let Some(ref db) = db_client {
        let health = check_postgres_health(db).await;
        let (files, chunks) = get_index_stats(db).await;
        (health, files, chunks)
    } else {
        ("disconnected".to_string(), 0, 0)
    };
    drop(db_client); // Early drop to reduce resource contention

    // Create Qdrant client
    let vector_storage = QdrantStorage::new(
        config.vector_storage.url.clone(),
        config.vector_storage.collection_name.clone(),
    )
    .await
    .ok();

    // Check Qdrant health and drop immediately
    let qdrant_status = if let Some(ref qdrant) = vector_storage {
        check_qdrant_health(qdrant).await
    } else {
        "disconnected".to_string()
    };
    drop(vector_storage); // Early drop

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
        },
    })
}

/// Create status routes
pub fn routes() -> axum::Router {
    use axum::routing::get;
    axum::Router::new().route("/api/status", get(status_handler))
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
            },
        };

        let json = serde_json::to_string(&response)?;
        assert!(json.contains("version"));
        assert!(json.contains("uptime_seconds"));
        assert!(json.contains("total_files"));
        Ok(())
    }

    #[tokio::test]
    async fn test_get_status_with_mock_clients() {
        let db_client = codetriever_meta_data::MockDataClient::new();
        let vector_storage = codetriever_vector_data::MockStorage::new();

        // MockStorage has collection by default after creation
        let start_time = std::time::SystemTime::now();

        let response = get_status(&db_client, &vector_storage, start_time).await;

        // Verify structure
        assert_eq!(response.server.version, env!("CARGO_PKG_VERSION"));
        assert!(response.server.uptime_seconds < 1); // Just started
        assert_eq!(response.services.postgres, "connected");
        // MockStorage collection_exists returns true by default
        assert!(
            response.services.qdrant == "connected" || response.services.qdrant == "no_collection"
        );
        assert_eq!(response.index.total_files, 0); // Empty mock
        assert_eq!(response.index.total_chunks, 0); // Empty mock
    }

    #[tokio::test]
    async fn test_status_calculates_uptime() {
        let db_client = codetriever_meta_data::MockDataClient::new();
        let vector_storage = codetriever_vector_data::MockStorage::new();

        // Simulate server that started 5 seconds ago
        let start_time = std::time::SystemTime::now() - std::time::Duration::from_secs(5);

        let response = get_status(&db_client, &vector_storage, start_time).await;

        assert!(response.server.uptime_seconds >= 5);
        assert!(response.server.uptime_seconds < 10); // Should be close to 5
    }
}
