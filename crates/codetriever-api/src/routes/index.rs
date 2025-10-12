use super::response::ResponseStatus;
use crate::impl_has_status;
use axum::{
    Router,
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
};
use codetriever_indexing::IndexerService;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use utoipa::ToSchema;

// Type alias to simplify the complex indexer service type
type IndexerServiceHandle = Arc<Mutex<dyn IndexerService>>;

/// Create routes with a specific indexer service
pub fn routes_with_indexer(indexer: IndexerServiceHandle) -> Router {
    Router::new()
        .route("/index", post(index_handler))
        .with_state(indexer)
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

    match indexer.index_file_content(&request.project_id, files).await {
        Ok(result) => {
            // Success - return 200 with results
            (
                StatusCode::OK,
                Json(IndexResponse::success(
                    result.files_indexed,
                    result.chunks_created,
                )),
            )
                .into_response()
        }
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!(
                "Indexing failed for project {}: {error_msg}",
                request.project_id
            );

            // Determine if this is an infrastructure error (500) or business logic error (200)
            let is_infrastructure_error = error_msg.contains("Pool closed")
                || error_msg.contains("database")
                || error_msg.contains("connection")
                || error_msg.contains("timeout")
                || error_msg.contains("Embedding generation failed");

            if is_infrastructure_error {
                // Infrastructure failure → 500
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(IndexResponse::error()),
                )
                    .into_response()
            } else {
                // Business logic error (file unchanged, validation, etc.) → 200
                (StatusCode::OK, Json(IndexResponse::error())).into_response()
            }
        }
    }
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
        // Use mock indexer (test validates JSON schema, not indexing logic)
        let mock_indexer = Arc::new(Mutex::new(MockIndexerService::new(0, 0)));
        let app = routes_with_indexer(mock_indexer);

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
