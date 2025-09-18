//! Search API routes and handlers for the Codetriever service.
//!
//! This module provides HTTP endpoints for searching through code repositories and files.
//! The search functionality is designed to help users find relevant code snippets, functions,
//! and files based on natural language queries or specific search terms.
//!
//! # API Overview
//!
//! The search API exposes the following endpoints:
//! - `POST /search` - Search for code using a query string with optional result limits
//!
//! # Example Usage
//!
//! ```json
//! POST /search
//! {
//!   "query": "authentication middleware function",
//!   "limit": 10
//! }
//! ```
//!
//! Response:
//! ```json
//! {
//!   "results": [
//!     {
//!       "file": "src/auth/middleware.rs",
//!       "score": 0.95
//!     }
//!   ]
//! }
//! ```

use axum::{Json, Router, extract::State, routing::post};
use codetriever_indexer::{ApiSearchService, SearchProvider};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

/// Request payload for code search operations.
///
/// This struct defines the parameters that can be sent to the search endpoint
/// to perform code searches across repositories and files.
///
/// # Fields
///
/// * `query` - The search query string. Can be natural language descriptions
///   (e.g., "authentication middleware") or specific code terms
/// * `limit` - Optional maximum number of results to return. If not specified,
///   the server will use a default limit to prevent excessive response sizes
///
/// # Examples
///
/// ```rust
/// use codetriever_api::routes::search::SearchRequest;
///
/// // Basic search
/// let request = SearchRequest {
///     query: "error handling".to_string(),
///     limit: None,
/// };
///
/// // Limited search
/// let request = SearchRequest {
///     query: "database connection pool".to_string(),
///     limit: Some(5),
/// };
/// ```
#[derive(Debug, Deserialize, ToSchema)]
pub struct SearchRequest {
    /// The search query string - can be natural language or specific code terms
    pub query: String,
    /// Optional limit on the number of search results returned
    pub limit: Option<usize>,
}

/// Response with matches and metadata
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResponse {
    /// Matched code snippets
    pub matches: Vec<Match>,
    /// Search metadata
    pub metadata: SearchMetadata,
}

/// Search metadata
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchMetadata {
    /// Total number of matches found
    pub total_matches: usize,
    /// Number of matches returned
    pub returned: usize,
    /// The original query string
    pub query: String,
    /// Query execution time in milliseconds
    pub query_time_ms: u64,
    /// Index version (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_version: Option<String>,
    /// Type of search performed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_type: Option<String>,
}

/// A single search match
#[derive(Debug, Serialize, ToSchema)]
pub struct Match {
    /// File name
    pub file: String,
    /// Full file path
    pub path: String,
    /// Repository name (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// Code content
    pub content: String,
    /// Programming language
    pub language: String,
    /// Type of code element (function, class, etc.)
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub element_type: Option<String>,
    /// Name of the symbol (function/class name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Line range
    pub lines: LineRange,
    /// Similarity score
    pub similarity: f32,
    /// Surrounding context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Context>,
    /// Highlight ranges for match display
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub highlights: Vec<Range>,
    /// Related symbols in the chunk
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<String>,
    /// Git commit information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<CommitInfo>,
}

/// Line range in a file
#[derive(Debug, Serialize, ToSchema)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

/// Context around a match
#[derive(Debug, Serialize, ToSchema)]
pub struct Context {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// Highlight range
#[derive(Debug, Serialize, ToSchema)]
pub struct Range {
    pub start: usize,
    pub end: usize,
}

/// Git commit information
#[derive(Debug, Serialize, ToSchema)]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub date: String,
}

/// Creates and configures the search API routes.
///
/// This function sets up all HTTP routes related to search functionality and returns
/// an Axum [`Router`] that can be mounted into the main application router.
///
/// # Routes
///
/// - `POST /search` - Handles search requests using [`search_handler`]
///
/// # Returns
///
/// Returns an Axum [`Router`] with all search-related routes configured.
/// The router handles JSON request/response serialization automatically.
///
/// # Examples
///
/// ```rust
/// use axum::Router;
/// use codetriever_api::routes::search;
///
/// // Mount search routes into main app
/// let app = Router::new()
///     .nest("/api/v1", search::routes());
/// ```
///
/// # Usage with Main Router
///
/// Typically, this router is nested under a versioned API path:
///
/// ```text
/// POST /api/v1/search
/// ```
/// Type for search service handle
type SearchServiceHandle = Arc<dyn SearchProvider>;

