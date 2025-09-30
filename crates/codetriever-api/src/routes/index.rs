use super::response::ResponseStatus;
use crate::impl_has_status;
use axum::{
    Router,
    extract::{Json, State},
    response::IntoResponse,
    routing::post,
};
use codetriever_config::{ApplicationConfig, Profile};
use codetriever_indexing::{Indexer, IndexerService};
use codetriever_meta_data::{
    DbFileRepository,
    pool_manager::{PoolConfig, PoolManager},
};
use codetriever_vector_data::QdrantStorage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use utoipa::ToSchema;

// Type alias to simplify the complex indexer service type
type IndexerServiceHandle = Arc<Mutex<dyn IndexerService>>;

pub fn routes() -> Router {
    // Create a lazy-initialized indexer wrapper
    // Storage will be initialized on first use, not at startup
    let indexer_wrapper = Arc::new(Mutex::new(LazyIndexer::new()));
    routes_with_indexer(indexer_wrapper)
}

/// Create a properly configured indexer with storage and repository
async fn create_configured_indexer() -> Indexer {
    // Load configuration
    let config = ApplicationConfig::with_profile(Profile::Development);

    // Set up database repository
    let pools = match PoolManager::new(&config.database, PoolConfig::default()).await {
        Ok(pools) => pools,
        Err(e) => {
            tracing::error!("Failed to create pool manager: {e}");
            // Return indexer without repository - will work for in-memory operations
            return Indexer::new();
        }
    };
    let repository = Arc::new(DbFileRepository::new(pools));

    // Create indexer with repository
    let mut indexer = Indexer::new_with_repository(repository);

    // Set up vector storage (Qdrant)
    // Try to connect, but if it fails, log and continue without storage
    match QdrantStorage::new(
        config.vector_storage.url.clone(),
        config.vector_storage.collection_name.clone(),
    )
    .await
    {
        Ok(storage) => {
            tracing::info!("Connected to Qdrant storage successfully");
            indexer.set_storage(storage);
        }
        Err(e) => {
            tracing::warn!("Could not connect to Qdrant: {e}");
            tracing::warn!("Indexing will work but vectors won't be stored!");
        }
    }

    indexer
}

/// Create routes with a specific indexer service (useful for testing)
pub fn routes_with_indexer(indexer: IndexerServiceHandle) -> Router {
    Router::new()
        .route("/index", post(index_handler))
        .with_state(indexer)
}

/// Lazy-initialized indexer that creates storage connection on first use
struct LazyIndexer {
    indexer: Option<Indexer>,
}

impl LazyIndexer {
    const fn new() -> Self {
        Self { indexer: None }
    }

    #[allow(clippy::expect_used)] // Safe: we guarantee initialization above
    async fn get_or_init(&mut self) -> &mut Indexer {
        if self.indexer.is_none() {
            tracing::info!("Initializing indexer with storage on first use");
            self.indexer = Some(create_configured_indexer().await);
        }
        // Safe: we just ensured initialization above
        self.indexer.as_mut().expect("Indexer must be initialized")
    }
}

#[async_trait::async_trait]
impl IndexerService for LazyIndexer {
    async fn index_directory(
        &mut self,
        path: &std::path::Path,
        recursive: bool,
    ) -> codetriever_indexing::IndexerResult<codetriever_indexing::IndexResult> {
        let indexer = self.get_or_init().await;
        indexer.index_directory(path, recursive).await
    }

    async fn index_file_content(
        &mut self,
        project_id: &str,
        files: Vec<codetriever_indexing::indexing::service::FileContent>,
    ) -> codetriever_indexing::IndexerResult<codetriever_indexing::IndexResult> {
        let indexer = self.get_or_init().await;
        indexer.index_file_content(project_id, files).await
    }

