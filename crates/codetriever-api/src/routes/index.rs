use super::response::ResponseStatus;
use crate::impl_has_status;
use axum::{
    Router,
    extract::{Json, State},
    response::IntoResponse,
    routing::post,
};
use codetriever_indexer::{ApiIndexerService, IndexerService};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

// Type alias to simplify the complex indexer service type
type IndexerServiceHandle = Arc<Mutex<dyn IndexerService>>;

pub fn routes() -> Router {
    // Create the default indexer service
    let indexer_service = Arc::new(Mutex::new(ApiIndexerService::new()));
    routes_with_indexer(indexer_service)
}

/// Create routes with a specific indexer service (useful for testing)
pub fn routes_with_indexer(indexer: IndexerServiceHandle) -> Router {
    Router::new()
        .route("/index", post(index_handler))
        .with_state(indexer)
}

#[derive(Debug, Deserialize)]
pub struct IndexRequest {
    pub path: String,
    pub recursive: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexResponse {
    pub status: ResponseStatus,
    pub files_indexed: usize,
    pub chunks_created: usize,
}

impl_has_status!(IndexResponse);

impl IndexResponse {
    pub fn success(files_indexed: usize, chunks_created: usize) -> Self {
        Self {
            status: ResponseStatus::Success,
            files_indexed,
            chunks_created,
        }
    }

    pub fn error() -> Self {
        Self {
            status: ResponseStatus::Error,
            files_indexed: 0,
            chunks_created: 0,
        }
    }
}

async fn index_handler(
    State(indexer): State<IndexerServiceHandle>,
    Json(request): Json<IndexRequest>,
) -> impl IntoResponse {
    let path = std::path::Path::new(&request.path);
    let recursive = request.recursive.unwrap_or(false);

    // Use the injected indexer service
    let mut indexer = indexer.lock().await;

    match indexer.index_directory(path, recursive).await {
        Ok(result) => Json(IndexResponse::success(
            result.files_indexed,
            result.chunks_created,
        )),
        Err(_) => Json(IndexResponse::error()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{StatusCode, header};
    use codetriever_indexer::test_mocks::MockIndexerService;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_index_endpoint_accepts_path() {
        // Use mock indexer that returns predictable results
        let mock_indexer = Arc::new(Mutex::new(MockIndexerService::new(0, 0)));
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{"path": "/test/path"}"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Parse response body
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response: IndexResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(response.status, ResponseStatus::Success);
    }

    #[tokio::test]
    async fn test_index_with_recursive_flag() {
        // Mock returns 5 files and 10 chunks
        let mock_indexer = Arc::new(Mutex::new(MockIndexerService::new(5, 10)));
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{"path": "/test/path", "recursive": true}"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_index_endpoint_validates_json() {
        let app = routes();

        let request_body = r#"{"invalid": "json_structure"}"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should get a client error for missing required field
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_index_endpoint_handles_empty_path() {
        let app = routes();

        let request_body = r#"{"path": ""}"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response: IndexResponse = serde_json::from_slice(&body).unwrap();

        // Empty path should return 0 files
        assert_eq!(response.files_indexed, 0);
    }

    #[tokio::test]
    async fn test_index_endpoint_handles_nonexistent_path() {
        let app = routes();

        let request_body =
            r#"{"path": "/definitely/does/not/exist/path/to/nowhere", "recursive": false}"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response: IndexResponse = serde_json::from_slice(&body).unwrap();

        // Should handle gracefully with 0 files indexed
        assert_eq!(response.files_indexed, 0);
        assert_eq!(response.chunks_created, 0);
    }

    #[tokio::test]
    async fn test_index_returns_file_count() {
        // Mock returns 3 files and 7 chunks
        let mock_indexer = Arc::new(Mutex::new(MockIndexerService::new(3, 7)));
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{"path": "/any/path", "recursive": false}"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response: IndexResponse = serde_json::from_slice(&body).unwrap();

        // Verify we got the mocked values
        assert_eq!(response.files_indexed, 3);
        assert_eq!(response.chunks_created, 7);
    }

    #[tokio::test]
    async fn test_response_has_status_trait() {
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
    }

    #[tokio::test]
    async fn test_index_creates_chunks() {
        // Mock returns 1 file and 5 chunks
        let mock_indexer = Arc::new(Mutex::new(MockIndexerService::new(1, 5)));
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{"path": "/test/file.py", "recursive": false}"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response: IndexResponse = serde_json::from_slice(&body).unwrap();

        // Verify we got the mocked values
        assert_eq!(response.files_indexed, 1);
        assert_eq!(response.chunks_created, 5);
    }

    #[tokio::test]
    async fn test_index_handles_indexer_errors() {
        // Mock that returns an error
        let mock_indexer = Arc::new(Mutex::new(MockIndexerService::with_error()));
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{"path": "/some/path", "recursive": true}"#;

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/index")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(request_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response: IndexResponse = serde_json::from_slice(&body).unwrap();

        // Error case should return error status and 0 values
        assert_eq!(response.status, ResponseStatus::Error);
        assert_eq!(response.files_indexed, 0);
        assert_eq!(response.chunks_created, 0);
    }
}