pub fn routes() -> Router {
    // Create search service with database integration for repository metadata
    // TODO: Wire up actual database client from configuration
    let search_service = Arc::new(ApiSearchService::new()) as Arc<dyn SearchProvider>;

    Router::new()
        .route("/search", post(search_handler))
        .with_state(search_service)
}

/// Create routes with specific search service (primarily for testing)
pub fn routes_with_service(search_service: Arc<dyn SearchProvider>) -> Router {
    Router::new()
        .route("/search", post(search_handler))
        .with_state(search_service)
}

/// Search for code in the indexed repository.
///
/// Performs semantic search using embeddings to find the most relevant code chunks
/// matching the query. Returns matches with similarity scores and metadata.
#[utoipa::path(
    post,
    path = "/search",
    tag = "search",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Search results", body = SearchResponse),
        (status = 500, description = "Internal server error")
    )
)]
/// HTTP handler for search requests.
///
/// This asynchronous function processes search requests by accepting a JSON payload
/// containing search parameters and returning matching results.
///
/// # Parameters
///
/// * `req` - JSON-deserialized [`SearchRequest`] containing the search query and options
///
/// # Returns
///
/// Returns a JSON response containing search results in the [`SearchResponse`] format
/// with matches array and metadata.
///
/// # Error Handling
///
/// Currently returns empty results on error, but could be enhanced to handle:
/// - Invalid query parameters
/// - Database/index unavailability
/// - Internal search engine errors
pub async fn search_handler(
    State(search_service): State<SearchServiceHandle>,
    Json(req): Json<SearchRequest>,
) -> Json<SearchResponse> {
    let start = std::time::Instant::now();

    // Use provided limit or default to 10
    let limit = req.limit.unwrap_or(10);
    let query = req.query.clone();

    // Perform search
    let results = search_service
        .search(&query, limit)
        .await
        .unwrap_or_else(|_| vec![]);

    let total_matches = results.len();

    // Convert to new Match format with repository metadata
    let matches: Vec<Match> = results
        .into_iter()
        .map(|result| {
            let file_path = result.chunk.file_path.clone();

            // Extract repository and commit info from metadata if available
            let (repository, commit) =
                result
                    .repository_metadata
                    .as_ref()
                    .map_or((None, None), |metadata| {
                        let repository_name = metadata
                            .repository_url
                            .as_ref()
                            .and_then(|url| url.split('/').next_back())
                            .map(std::string::ToString::to_string);

                        let commit_info =
                            if let (Some(sha), Some(message), Some(author), Some(date)) = (
                                &metadata.commit_sha,
                                &metadata.commit_message,
                                &metadata.author,
                                &metadata.commit_date,
                            ) {
                                Some(CommitInfo {
                                    sha: sha.clone(),
                                    message: message.clone(),
                                    author: author.clone(),
                                    date: date.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                                })
                            } else {
                                None
                            };

                        (repository_name, commit_info)
                    });

            Match {
                file: file_path.clone(),
                path: file_path,
                repository,
                content: result.chunk.content,
                language: result.chunk.language,
                element_type: result.chunk.kind,
                name: result.chunk.name,
                lines: LineRange {
                    start: result.chunk.start_line,
                    end: result.chunk.end_line,
                },
                similarity: result.similarity,
                context: None,      // TODO: Fetch surrounding lines
                highlights: vec![], // TODO: Implement highlighting
                symbols: vec![],    // TODO: Extract symbols from chunk
                commit,
            }
        })
        .collect();

    let query_time_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let returned = matches.len();

    Json(SearchResponse {
        matches,
        metadata: SearchMetadata {
            total_matches,
            returned,
            query,
            query_time_ms,
            index_version: None,
            search_type: Some("semantic".to_string()),
        },
    })
}