    async fn drop_collection(&mut self) -> codetriever_indexing::IndexerResult<bool> {
        let indexer = self.get_or_init().await;
        indexer.drop_collection().await
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct IndexRequest {
    pub project_id: String,
    pub files: Vec<FileContent>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct FileContent {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IndexResponse {
    pub status: ResponseStatus,
    pub files_indexed: usize,
    pub chunks_created: usize,
}

impl_has_status!(IndexResponse);

impl IndexResponse {
    pub const fn success(files_indexed: usize, chunks_created: usize) -> Self {
        Self {
            status: ResponseStatus::Success,
            files_indexed,
            chunks_created,
        }
    }

    pub const fn error() -> Self {
        Self {
            status: ResponseStatus::Error,
            files_indexed: 0,
            chunks_created: 0,
        }
    }
}

/// Index code files for semantic search.
///
/// Accepts a list of files with their content to be parsed, chunked, and indexed
/// into the vector database for later semantic search.
#[utoipa::path(
    post,
    path = "/index",
    tag = "index",
    request_body = IndexRequest,
    responses(
        (status = 200, description = "Files indexed successfully", body = IndexResponse),
        (status = 500, description = "Internal server error", body = IndexResponse)
    )
)]
pub async fn index_handler(
    State(indexer): State<IndexerServiceHandle>,
    Json(request): Json<IndexRequest>,
) -> impl IntoResponse {
    use sha2::{Digest, Sha256};

    // Convert API FileContent to indexer FileContent, calculating hash from content
    let files = request
        .files
        .into_iter()
        .map(|f| {
            // Calculate SHA256 hash of content
            let mut hasher = Sha256::new();
            hasher.update(&f.content);
            let hash = format!("{:x}", hasher.finalize());

            codetriever_indexing::indexing::FileContent {
                path: f.path,
                content: f.content,
                hash,
            }
        })
        .collect();

    // Use the injected indexer service
    let mut indexer = indexer.lock().await;

    indexer
        .index_file_content(&request.project_id, files)
        .await
        .map_or_else(
            |_| Json(IndexResponse::error()),
            |result| {
                Json(IndexResponse::success(
                    result.files_indexed,
                    result.chunks_created,
                ))
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TestResult;
    use axum::body::Body;
    use axum::http::{StatusCode, header};
    use codetriever_indexing::test_mocks::MockIndexerService;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_index_endpoint_accepts_content() -> TestResult {
        // Use mock indexer that returns predictable results
        let mock_indexer = Arc::new(Mutex::new(MockIndexerService::new(2, 10)));
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{
            "project_id": "test-project",
            "files": [
                {
                    "path": "src/main.rs",
                    "content": "fn main() {\n    println!(\"Hello\");\n}"
                },
                {
                    "path": "src/lib.rs",
                    "content": "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}"
                }
            ]
        }"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        // Parse response body
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let response: IndexResponse = serde_json::from_slice(&body)?;

        assert_eq!(response.status, ResponseStatus::Success);
        assert_eq!(response.files_indexed, 2);
        assert_eq!(response.chunks_created, 10);
        Ok(())
    }

    #[tokio::test]
    async fn test_index_with_recursive_flag() -> TestResult {
        // Mock returns 5 files and 10 chunks
        let mock_indexer = Arc::new(Mutex::new(MockIndexerService::new(5, 10)));
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{
            "project_id": "test-project",
            "files": [
                {"path": "file1.rs", "content": "fn main() {}"},
                {"path": "file2.rs", "content": "fn test() {}"}
            ]
        }"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);
        Ok(())
    }

    #[tokio::test]
    async fn test_index_endpoint_validates_json() -> TestResult {
        let app = routes();

        let request_body = r#"{"invalid": "json_structure"}"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))?,
            )
            .await?;

        // Should get a client error for missing required field
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        Ok(())
    }

    #[tokio::test]
    async fn test_index_endpoint_handles_empty_files() -> TestResult {
        // Use mock indexer that returns predictable results
        let mock_indexer = Arc::new(Mutex::new(MockIndexerService::new(0, 0)));
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{
            "project_id": "test-project",
            "files": []
        }"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let response: IndexResponse = serde_json::from_slice(&body)?;

        // Empty files list should return 0 files
        assert_eq!(response.files_indexed, 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_index_endpoint_handles_no_content() -> TestResult {
        // Use mock indexer that returns predictable results
        let mock_indexer = Arc::new(Mutex::new(MockIndexerService::new(0, 0)));
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{
            "project_id": "test-project",
            "files": [
                {"path": "file.rs", "content": ""}
            ]
        }"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let response: IndexResponse = serde_json::from_slice(&body)?;

        // Empty content means no chunks, so file is not indexed
        assert_eq!(response.files_indexed, 0);
        assert_eq!(response.chunks_created, 0);
        Ok(())
    }

    #[tokio::test]
    async fn test_index_returns_file_count() -> TestResult {
        // Mock returns 3 files and 7 chunks
        let mock_indexer = Arc::new(Mutex::new(MockIndexerService::new(3, 7)));
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{
            "project_id": "test-project",
            "files": [
                {"path": "file1.rs", "content": "code1"},
                {"path": "file2.rs", "content": "code2"},
                {"path": "file3.rs", "content": "code3"}
            ]
        }"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let response: IndexResponse = serde_json::from_slice(&body)?;

        // Verify we got the mocked values
        assert_eq!(response.files_indexed, 3);
        assert_eq!(response.chunks_created, 7);
        Ok(())
    }

    #[tokio::test]
    async fn test_response_has_status_trait() -> TestResult {
        use crate::routes::response::HasStatus;

        let mut response = IndexResponse::success(5, 10);
        assert!(response.is_success());
        assert!(!response.is_error());

        response.set_status(ResponseStatus::Error);
        assert!(!response.is_success());
        assert!(response.is_error());

        response.set_status(ResponseStatus::PartialSuccess);
        assert!(response.is_success());
        assert!(!response.is_error());
        Ok(())
    }

    #[tokio::test]
    async fn test_index_creates_chunks() -> TestResult {
        // Mock returns 1 file and 5 chunks
        let mock_indexer = Arc::new(Mutex::new(MockIndexerService::new(1, 5)));
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{
            "project_id": "test-project",
            "files": [
                {"path": "file.py", "content": "def hello(): pass"}
            ]
        }"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))?,
            )
            .await?;

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let response: IndexResponse = serde_json::from_slice(&body)?;

        // Verify we got the mocked values
        assert_eq!(response.files_indexed, 1);
        assert_eq!(response.chunks_created, 5);
        Ok(())
    }

    #[tokio::test]
    async fn test_index_handles_indexer_errors() -> TestResult {
        // Mock that returns an error
        let mock_indexer = Arc::new(Mutex::new(MockIndexerService::with_error()));
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{
            "project_id": "test-project",
            "files": [
                {"path": "file.rs", "content": "code"}
            ]
        }"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let response: IndexResponse = serde_json::from_slice(&body)?;

        // Error case should return error status and 0 values
        assert_eq!(response.status, ResponseStatus::Error);
        assert_eq!(response.files_indexed, 0);
        assert_eq!(response.chunks_created, 0);
        Ok(())
    }
}
