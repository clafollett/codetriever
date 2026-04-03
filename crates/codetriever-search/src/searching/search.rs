//! Search service implementation

use super::service::SearchService;
use crate::error::SearchError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use codetriever_common::CorrelationId;
use codetriever_embeddings::EmbeddingService;
use codetriever_meta_data::DataClient;
use codetriever_parsing::CodeChunk;
use codetriever_vector_data::{SearchFilters, VectorStorage};
use std::sync::Arc;
use tokio::time::{Duration, sleep};

// Type aliases to simplify complex types
type SearchCache = Arc<std::sync::Mutex<lru::LruCache<String, Vec<SearchMatch>>>>;

/// Repository metadata for search results
#[derive(Debug, Clone)]
pub struct RepositoryMetadata {
    pub repository_id: String,
    pub repository_url: Option<String>,
    pub branch: String,
    pub commit_sha: Option<String>,
    pub commit_message: Option<String>,
    pub commit_date: Option<DateTime<Utc>>,
    pub author: Option<String>,
}

/// Result from a search operation including similarity score and repository metadata
#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub chunk: CodeChunk,
    pub similarity: f32,
    /// Repository metadata populated from database
    pub repository_metadata: Option<RepositoryMetadata>,
}

/// Result type for search operations
pub type SearchResult<T> = std::result::Result<T, SearchError>;

/// Search service that provides semantic code search with repository metadata.
/// Includes built-in resilience with retry logic.
pub struct Search {
    embedding_service: Arc<dyn EmbeddingService>,
    vector_storage: Arc<dyn VectorStorage>,
    db_client: Arc<DataClient>,
    max_retries: usize,
    retry_delay: Duration,
    search_timeout: Duration,
    // Simple in-memory cache for search results
    cache: SearchCache,
}

/// Normalize a string for fuzzy symbol matching: lowercase and strip separators.
///
/// `"ParseConfig"` becomes `"parseconfig"`, `"parse_config"` becomes `"parseconfig"`
fn normalize_symbol(s: &str) -> String {
    s.to_lowercase().replace(['_', '-'], "")
}

/// Apply score adjustments based on metadata signals already present in search results.
///
/// Boosts and penalties are multiplicative and compound:
/// - Exact symbol name match in query: +15%
/// - Definition kind (function/method/class/etc.): +10%
/// - Test/generated/fixture file path: -20%
fn rerank(query: &str, results: &mut [SearchMatch]) {
    let query_norm = normalize_symbol(query);

    for result in results.iter_mut() {
        let mut boost: f32 = 1.0;

        // Case/separator-insensitive symbol name match: +15%
        if let Some(ref name) = result.chunk.name {
            let name_norm = normalize_symbol(name);
            if query_norm.contains(&name_norm) || name_norm.contains(&query_norm) {
                boost *= 1.15;
            }
        }

        // Definition kind boost: +10%
        if let Some(ref kind) = result.chunk.kind
            && matches!(
                kind.as_str(),
                "function"
                    | "method"
                    | "class"
                    | "struct"
                    | "enum"
                    | "trait"
                    | "impl"
                    | "interface"
            )
        {
            boost *= 1.10;
        }

        // Test/generated file penalty: -20%
        // Match on path segments to avoid false positives like "attestation" or "contest"
        let path = &result.chunk.file_path;
        let is_test_path = path.split('/').any(|seg| {
            matches!(
                seg,
                "test" | "tests" | "spec" | "specs" | "mock" | "mocks" | "generated" | "fixtures"
            )
        }) || path.rsplit('/').next().is_some_and(|filename| {
            filename.contains("_test.")
                || filename.contains("_spec.")
                || filename.contains(".test.")
                || filename.contains(".spec.")
        });
        if is_test_path {
            boost *= 0.80;
        }

        result.similarity *= boost;
    }
}

impl Search {
    /// Create a search service with full dependency injection and database integration.
    pub fn new(
        embedding_service: Arc<dyn EmbeddingService>,
        vector_storage: Arc<dyn VectorStorage>,
        db_client: Arc<DataClient>,
    ) -> Self {
        Self {
            embedding_service,
            vector_storage,
            db_client,
            max_retries: 3,
            retry_delay: Duration::from_millis(100),
            search_timeout: Duration::from_secs(30),
            cache: Arc::new(std::sync::Mutex::new(lru::LruCache::new(
                std::num::NonZeroUsize::new(100).unwrap(),
            ))),
        }
    }

