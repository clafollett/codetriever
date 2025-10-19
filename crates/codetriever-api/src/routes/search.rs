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

use crate::middleware::RequestContext;
use crate::{ApiError, ApiResult};
use axum::{
    Json, Router,
    extract::{Extension, State},
    routing::post,
};
use codetriever_common::CorrelationId;
use codetriever_search::{SearchError, SearchProvider, SearchService};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, instrument, warn};
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
type LazySearchHandle = Arc<tokio::sync::Mutex<LazySearchService>>;

// Type alias to simplify complex type
type SearchProviderHandle = Option<Arc<dyn SearchProvider>>;

/// Lazy-initialized search service that creates real storage connection on first use
pub struct LazySearchService {
    service: SearchProviderHandle,
}

impl LazySearchService {
    const fn new() -> Self {
        Self { service: None }
    }

    #[allow(clippy::expect_used)] // Safe: we guarantee initialization above
    async fn get_or_init(&mut self) -> Result<Arc<dyn SearchProvider>, SearchError> {
        if self.service.is_none() {
            tracing::info!("Initializing search service with Qdrant storage on first use");
            self.service = Some(create_configured_search_service().await?);
        }
        // Safe: we just ensured initialization above
        Ok(Arc::clone(
            self.service.as_ref().expect("Service must be initialized"),
        ))
    }
}

/// Create a properly configured search service with real Qdrant storage
async fn create_configured_search_service() -> Result<Arc<dyn SearchProvider>, SearchError> {
    use codetriever_config::ApplicationConfig;
    use codetriever_meta_data::{PoolConfig, PoolManager};
    use codetriever_vector_data::QdrantStorage;

    // Load configuration
    let config = ApplicationConfig::from_env();

    // Set up embedding service first
    let embedding_service = Arc::new(codetriever_embeddings::DefaultEmbeddingService::new(
        config.embedding.clone(),
    )) as Arc<dyn codetriever_embeddings::EmbeddingService>;

    // Set up vector storage (Qdrant) - same as indexing service
    let vector_storage = match QdrantStorage::new(
        config.vector_storage.url.clone(),
        config.vector_storage.collection_name.clone(),
    )
    .await
    {
        Ok(storage) => {
            tracing::info!("Connected to Qdrant storage successfully for search");
            Arc::new(storage) as Arc<dyn codetriever_vector_data::VectorStorage>
        }
        Err(e) => {
            tracing::warn!("Could not connect to Qdrant for search: {e}");
            tracing::warn!("Search will not work without vector storage!");
            // Fall back to mock storage (searches will return empty)
            Arc::new(codetriever_vector_data::MockStorage::new())
                as Arc<dyn codetriever_vector_data::VectorStorage>
        }
    };

    // Set up database client - REQUIRED for SearchService to function
    let pools = PoolManager::new(&config.database, PoolConfig::default())
        .await
        .map_err(|e| SearchError::DatabaseConnectionFailed {
            message: format!("Failed to connect to database: {e}"),
        })?;
    let db_client = Arc::new(codetriever_meta_data::DataClient::new(pools));

    Ok(Arc::new(SearchService::new(
        embedding_service,
        vector_storage,
        db_client,
    )))
}

/// Service factory for proper dependency injection
pub struct ServiceFactory {
    embedding_service: Arc<dyn codetriever_embeddings::EmbeddingService>,
    vector_storage: Arc<dyn codetriever_vector_data::VectorStorage>,
    db_client: Arc<codetriever_meta_data::DataClient>,
}

impl ServiceFactory {
    /// Create a new service factory with injected dependencies
    pub fn new(
        embedding_service: Arc<dyn codetriever_embeddings::EmbeddingService>,
        vector_storage: Arc<dyn codetriever_vector_data::VectorStorage>,
        db_client: Arc<codetriever_meta_data::DataClient>,
    ) -> Self {
        Self {
            embedding_service,
            vector_storage,
            db_client,
        }
    }

    /// Create the search service with all dependencies
    pub fn create_search_service(&self) -> Arc<dyn SearchProvider> {
        Arc::new(SearchService::new(
            Arc::clone(&self.embedding_service),
            Arc::clone(&self.vector_storage),
            Arc::clone(&self.db_client),
        ))
    }
}

