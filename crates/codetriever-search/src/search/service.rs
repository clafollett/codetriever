//! Search service implementation

use async_trait::async_trait;
use std::sync::Arc;
use tokio::time::{Duration, sleep};

use super::{RepositoryMetadata, SearchMatch, SearchProvider, SearchResult};
use codetriever_common::CorrelationId;
use codetriever_embeddings::EmbeddingService;
use codetriever_meta_data::{DataClient, FileRepository, ProjectBranch};
use codetriever_vector_data::VectorStorage;

// Type aliases to simplify complex types
type RepoBranchPairs = Vec<(String, String)>;
type ProjectBranchMap = std::collections::HashMap<(String, String), ProjectBranch>;
type SearchCache = Arc<std::sync::Mutex<lru::LruCache<String, Vec<SearchMatch>>>>;

/// Search service that provides semantic code search with optional repository metadata
/// This is the unified search service that works with or without database integration
/// Includes built-in resilience with retry logic and graceful degradation
pub struct SearchService {
    embedding_service: Arc<dyn EmbeddingService>,
    vector_storage: Arc<dyn VectorStorage>,
    db_client: Option<Arc<DataClient>>,
    max_retries: usize,
    retry_delay: Duration,
    search_timeout: Duration,
    // Simple in-memory cache for search results
    cache: SearchCache,
}

impl Default for SearchService {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchService {
    /// Create a new search service with default configuration (like ApiIndexerService::new)
    pub fn new() -> Self {
        // Construct dependencies with defaults - no injection needed for simple usage
        let embedding_service = Arc::new(codetriever_embeddings::DefaultEmbeddingService::new(
            codetriever_embeddings::EmbeddingConfig::default(),
        )) as Arc<dyn EmbeddingService>;

        let vector_storage =
            Arc::new(codetriever_vector_data::MockStorage::new()) as Arc<dyn VectorStorage>;

        Self {
            embedding_service,
            vector_storage,
            db_client: None,
            max_retries: 3,
            retry_delay: Duration::from_millis(100),
            search_timeout: Duration::from_secs(30),
            cache: Arc::new(std::sync::Mutex::new(lru::LruCache::new(
                std::num::NonZeroUsize::new(100).unwrap(),
            ))),
        }
    }

    /// Create a search service with full dependency injection and database integration
    pub fn with_dependencies(
        embedding_service: Arc<dyn EmbeddingService>,
        vector_storage: Arc<dyn VectorStorage>,
        db_client: Arc<DataClient>,
    ) -> Self {
        Self {
            embedding_service,
            vector_storage,
            db_client: Some(db_client),
            max_retries: 3,
            retry_delay: Duration::from_millis(100),
            search_timeout: Duration::from_secs(30),
            cache: Arc::new(std::sync::Mutex::new(lru::LruCache::new(
                std::num::NonZeroUsize::new(100).unwrap(),
            ))),
        }
    }

    /// Create a search service without database integration (for dev/testing)
    pub fn without_database(
        embedding_service: Arc<dyn EmbeddingService>,
        vector_storage: Arc<dyn VectorStorage>,
    ) -> Self {
        Self {
            embedding_service,
            vector_storage,
            db_client: None,
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
        db_client: Option<Arc<DataClient>>,
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

    /// Enrich search results with repository metadata from database (if available)
    #[tracing::instrument(skip(self, results), fields(correlation_id, result_count = results.len()))]
    async fn enrich_with_metadata(
        &self,
        mut results: Vec<SearchMatch>,
        correlation_id: &CorrelationId,
    ) -> SearchResult<Vec<SearchMatch>> {
        tracing::Span::current().record("correlation_id", correlation_id.to_string());

        // If no database client, return results without metadata enrichment
        let db_client = match &self.db_client {
            Some(client) => client,
            None => return Ok(results),
        };

        // Extract unique file paths from results
        let file_paths: Vec<&str> = results.iter().map(|r| r.chunk.file_path.as_str()).collect();

        // Batch query database for file metadata
        let files_metadata = match db_client.repository().get_files_metadata(&file_paths).await {
            Ok(metadata) => metadata,
            Err(_) => {
                // Database unavailable - return results without metadata
                return Ok(results);
            }
        };

        // Extract unique repository/branch combinations for batch query
        let mut repo_branch_pairs = std::collections::HashSet::new();
        for file in &files_metadata {
            repo_branch_pairs.insert((file.repository_id.clone(), file.branch.clone()));
        }

        // Batch fetch all project branches in a single query
        let repo_branches: RepoBranchPairs = repo_branch_pairs.into_iter().collect();
        let project_branches = db_client
            .repository()
            .get_project_branches(&repo_branches)
            .await
            .unwrap_or_default(); // Database error - continue without project info

        // Create lookup map for project branches
        let project_branch_map: ProjectBranchMap = project_branches
            .into_iter()
            .map(|pb| ((pb.repository_id.clone(), pb.branch.clone()), pb))
            .collect();

        // Create metadata map with batched project branch data
        let metadata_map: std::collections::HashMap<String, RepositoryMetadata> = files_metadata
            .into_iter()
            .map(|file| {
                let project_key = (file.repository_id.clone(), file.branch.clone());
                let project_branch = project_branch_map.get(&project_key);

                let repo_metadata = RepositoryMetadata {
                    repository_id: file.repository_id,
                    repository_url: project_branch.and_then(|pb| pb.repository_url.clone()),
                    branch: file.branch,
                    commit_sha: file.commit_sha,
                    commit_message: file.commit_message,
                    commit_date: file.commit_date,
                    author: file.author,
                };

                (file.file_path, repo_metadata)
            })
            .collect();

        // Enrich results with metadata
        for result in &mut results {
            result.repository_metadata = metadata_map.get(&result.chunk.file_path).cloned();
        }

        Ok(results)
    }
}

#[async_trait]
impl SearchProvider for SearchService {
    #[tracing::instrument(skip(self), fields(query, limit, correlation_id, cached = false))]
    async fn search(
        &self,
        query: &str,
        limit: usize,
        correlation_id: &CorrelationId,
    ) -> SearchResult<Vec<SearchMatch>> {
        tracing::Span::current().record("correlation_id", correlation_id.to_string());

        let _start_time = std::time::Instant::now(); // TODO: Use for metrics once enabled

        // Check cache first
        let cache_key = format!("{query}:{limit}");
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
            match self.try_search(query, limit, correlation_id).await {
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
}

impl SearchService {
    /// Internal search attempt - can fail and be retried
    #[tracing::instrument(skip(self), fields(correlation_id))]
    async fn try_search(
        &self,
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

            tracing::debug!("Performing vector search");
            // Search in vector storage directly
            let storage_results = self
                .vector_storage
                .search(query_embedding, limit, correlation_id)
                .await?;

            // Convert StorageSearchResult to SearchMatch
            let results: Vec<SearchMatch> = storage_results
                .into_iter()
                .map(|r| SearchMatch {
                    chunk: r.chunk,
                    similarity: r.similarity,
                    repository_metadata: None, // Will be enriched below
                })
                .collect();
            tracing::debug!("Vector search returned {} results", results.len());

            // Enrich with database metadata
            tracing::debug!("Enriching with database metadata");
            let enriched = self.enrich_with_metadata(results, correlation_id).await?;
            tracing::debug!("Metadata enrichment complete");
            Ok(enriched)
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use codetriever_vector_data::CodeChunk;

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