    /// Create with custom retry configuration for production tuning
    pub fn with_retry_config(
        embedding_service: Arc<dyn EmbeddingService>,
        vector_storage: Arc<dyn VectorStorage>,
        db_client: Arc<DataClient>,
        max_retries: usize,
        retry_delay: Duration,
        search_timeout: Duration,
    ) -> Self {
        Self {
            embedding_service,
            vector_storage,
            db_client,
            max_retries,
            retry_delay,
            search_timeout,
            cache: Arc::new(std::sync::Mutex::new(lru::LruCache::new(
                std::num::NonZeroUsize::new(100).unwrap(),
            ))),
        }
    }

    /// Internal search attempt - can fail and be retried
    #[tracing::instrument(skip(self), fields(tenant_id = %tenant_id, correlation_id))]
    async fn try_search(
        &self,
        tenant_id: &uuid::Uuid,
        repository_id: Option<&str>,
        branch: Option<&str>,
        query: &str,
        limit: usize,
        correlation_id: &CorrelationId,
    ) -> SearchResult<Vec<SearchMatch>> {
        tracing::Span::current().record("correlation_id", correlation_id.to_string());

        // Wrap entire search operation in timeout for production resilience
        tokio::time::timeout(self.search_timeout, async {
            tracing::debug!("Generating embeddings for search query");
            // Generate embedding for the query directly
            let embeddings = self
                .embedding_service
                .generate_embeddings(vec![query])
                .await?;

            let query_embedding = embeddings.into_iter().next().ok_or_else(|| {
                crate::SearchError::EmbeddingFailed {
                    query: query.to_string(),
                    correlation_id: correlation_id.clone(),
                }
            })?;

            // Build filters for Qdrant payload filtering (applied at vector search level)
            let filters = SearchFilters {
                repository_id: repository_id.map(|s| s.to_string()),
                branch: branch.map(|s| s.to_string()),
            };

            tracing::debug!(
                repository_filter = ?filters.repository_id,
                branch_filter = ?filters.branch,
                "Performing vector search with tenant isolation and payload filters"
            );

            // Over-fetch to give the re-ranker enough candidates to work with.
            // Re-ranking adjusts scores based on metadata signals, so the final
            // top-k may differ from raw cosine order.
            let fetch_limit = limit.saturating_mul(3);

            // Search in vector storage with tenant + payload filtering
            // Filters are applied at Qdrant level for efficiency
            let storage_results = self
                .vector_storage
                .search(
                    tenant_id,
                    query_embedding,
                    fetch_limit,
                    &filters,
                    correlation_id,
                )
                .await?;

            // Convert StorageSearchResult to SearchMatch
            // Metadata is complete from Qdrant payload - no enrichment needed!
            let mut results: Vec<SearchMatch> = storage_results
                .into_iter()
                .map(|r| {
                    // Convert vector-data RepositoryMetadata to search RepositoryMetadata
                    let repo_metadata = RepositoryMetadata {
                        repository_id: r.metadata.repository_id,
                        repository_url: r.metadata.repository_url,
                        branch: r.metadata.branch,
                        commit_sha: r.metadata.commit_sha,
                        commit_message: r.metadata.commit_message,
                        commit_date: r.metadata.commit_date,
                        author: r.metadata.author,
                    };

                    SearchMatch {
                        chunk: r.chunk,
                        similarity: r.similarity,
                        repository_metadata: Some(repo_metadata),
                    }
                })
                .collect();

            // Apply metadata-signal re-ranking, re-sort, then truncate to requested limit
            rerank(query, &mut results);
            results.sort_by(|a, b| {
                b.similarity
                    .partial_cmp(&a.similarity)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            results.truncate(limit);

            tracing::debug!(
                fetched = results.len(),
                limit,
                "Search re-ranked and truncated results"
            );

            Ok(results)
        })
        .await
        .map_err(|_| {
            tracing::error!(
                "Search operation timed out after {:?} for query '{}' (correlation_id: {})",
                self.search_timeout,
                query,
                correlation_id
            );
            crate::SearchError::SearchTimeout {
                query: query.to_string(),
                timeout_ms: self.search_timeout.as_millis() as u64,
                correlation_id: correlation_id.clone(),
            }
        })?
    }
}

#[async_trait]
impl SearchService for Search {
    #[tracing::instrument(skip(self), fields(tenant_id = %tenant_id, repository_id, branch, query, limit, correlation_id, cached = false))]
    async fn search(
        &self,
        tenant_id: &uuid::Uuid,
        repository_id: Option<&str>,
        branch: Option<&str>,
        query: &str,
        limit: usize,
        correlation_id: &CorrelationId,
    ) -> SearchResult<Vec<SearchMatch>> {
        tracing::Span::current().record("correlation_id", correlation_id.to_string());
        if let Some(repo) = repository_id {
            tracing::Span::current().record("repository_id", repo);
        }
        if let Some(b) = branch {
            tracing::Span::current().record("branch", b);
        }

        let _start_time = std::time::Instant::now(); // TODO: Use for metrics once enabled

        // Check cache first (include tenant_id and filters in cache key for security/correctness)
        let cache_key = format!(
            "{}:{}:{}:{}:{}",
            tenant_id,
            repository_id.unwrap_or("all"),
            branch.unwrap_or("all"),
            query,
            limit
        );
        if let Ok(mut cache) = self.cache.lock()
            && let Some(cached_results) = cache.get(&cache_key)
        {
            tracing::Span::current().record("cached", true);
            tracing::info!("Cache hit for query: {}", query);
            // TODO: Add metrics once we get compilation working
            // metrics::counter!("search_cache_hits").increment(1);
            // metrics::histogram!("search_duration_ms").record(start_time.elapsed().as_millis() as f64);
            return Ok(cached_results.clone());
        }

        // Retry search with exponential backoff for resilience
        for attempt in 0..=self.max_retries {
            match self
                .try_search(
                    tenant_id,
                    repository_id,
                    branch,
                    query,
                    limit,
                    correlation_id,
                )
                .await
            {
                Ok(results) => {
                    // Cache successful results
                    if let Ok(mut cache) = self.cache.lock() {
                        cache.put(cache_key, results.clone());
                    }
                    tracing::info!("Search completed successfully on attempt {}", attempt + 1);

                    // TODO: Add metrics once we get compilation working
                    // metrics::counter!("search_requests_total").increment(1);
                    // metrics::counter!("search_cache_misses").increment(1);
                    // metrics::histogram!("search_duration_ms").record(start_time.elapsed().as_millis() as f64);
                    // metrics::histogram!("search_results_count").record(results.len() as f64);

                    return Ok(results);
                }
                Err(e) if attempt < self.max_retries => {
                    // Exponential backoff: delay increases with each retry
                    let delay = self.retry_delay * 2_u32.pow(attempt as u32);
                    tracing::warn!(
                        "Search attempt {} failed, retrying in {:?}: {:?}",
                        attempt + 1,
                        delay,
                        e
                    );
                    sleep(delay).await;
                }
                Err(e) => {
                    tracing::error!(
                        "Search failed after {} attempts: {:?}",
                        self.max_retries + 1,
                        e
                    );
                    // TODO: Add metrics once we get compilation working
                    // metrics::counter!("search_requests_total").increment(1);
                    // metrics::counter!("search_failures_total").increment(1);
                    return Err(e);
                }
            }
        }

        unreachable!() // Loop should always return or error
    }

    #[tracing::instrument(skip(self), fields(file_path, correlation_id))]
    async fn get_context(
        &self,
        repository_id: Option<&str>,
        branch: Option<&str>,
        file_path: &str,
        correlation_id: &CorrelationId,
    ) -> SearchResult<super::service::ContextResult> {
        use crate::error::SearchError;

        tracing::Span::current().record("correlation_id", correlation_id.to_string());
        tracing::Span::current().record("file_path", file_path);

        tracing::info!(
            correlation_id = %correlation_id,
            repository_id = ?repository_id,
            branch = ?branch,
            file_path = %file_path,
            "Retrieving file context"
        );

        // Fetch file content from database
        let result = self
            .db_client
            .get_file_content(repository_id, branch, file_path)
            .await
            .map_err(SearchError::MetaDataError)?;

        let (repo_id, br, content) = match result {
            Some(data) => data,
            None => {
                tracing::warn!(
                    correlation_id = %correlation_id,
                    file_path = %file_path,
                    "File not found in index"
                );
                // Return empty result for not found (API layer will handle as 404)
                return Ok(super::service::ContextResult {
                    repository_id: String::new(),
                    branch: String::new(),
                    file_content: String::new(),
                });
            }
        };

        tracing::info!(
            correlation_id = %correlation_id,
            repository_id = %repo_id,
            branch = %br,
            file_size = content.len(),
            "File context retrieved successfully"
        );

        Ok(super::service::ContextResult {
            repository_id: repo_id,
            branch: br,
            file_content: content,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use codetriever_vector_data::CodeChunk;

    /// Build a minimal `SearchMatch` for re-ranking unit tests.
    /// Only the fields exercised by `rerank()` need real values.
    fn make_match(
        file_path: &str,
        name: Option<&str>,
        kind: Option<&str>,
        similarity: f32,
    ) -> SearchMatch {
        SearchMatch {
            chunk: CodeChunk {
                file_path: file_path.to_string(),
                content: String::new(),
                start_line: 1,
                end_line: 1,
                byte_start: 0,
                byte_end: 0,
                kind: kind.map(str::to_string),
                language: "rust".to_string(),
                name: name.map(str::to_string),
                token_count: None,
                embedding: None,
            },
            similarity,
            repository_metadata: None,
        }
    }

    // --- Re-ranking unit tests (pure logic, no I/O) ---

    #[test]
    fn test_symbol_name_match_boosted() {
        // A result whose chunk.name matches the query receives the +15% name boost.
        let query = "parse_config";
        let mut results = vec![make_match("src/config.rs", Some("parse_config"), None, 1.0)];

        rerank(query, &mut results);

        let expected = 1.0_f32 * 1.15;
        assert!(
            (results[0].similarity - expected).abs() < 1e-5,
            "name match should yield {expected:.4}, got {:.4}",
            results[0].similarity
        );
    }

    #[test]
    fn test_definition_kind_boosted() {
        // kind="function" earns +10%; kind="comment" receives no kind boost.
        let query = "anything";
        let mut results = vec![
            make_match("src/a.rs", None, Some("function"), 1.0),
            make_match("src/b.rs", None, Some("comment"), 1.0),
        ];

        rerank(query, &mut results);

        assert!(
            (results[0].similarity - 1.10).abs() < 1e-5,
            "function kind should score 1.10, got {:.4}",
            results[0].similarity
        );
        assert!(
            (results[1].similarity - 1.0).abs() < 1e-5,
            "comment kind should stay 1.0, got {:.4}",
            results[1].similarity
        );
    }

    #[test]
    fn test_test_file_penalized() {
        // Paths containing "test" receive the -20% penalty; clean paths do not.
        let query = "anything";
        let mut results = vec![
            make_match("src/tests/config_test.rs", None, None, 1.0),
            make_match("src/config.rs", None, None, 1.0),
        ];

        rerank(query, &mut results);

        assert!(
            (results[0].similarity - 0.80).abs() < 1e-5,
            "test file should score 0.80, got {:.4}",
            results[0].similarity
        );
        assert!(
            (results[1].similarity - 1.0).abs() < 1e-5,
            "normal file should stay 1.0, got {:.4}",
            results[1].similarity
        );
    }

    #[test]
    fn test_boosts_compound() {
        // Name match (+15%) and function kind (+10%) compound: 1.0 * 1.15 * 1.10 = 1.265.
        let query = "parse_config";
        let mut results = vec![make_match(
            "src/config.rs",
            Some("parse_config"),
            Some("function"),
            1.0,
        )];

        rerank(query, &mut results);

        let expected = 1.0_f32 * 1.15 * 1.10;
        assert!(
            (results[0].similarity - expected).abs() < 1e-5,
            "compound boost should yield {expected:.4}, got {:.4}",
            results[0].similarity
        );
    }

    #[test]
    fn test_rerank_preserves_order_when_no_signals() {
        // When no signals fire, `rerank` leaves scores unchanged (boost stays 1.0).
        let query = "something_completely_unrelated";
        let mut results = vec![
            make_match("src/a.rs", None, None, 0.9),
            make_match("src/b.rs", None, None, 0.8),
            make_match("src/c.rs", None, None, 0.7),
        ];

        rerank(query, &mut results);

        assert!((results[0].similarity - 0.9).abs() < 1e-5);
        assert!((results[1].similarity - 0.8).abs() < 1e-5);
        assert!((results[2].similarity - 0.7).abs() < 1e-5);
    }

    #[test]
    fn test_overfetch_and_truncate() {
        // Simulates the over-fetch pipeline: rerank → sort descending → truncate to limit.
        // With no signals firing the scores are unchanged, so sort preserves the original
        // descending order and truncate returns exactly `limit` items.
        let query = "irrelevant";
        let limit = 3_usize;

        // 9 items (simulating limit * 3 over-fetch), descending similarity
        let mut results: Vec<SearchMatch> = (0..9_u32)
            .rev()
            .map(|i| make_match("src/a.rs", None, None, 0.1 * i as f32))
            .collect();

        rerank(query, &mut results);
        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);

        assert_eq!(
            results.len(),
            limit,
            "truncate should return exactly {limit} results"
        );
        // Top result should be the highest score (0.8 = 0.1 * 8)
        assert!(
            (results[0].similarity - 0.8).abs() < 1e-5,
            "top result should have similarity 0.8, got {:.4}",
            results[0].similarity
        );
    }

    #[test]
    fn test_symbol_name_match_case_insensitive() {
        // "ParseConfig" query should boost chunk named "parse_config" and vice versa
        let query = "ParseConfig";
        let mut results = vec![make_match("src/config.rs", Some("parse_config"), None, 1.0)];

        rerank(query, &mut results);

        let expected = 1.0_f32 * 1.15;
        assert!(
            (results[0].similarity - expected).abs() < 1e-5,
            "case-insensitive name match should yield {expected:.4}, got {:.4}",
            results[0].similarity
        );
    }

    #[test]
    fn test_attestation_path_not_penalized() {
        // "attestation" contains "test" as a substring but is NOT a test path
        let query = "anything";
        let mut results = vec![
            make_match("src/attestation/validator.rs", None, None, 1.0),
            make_match("src/contest/rules.rs", None, None, 1.0),
            make_match("src/latest_config.rs", None, None, 1.0),
        ];

        rerank(query, &mut results);

        for result in &results {
            assert!(
                (result.similarity - 1.0).abs() < 1e-5,
                "path '{}' should NOT be penalized, got {:.4}",
                result.chunk.file_path,
                result.similarity
            );
        }
    }

    #[test]
    fn test_search_result_with_metadata() {
        // Test that SearchResult can hold repository metadata
        let metadata = RepositoryMetadata {
            repository_id: "test-repo".to_string(),
            repository_url: Some("https://github.com/test/repo".to_string()),
            branch: "main".to_string(),
            commit_sha: Some("abc123".to_string()),
            commit_message: Some("Test commit".to_string()),
            commit_date: Some(Utc::now()),
            author: Some("Test Author".to_string()),
        };

        let result = SearchMatch {
            chunk: CodeChunk {
                file_path: "src/test.rs".to_string(),
                content: "fn test() {}".to_string(),
                start_line: 1,
                end_line: 1,
                byte_start: 0,
                byte_end: 12,
                kind: Some("function".to_string()),
                language: "rust".to_string(),
                name: Some("test".to_string()),
                token_count: Some(3),
                embedding: None,
            },
            similarity: 0.95,
            repository_metadata: Some(metadata),
        };

        // Verify that metadata is accessible
        assert!(result.repository_metadata.is_some());
        let metadata = result.repository_metadata.as_ref().unwrap();
        assert_eq!(metadata.repository_id, "test-repo");
        assert_eq!(metadata.branch, "main");
        assert_eq!(
            metadata.repository_url,
            Some("https://github.com/test/repo".to_string())
        );
        assert_eq!(metadata.commit_sha, Some("abc123".to_string()));
        assert_eq!(metadata.commit_message, Some("Test commit".to_string()));
        assert_eq!(metadata.author, Some("Test Author".to_string()));
    }
}