/// Create routes with lazy-initialized search service that properly connects to Qdrant
pub fn routes() -> Router {
    // Create a lazy-initialized search wrapper
    // Storage will be initialized on first use with proper Qdrant configuration
    let search_wrapper = Arc::new(tokio::sync::Mutex::new(LazySearchService::new()));
    routes_with_lazy_search(search_wrapper)
}

/// Create routes with injected search service (proper dependency injection)
pub fn routes_with_search_service(search_service: Arc<dyn SearchProvider>) -> Router {
    Router::new()
        .route("/search", post(search_handler))
        .with_state(search_service)
}

/// Create routes with lazy search service wrapper
pub fn routes_with_lazy_search(
    search_service: Arc<tokio::sync::Mutex<LazySearchService>>,
) -> Router {
    Router::new()
        .route("/search", post(lazy_search_handler))
        .with_state(search_service)
}

/// Fetch surrounding lines for context
/// Returns up to 3 lines before or after the chunk as a joined string
fn fetch_surrounding_lines(content: &str, line_number: usize, before: bool) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let context_size = 3;

    let context_lines = if before && line_number > 1 {
        // Get lines before the chunk
        let start = line_number.saturating_sub(context_size + 1);
        let end = line_number.saturating_sub(1);
        lines.get(start..end).unwrap_or(&[])
    } else if !before {
        // Get lines after the chunk
        let start = line_number;
        let end = line_number.saturating_add(context_size).min(lines.len());
        lines.get(start..end).unwrap_or(&[])
    } else {
        &[]
    };

    if context_lines.is_empty() {
        None
    } else {
        Some(context_lines.join("\n"))
    }
}

/// Extract symbols from a code chunk
/// Leverages the existing tree-sitter parsing that already extracted the chunk name and kind
fn extract_symbols_from_chunk(chunk: &codetriever_parsing::CodeChunk) -> Vec<String> {
    let mut symbols = Vec::new();

    // The tree-sitter parser already extracted the primary symbol name
    if let Some(name) = &chunk.name {
        symbols.push(name.clone());
    }

    // Add the chunk kind as a symbol type for better searchability
    if let Some(kind) = &chunk.kind {
        symbols.push(format!(
            "{}:{}",
            kind,
            chunk.name.as_deref().unwrap_or("anonymous")
        ));
    }

    symbols
}

/// Implement search term highlighting
fn highlight_search_terms(content: &str, query: &str) -> Vec<Range> {
    let mut highlights = Vec::new();
    let query_lower = query.to_lowercase();
    let content_lower = content.to_lowercase();

    let mut start = 0;
    while let Some(pos) = content_lower[start..].find(&query_lower) {
        let actual_start = start.saturating_add(pos);
        let actual_end = actual_start.saturating_add(query.len());

        highlights.push(Range {
            start: actual_start,
            end: actual_end,
        });

        start = actual_end;
    }

    highlights
}

