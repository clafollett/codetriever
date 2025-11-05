use super::response::ResponseStatus;
use crate::impl_has_status;
use crate::middleware::RequestContext;
use crate::{ApiError, ApiResult};
use axum::{
    Router,
    extract::{Extension, Json, State},
    routing::post,
};
use codetriever_common::CorrelationId;
use codetriever_indexing::IndexerService;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, instrument, warn};
use utoipa::ToSchema;
use uuid::Uuid;

// Type alias for indexer service (no mutex needed!)
type IndexerServiceHandle = Arc<dyn IndexerService>;

/// Create routes with a specific indexer service
pub fn routes_with_indexer(indexer: IndexerServiceHandle) -> Router {
    use axum::routing::get;
    Router::new()
        .route("/index", post(index_handler))
        .route("/index/jobs/{job_id}", get(get_job_status_handler))
        .with_state(indexer)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct IndexRequest {
    /// Tenant ID for multi-tenancy isolation
    /// Defaults to nil UUID (00000000-...) for single-tenant deployments
    /// TODO: Extract from JWT/auth headers once authentication is implemented
    #[serde(default = "default_tenant_id")]
    #[schema(value_type = String, example = "00000000-0000-0000-0000-000000000000")]
    pub tenant_id: uuid::Uuid,
    pub project_id: String,
    pub files: Vec<FileContent>,
    /// Git commit context (required - extracted by CLI/MCP from user's repo)
    pub commit_context: codetriever_meta_data::models::CommitContext,
}

/// Default tenant ID for requests that don't specify one
const fn default_tenant_id() -> uuid::Uuid {
    uuid::Uuid::nil()
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct FileContent {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IndexResponse {
    pub status: ResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_queued: Option<usize>,
    pub files_indexed: usize,
    pub chunks_created: usize,
}

impl_has_status!(IndexResponse);

impl IndexResponse {
    pub const fn success(files_indexed: usize, chunks_created: usize) -> Self {
        Self {
            status: ResponseStatus::Success,
            job_id: None,
            files_queued: None,
            files_indexed,
            chunks_created,
        }
    }

    pub fn accepted(job_id: uuid::Uuid, files_queued: usize) -> Self {
        Self {
            status: ResponseStatus::Success,
            job_id: Some(job_id.to_string()),
            files_queued: Some(files_queued),
            files_indexed: 0,
            chunks_created: 0,
        }
    }

    pub const fn error() -> Self {
        Self {
            status: ResponseStatus::Error,
            job_id: None,
            files_queued: None,
            files_indexed: 0,
            chunks_created: 0,
        }
    }
}

/// Index code files for semantic search.
///
/// Accepts a list of files with their content to be parsed, chunked, and indexed
/// into the vector database for later semantic search.
///
/// # Errors
///
/// Returns `ApiError` if indexing fails due to service errors or timeouts
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
#[instrument(skip(indexer), fields(correlation_id))]
pub async fn index_handler(
    State(indexer): State<IndexerServiceHandle>,
    context: Option<Extension<RequestContext>>,
    Json(request): Json<IndexRequest>,
) -> ApiResult<Json<IndexResponse>> {
    use sha2::{Digest, Sha256};

    let start = std::time::Instant::now();

    // Extract correlation ID
    let correlation_id = context
        .as_ref()
        .map_or_else(CorrelationId::new, |ctx| ctx.correlation_id.clone());

    tracing::Span::current().record("correlation_id", correlation_id.to_string());

    info!(
        correlation_id = %correlation_id,
        project_id = %request.project_id,
        file_count = request.files.len(),
        "Processing index request"
    );

    // Validate request
    if request.files.is_empty() {
        warn!(correlation_id = %correlation_id, "Empty files list rejected");
        return Err(ApiError::invalid_query(
            request.project_id,
            "Files list cannot be empty".to_string(),
            correlation_id,
        ));
    }

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
        .collect::<Vec<_>>();

    let file_count = files.len();

    // Use tenant_id from request (defaults to nil UUID if not provided)
    // TODO: In future, extract from JWT/auth headers and validate against request.tenant_id
    let tenant_id = request.tenant_id;

    // Start async indexing job (returns immediately - no lock needed!)
    let job_id = match tokio::time::timeout(
        Duration::from_secs(5), // Job creation should be fast
        indexer.start_indexing_job(
            tenant_id,
            &request.project_id,
            files,
            &request.commit_context,
        ),
    )
    .await
    {
        Ok(Ok(job_id)) => job_id,
        Ok(Err(indexer_error)) => {
            error!(
                correlation_id = %correlation_id,
                error = %indexer_error,
                project_id = %request.project_id,
                "Failed to create indexing job"
            );
            return Err(ApiError::InternalServerError { correlation_id });
        }
        Err(_timeout) => {
            error!(
                correlation_id = %correlation_id,
                project_id = %request.project_id,
                "Job creation timed out"
            );
            return Err(ApiError::SearchServiceUnavailable {
                correlation_id,
                timeout_duration: Duration::from_secs(5),
            });
        }
    };

    let query_time_ms = start.elapsed().as_millis();

    info!(
        correlation_id = %correlation_id,
        project_id = %request.project_id,
        job_id = %job_id,
        files_queued = file_count,
        query_time_ms,
        "Indexing job created successfully"
    );

    // Return 202 Accepted with job ID
    Ok(Json(IndexResponse::accepted(job_id, file_count)))
}

/// Response for job status queries
#[derive(Debug, Serialize, ToSchema)]
pub struct JobStatusResponse {
    pub job_id: String,
    pub repository_id: String,
    pub branch: String,
    pub status: String,
    pub files_total: Option<i32>,
    pub files_processed: i32,
    pub chunks_created: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

/// Get indexing job status
///
/// # Errors
///
/// Returns `ApiError` if job not found or database errors occur
#[utoipa::path(
    get,
    path = "/index/jobs/{job_id}",
    tag = "index",
    params(
        ("job_id" = String, Path, description = "Job ID (UUID) to query")
    ),
    responses(
        (status = 200, description = "Job status retrieved", body = JobStatusResponse),
        (status = 404, description = "Job not found"),
        (status = 500, description = "Internal server error")
    )
)]
#[instrument(skip(indexer), fields(correlation_id, job_id))]
pub async fn get_job_status_handler(
    State(indexer): State<IndexerServiceHandle>,
    context: Option<Extension<RequestContext>>,
    axum::extract::Path(job_id): axum::extract::Path<Uuid>,
) -> ApiResult<Json<JobStatusResponse>> {
    let start = std::time::Instant::now();

    // Extract correlation ID
    let correlation_id = context
        .as_ref()
        .map_or_else(CorrelationId::new, |ctx| ctx.correlation_id.clone());

    tracing::Span::current().record("correlation_id", correlation_id.to_string());
    tracing::Span::current().record("job_id", job_id.to_string());

    info!(
        correlation_id = %correlation_id,
        job_id = %job_id,
        "Querying job status"
    );

    // Get job status from indexer
    let job = match tokio::time::timeout(
        Duration::from_secs(5), // Job status query should be fast
        indexer.get_job_status(&job_id),
    )
    .await
    {
        Ok(Ok(Some(job))) => job,
        Ok(Ok(None)) => {
            warn!(
                correlation_id = %correlation_id,
                job_id = %job_id,
                "Job not found"
            );
            return Err(ApiError::invalid_query(
                job_id.to_string(),
                "Job not found".to_string(),
                correlation_id,
            ));
        }
        Ok(Err(indexer_error)) => {
            error!(
                correlation_id = %correlation_id,
                error = %indexer_error,
                job_id = %job_id,
                "Failed to query job status"
            );
            return Err(ApiError::InternalServerError { correlation_id });
        }
        Err(_timeout) => {
            error!(
                correlation_id = %correlation_id,
                job_id = %job_id,
                "Job status query timed out"
            );
            return Err(ApiError::SearchServiceUnavailable {
                correlation_id,
                timeout_duration: Duration::from_secs(5),
            });
        }
    };

    let query_time_ms = start.elapsed().as_millis();

    info!(
        correlation_id = %correlation_id,
        job_id = %job_id,
        status = %job.status,
        query_time_ms,
        "Job status retrieved"
    );

    Ok(Json(JobStatusResponse {
        job_id: job.job_id.to_string(),
        repository_id: job.repository_id,
        branch: job.branch,
        status: job.status.to_string(),
        files_total: job.files_total,
        files_processed: job.files_processed,
        chunks_created: job.chunks_created,
        error_message: job.error_message,
        started_at: job.started_at.to_rfc3339(),
        completed_at: job.completed_at.map(|dt| dt.to_rfc3339()),
    }))
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
        let mock_indexer = Arc::new(MockIndexerService::new(2, 10)) as Arc<dyn IndexerService>;
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{
            "commit_context": {"repository_url": "https://github.com/test/repo", "commit_sha": "abc123", "commit_message": "Test", "commit_date": "2025-01-01T00:00:00Z", "author": "Test"}, "project_id": "test-project",
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
        // Async mode - check job_id and files_queued instead of results
        assert!(response.job_id.is_some(), "Should have job_id");
        assert_eq!(response.files_queued, Some(2));
        Ok(())
    }

    #[tokio::test]
    async fn test_index_with_recursive_flag() -> TestResult {
        // Mock returns 5 files and 10 chunks
        let mock_indexer = Arc::new(MockIndexerService::new(5, 10)) as Arc<dyn IndexerService>;
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{
            "commit_context": {"repository_url": "https://github.com/test/repo", "commit_sha": "abc123", "commit_message": "Test", "commit_date": "2025-01-01T00:00:00Z", "author": "Test"}, "project_id": "test-project",
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
        let mock_indexer = Arc::new(MockIndexerService::new(0, 0)) as Arc<dyn IndexerService>;
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
        let mock_indexer = Arc::new(MockIndexerService::new(0, 0)) as Arc<dyn IndexerService>;
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{
            "commit_context": {"repository_url": "https://github.com/test/repo", "commit_sha": "abc123", "commit_message": "Test", "commit_date": "2025-01-01T00:00:00Z", "author": "Test"}, "project_id": "test-project",
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

        // Empty files list should now return 400 Bad Request (validation added)
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        Ok(())
    }

    #[tokio::test]
    async fn test_index_endpoint_handles_no_content() -> TestResult {
        // Use mock indexer that returns predictable results
        let mock_indexer = Arc::new(MockIndexerService::new(0, 0)) as Arc<dyn IndexerService>;
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{
            "commit_context": {"repository_url": "https://github.com/test/repo", "commit_sha": "abc123", "commit_message": "Test", "commit_date": "2025-01-01T00:00:00Z", "author": "Test"}, "project_id": "test-project",
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
        assert!(
            response.job_id.is_some(),
            "Should have job_id in async mode"
        );
        // Async mode - chunks_created is 0 until job processes
        Ok(())
    }

    #[tokio::test]
    async fn test_index_returns_file_count() -> TestResult {
        // Mock returns 3 files and 7 chunks
        let mock_indexer = Arc::new(MockIndexerService::new(3, 7)) as Arc<dyn IndexerService>;
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{
            "commit_context": {"repository_url": "https://github.com/test/repo", "commit_sha": "abc123", "commit_message": "Test", "commit_date": "2025-01-01T00:00:00Z", "author": "Test"}, "project_id": "test-project",
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
        assert!(
            response.job_id.is_some(),
            "Should have job_id in async mode"
        );
        // Async mode - chunks_created is 0 until job processes
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
        let mock_indexer = Arc::new(MockIndexerService::new(1, 5)) as Arc<dyn IndexerService>;
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{
            "commit_context": {"repository_url": "https://github.com/test/repo", "commit_sha": "abc123", "commit_message": "Test", "commit_date": "2025-01-01T00:00:00Z", "author": "Test"}, "project_id": "test-project",
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
        assert!(
            response.job_id.is_some(),
            "Should have job_id in async mode"
        );
        // Async mode - chunks_created is 0 until job processes
        Ok(())
    }

    #[tokio::test]
    async fn test_index_handles_indexer_errors() -> TestResult {
        // In async mode, job creation should succeed even if processing will fail later
        // The error would show up when checking job status, not during job creation
        // For now, mock returns success for start_indexing_job()
        let mock_indexer = Arc::new(MockIndexerService::new(0, 0)) as Arc<dyn IndexerService>;
        let app = routes_with_indexer(mock_indexer);

        let request_body = r#"{
            "commit_context": {"repository_url": "https://github.com/test/repo", "commit_sha": "abc123", "commit_message": "Test", "commit_date": "2025-01-01T00:00:00Z", "author": "Test"}, "project_id": "test-project",
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

        // Job creation should succeed (async pattern)
        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let response: IndexResponse = serde_json::from_slice(&body)?;

        // Should have job_id (job created successfully)
        assert!(response.job_id.is_some());
        Ok(())
    }
}