/// Handler with injected service for testing
#[cfg(test)]
async fn search_handler_with_service(
    req: SearchRequest,
    search_service: Arc<tokio::sync::Mutex<codetriever_indexer::test_mocks::MockSearchService>>,
) -> Json<SearchResponse> {
    let limit = req.limit.unwrap_or(10);
    let query = req.query.clone();

    let results = search_service
        .lock()
        .await
        .search(&query, limit)
        .await
        .unwrap_or_else(|_| vec![]);

    let total_matches = results.len();

    let matches: Vec<Match> = results
        .into_iter()
        .map(|result| {
            let file_path = result.chunk.file_path.clone();
            Match {
                file: file_path.clone(),
                path: file_path,
                repository: None,
                content: result.chunk.content,
                language: result.chunk.language,
                element_type: result.chunk.kind,
                name: result.chunk.name,
                lines: LineRange {
                    start: result.chunk.start_line,
                    end: result.chunk.end_line,
                },
                similarity: result.similarity,
                context: None,
                highlights: vec![],
                symbols: vec![],
                commit: None,
            }
        })
        .collect();

    let returned = matches.len();

    Json(SearchResponse {
        matches,
        metadata: SearchMetadata {
            total_matches,
            returned,
            query,
            query_time_ms: 0,
            index_version: None,
            search_type: Some("semantic".to_string()),
        },
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)] // OK in tests
    #![allow(clippy::unwrap_used)] // OK in tests
    use super::*;
    use crate::test_utils::TestResult;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use codetriever_indexer::test_mocks::MockSearchService;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tower::ServiceExt;

    /// Create routes with a mock search service for testing
    fn routes_with_mock(search_service: Arc<Mutex<MockSearchService>>) -> Router {
        Router::new().route(
            "/search",
            post({
                let service = search_service;
                move |Json(req): Json<SearchRequest>| {
                    let service = Arc::clone(&service);
                    async move { search_handler_with_service(req, service).await }
                }
            }),
        )
    }

    #[tokio::test]
    async fn test_search_returns_matches_with_metadata() -> TestResult {
        // Test that the new response format includes matches array and metadata
        let mock_service = Arc::new(Mutex::new(MockSearchService::with_results(vec![
            (
                "src/auth.rs".to_string(),
                "fn authenticate() {}".to_string(),
                0.95,
            ),
            (
                "src/middleware.rs".to_string(),
                "check_auth()".to_string(),
                0.87,
            ),
        ])));

        let app = routes_with_mock(mock_service);

        let request_body = json!({
            "query": "authentication middleware",
            "limit": 10
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/search")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request_body)?))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let json: serde_json::Value = serde_json::from_slice(&body)?;

        // Verify new response structure
        assert!(
            json.get("matches").is_some(),
            "Response must have 'matches' field"
        );
        assert!(
            json.get("metadata").is_some(),
            "Response must have 'metadata' field"
        );

        // Check metadata structure
        let metadata = json.get("metadata").expect("metadata field exists");
        assert!(metadata.get("total_matches").is_some());
        assert!(metadata.get("returned").is_some());
        assert!(metadata.get("query").is_some());
        assert!(metadata.get("query_time_ms").is_some());
        assert_eq!(
            metadata.get("query"),
            Some(&json!("authentication middleware"))
        );
        assert_eq!(metadata.get("returned"), Some(&json!(2)));

        // Check matches structure
        let matches = json
            .get("matches")
            .and_then(|v| v.as_array())
            .expect("matches is an array");
        assert_eq!(matches.len(), 2);

        // Verify first match has all required fields
        let first_match = matches.first().expect("at least one match");
        assert_eq!(first_match.get("file"), Some(&json!("src/auth.rs")));
        assert_eq!(first_match.get("path"), Some(&json!("src/auth.rs")));
        assert_eq!(
            first_match.get("content"),
            Some(&json!("fn authenticate() {}"))
        );
        assert_eq!(first_match.get("similarity"), Some(&json!(0.95)));
        assert!(first_match.get("language").is_some());
        assert!(first_match.get("lines").is_some());
        let lines = first_match.get("lines").expect("lines exists");
        assert_eq!(lines.get("start"), Some(&json!(1)));
        assert_eq!(lines.get("end"), Some(&json!(10)));

        Ok(())
    }

    #[tokio::test]
    async fn test_search_backward_compat_format() -> TestResult {
        // Create mock search service with test data
        let mock_service = Arc::new(Mutex::new(MockSearchService::with_results(vec![
            (
                "src/auth.rs".to_string(),
                "fn authenticate() {}".to_string(),
                0.95,
            ),
            (
                "src/middleware.rs".to_string(),
                "check_auth()".to_string(),
                0.87,
            ),
        ])));

        let app = routes_with_mock(mock_service);

        let request_body = json!({
            "query": "authentication",
            "limit": 10
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/search")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request_body)?))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        // Parse response and verify structure matches OpenAPI spec
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let json: serde_json::Value = serde_json::from_slice(&body)?;

        // Verify new response structure - should have matches and metadata
        assert!(
            json.get("matches").is_some(),
            "Response must have 'matches' field"
        );
        assert!(
            json.get("metadata").is_some(),
            "Response must have 'metadata' field"
        );

        let matches = json
            .get("matches")
            .and_then(|v| v.as_array())
            .expect("matches is an array");
        assert_eq!(matches.len(), 2);

        // Verify first match structure
        let first_match = matches.first().expect("at least one match");
        assert_eq!(first_match.get("file"), Some(&json!("src/auth.rs")));
        assert_eq!(
            first_match.get("content"),
            Some(&json!("fn authenticate() {}"))
        );
        assert_eq!(first_match.get("similarity"), Some(&json!(0.95)));

        Ok(())
    }

    #[tokio::test]
    async fn test_search_respects_limit() -> TestResult {
        // Create mock with 5 results
        let mock_service = Arc::new(Mutex::new(MockSearchService::with_results(vec![
            ("file1.rs".to_string(), "content1".to_string(), 0.9),
            ("file2.rs".to_string(), "content2".to_string(), 0.8),
            ("file3.rs".to_string(), "content3".to_string(), 0.7),
            ("file4.rs".to_string(), "content4".to_string(), 0.6),
            ("file5.rs".to_string(), "content5".to_string(), 0.5),
        ])));

        let app = routes_with_mock(mock_service);

        let request_body = json!({
            "query": "test query",
            "limit": 2  // Request only 2 results
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/search")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request_body)?))?,
            )
            .await?;

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let json: serde_json::Value = serde_json::from_slice(&body)?;

        let matches = json
            .get("matches")
            .and_then(|v| v.as_array())
            .expect("matches is an array");
        assert_eq!(
            matches.len(),
            2,
            "Should return only 2 results as per limit"
        );

        // Also verify metadata
        let metadata = json.get("metadata").expect("metadata exists");
        assert_eq!(metadata.get("returned"), Some(&json!(2)));

        Ok(())
    }

    #[tokio::test]
    async fn test_search_handles_empty_results() -> TestResult {
        // Create mock with no results
        let mock_service = Arc::new(Mutex::new(MockSearchService::with_results(vec![])));

        let app = routes_with_mock(mock_service);

        let request_body = json!({
            "query": "nonexistent code",
            "limit": 10
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/search")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request_body)?))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let json: serde_json::Value = serde_json::from_slice(&body)?;

        let matches = json
            .get("matches")
            .and_then(|v| v.as_array())
            .expect("matches is an array");
        assert_eq!(
            matches.len(),
            0,
            "Should return empty array when no results"
        );

        // Verify metadata for empty results
        let metadata = json.get("metadata").expect("metadata exists");
        assert_eq!(metadata.get("total_matches"), Some(&json!(0)));
        assert_eq!(metadata.get("returned"), Some(&json!(0)));

        Ok(())
    }

    #[tokio::test]
    async fn test_search_response_includes_repository_fields() -> TestResult {
        // Test that the search response structure includes repository and commit fields
        // Even if they're None, the fields should be present in the response structure

        let mock_service = Arc::new(Mutex::new(MockSearchService::with_results(vec![(
            "src/auth.rs".to_string(),
            "fn authenticate() {}".to_string(),
            0.95,
        )])));

        let app = routes_with_mock(mock_service);

        let request_body = json!({
            "query": "authentication",
            "limit": 10
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/search")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request_body)?))?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
        let json: serde_json::Value = serde_json::from_slice(&body)?;

        let matches = json
            .get("matches")
            .and_then(|v| v.as_array())
            .expect("matches is an array");
        assert_eq!(matches.len(), 1);

        // Verify that repository and commit fields exist in response structure
        let first_match = matches.first().expect("at least one match");

        // The repository and commit fields use skip_serializing_if = "Option::is_none"
        // So they won't be present in the JSON when None (which is correct behavior)

        // When no repository metadata is available, these fields should not be present
        assert!(first_match.get("repository").is_none());
        assert!(first_match.get("commit").is_none());

        // Verify the basic match structure is correct
        assert_eq!(first_match.get("file"), Some(&json!("src/auth.rs")));
        assert_eq!(
            first_match.get("content"),
            Some(&json!("fn authenticate() {}"))
        );
        assert_eq!(first_match.get("similarity"), Some(&json!(0.95)));

        Ok(())
    }
}