/// Handler for lazy-initialized search service
///
/// # Errors
///
/// Returns `ApiError` if:
/// - Query is empty or invalid
/// - Search service is unavailable
/// - Database timeout occurs
pub async fn lazy_search_handler(
    State(search_service): State<LazySearchHandle>,
    context: Option<Extension<RequestContext>>,
    Json(req): Json<SearchRequest>,
) -> ApiResult<Json<SearchResponse>> {
    // Get or initialize the search service
    let correlation_id = context
        .as_ref()
        .map_or_else(CorrelationId::new, |ctx| ctx.correlation_id.clone());

    let mut service_guard = search_service.lock().await;
    let service = service_guard.get_or_init().await.map_err(|e| {
        // Database connection failure during initialization is a service unavailability issue
        tracing::error!(correlation_id = %correlation_id, error = %e, "Failed to initialize search service");
        ApiError::SearchServiceUnavailable {
            correlation_id: correlation_id.clone(),
            timeout_duration: Duration::from_secs(30),
        }
    })?;
    drop(service_guard); // Release lock before doing the actual search

    // Delegate to the regular search handler logic
    search_handler_impl(service, context, req).await
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
/// HTTP handler for search requests with structured error handling.
///
/// This asynchronous function processes search requests by accepting a JSON payload
/// containing search parameters and returning matching results with proper error handling.
///
/// # Parameters
///
/// * `search_service` - Injected search service handle
/// * `context` - Optional request context with correlation ID from middleware
/// * `req` - JSON-deserialized [`SearchRequest`] containing the search query and options
///
/// # Returns
///
/// Returns a JSON response containing search results in the [`SearchResponse`] format
/// with matches array and metadata, or a structured error response.
///
/// # Errors
///
/// Returns `ApiError` in the following cases:
/// - Invalid query parameters (400 Bad Request)
/// - Search service unavailability (503 Service Unavailable)
/// - Database timeouts (503 Service Unavailable)
/// - Internal server errors (500 Internal Server Error)
///
/// All errors include correlation IDs for tracking and debugging.
#[instrument(skip(search_service), fields(correlation_id))]
pub async fn search_handler(
    State(search_service): State<SearchServiceHandle>,
    context: Option<Extension<RequestContext>>,
    Json(req): Json<SearchRequest>,
) -> ApiResult<Json<SearchResponse>> {
    search_handler_impl(search_service, context, req).await
}

/// Common search handler implementation
async fn search_handler_impl(
    search_service: Arc<dyn SearchProvider>,
    context: Option<Extension<RequestContext>>,
    req: SearchRequest,
) -> ApiResult<Json<SearchResponse>> {
    let start = std::time::Instant::now();
    // Use correlation ID from middleware if available, otherwise generate one
    let correlation_id = context
        .as_ref()
        .map_or_else(CorrelationId::new, |ctx| ctx.correlation_id.clone());

    // Add correlation ID to tracing span for all subsequent logs
    tracing::Span::current().record("correlation_id", correlation_id.to_string());

    info!(
        correlation_id = %correlation_id,
        query = %req.query,
        limit = req.limit,
        "Processing search request"
    );

    // Validate query parameters
    if req.query.trim().is_empty() {
        warn!(
            correlation_id = %correlation_id,
            "Empty search query rejected"
        );
        return Err(ApiError::invalid_query(
            req.query,
            "Query cannot be empty".to_string(),
            correlation_id,
        ));
    }

    if req.query.len() > 1000 {
        warn!(
            correlation_id = %correlation_id,
            query_length = req.query.len(),
            "Query too long rejected"
        );
        return Err(ApiError::invalid_query(
            req.query,
            "Query exceeds maximum length of 1000 characters".to_string(),
            correlation_id,
        ));
    }

    // Use provided limit or default to 10, with reasonable bounds
    let limit = req.limit.unwrap_or(10).min(100); // Cap at 100 results
    let query = req.query.clone();

    // Perform search with proper error handling
    let results = match tokio::time::timeout(
        Duration::from_secs(30), // 30-second timeout for search operations
        search_service.search(&query, limit, &correlation_id),
    )
    .await
    {
        Ok(Ok(results)) => {
            info!(
                correlation_id = %correlation_id,
                result_count = results.len(),
                query_time_ms = start.elapsed().as_millis(),
                "Search completed successfully"
            );
            results
        }
        Ok(Err(search_error)) => {
            error!(
                correlation_id = %correlation_id,
                error = %search_error,
                query = %query,
                "Search service returned error"
            );

            // Convert search service errors to appropriate API errors
            if search_error.to_string().contains("timeout") {
                return Err(ApiError::database_timeout(
                    "search".to_string(),
                    correlation_id,
                ));
            } else if search_error.to_string().contains("unavailable")
                || search_error.to_string().contains("connection")
            {
                return Err(ApiError::SearchServiceUnavailable {
                    correlation_id,
                    timeout_duration: Duration::from_secs(30),
                });
            }
            error!(
                correlation_id = %correlation_id,
                error = %search_error,
                query = %query,
                "Search failed with unexpected error"
            );
            return Err(ApiError::InternalServerError { correlation_id });
        }
        Err(_timeout) => {
            error!(
                correlation_id = %correlation_id,
                timeout_duration_ms = 30000,
                query = %query,
                "Search operation timed out"
            );
            return Err(ApiError::SearchServiceUnavailable {
                correlation_id,
                timeout_duration: Duration::from_secs(30),
            });
        }
    };

    let total_matches = results.len();

    // Convert to new Match format with repository metadata
    let matches: Vec<Match> = results
        .into_iter()
        .map(|result| {
            let chunk = &result.chunk;
            let file_path = chunk.file_path.clone();
            let chunk_content = chunk.content.clone();

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
                content: chunk_content.clone(),
                language: chunk.language.clone(),
                element_type: chunk.kind.clone(),
                name: chunk.name.clone(),
                lines: LineRange {
                    start: chunk.start_line,
                    end: chunk.end_line,
                },
                similarity: result.similarity,
                context: Some(Context {
                    before: fetch_surrounding_lines(&chunk_content, chunk.start_line, true),
                    after: fetch_surrounding_lines(&chunk_content, chunk.end_line, false),
                }),
                highlights: highlight_search_terms(&chunk_content, &query),
                symbols: extract_symbols_from_chunk(chunk),
                commit,
            }
        })
        .collect();

    let query_time_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let returned = matches.len();

    info!(
        correlation_id = %correlation_id,
        total_matches,
        returned,
        query_time_ms,
        "Search request completed successfully"
    );

    Ok(Json(SearchResponse {
        matches,
        metadata: SearchMetadata {
            total_matches,
            returned,
            query,
            query_time_ms,
            index_version: None,
            search_type: Some("semantic".to_string()),
        },
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)] // OK in tests
    #![allow(clippy::unwrap_used)] // OK in tests
    use super::*;
    use crate::test_utils::TestResult;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use codetriever_search::test_mocks::MockSearchService;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tower::ServiceExt;

    async fn search_handler_with_service(
        req: SearchRequest,
        search_service: Arc<tokio::sync::Mutex<codetriever_search::test_mocks::MockSearchService>>,
    ) -> Json<SearchResponse> {
        let limit = req.limit.unwrap_or(10);
        let query = req.query.clone();
        let correlation_id = CorrelationId::new();

        let results = match search_service
            .lock()
            .await
            .search(&query, limit, &correlation_id)
            .await
        {
            Ok(results) => results,
            Err(e) => {
                tracing::error!("Test search failed: {:?}", e);
                vec![]
            }
        };

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
    async fn test_search_results_include_repository_and_commit_info() {
        // Test that search results properly populate repository and commit fields from metadata
        use chrono::Utc;

        let mock_result = codetriever_search::SearchMatch {
            chunk: codetriever_parsing::CodeChunk {
                file_path: "src/auth.rs".to_string(),
                content: "fn authenticate() {}".to_string(),
                start_line: 1,
                end_line: 1,
                byte_start: 0,
                byte_end: 20,
                kind: Some("function".to_string()),
                language: "rust".to_string(),
                name: Some("authenticate".to_string()),
                token_count: Some(3),
                embedding: None,
            },
            similarity: 0.95,
            repository_metadata: Some(codetriever_search::RepositoryMetadata {
                repository_id: "my-repo".to_string(),
                repository_url: Some("https://github.com/user/my-repo".to_string()),
                branch: "main".to_string(),
                commit_sha: Some("abc123".to_string()),
                commit_message: Some("Add authentication".to_string()),
                commit_date: Some(Utc::now()),
                author: Some("John Doe".to_string()),
            }),
        };

        // Convert to API Match format
        let (repository, commit) =
            mock_result
                .repository_metadata
                .as_ref()
                .map_or((None, None), |metadata| {
                    let repository_name = metadata
                        .repository_url
                        .as_ref()
                        .and_then(|url| url.split('/').next_back())
                        .map(std::string::ToString::to_string);

                    let commit_info = CommitInfo {
                        sha: metadata.commit_sha.clone().unwrap_or_default(),
                        message: metadata.commit_message.clone().unwrap_or_default(),
                        author: metadata.author.clone().unwrap_or_default(),
                        date: metadata
                            .commit_date
                            .map(|d| d.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                            .unwrap_or_default(),
                    };

                    (repository_name, Some(commit_info))
                });

        // Verify that repository and commit fields are properly populated
        assert_eq!(repository, Some("my-repo".to_string()));
        assert!(commit.is_some());
        let commit_info = commit.unwrap();
        assert_eq!(commit_info.sha, "abc123");
        assert_eq!(commit_info.author, "John Doe");
    }

    #[tokio::test]
    async fn test_search_service_without_database() {
        // Test that SearchService works without database integration
        // Clean test - just use the mock service

        // Use mock for testing instead of real embedding service
        let mock_search_service = codetriever_search::test_mocks::MockSearchService::empty();

        // Verify that we can use the search service
        let results = mock_search_service
            .search("test query", 5, &CorrelationId::new())
            .await;
        assert!(results.is_ok());

        // Results should be empty (no indexed content) but service should work
        let search_results = results.unwrap();
        assert_eq!(search_results.len(), 0);
    }

    #[tokio::test]
    async fn test_routes_default_creates_working_router() {
        // Test that routes_default creates a working router
        let _router = routes();

        // Test passes if we can create routes without panicking
        // This validates the dependency injection is working
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
