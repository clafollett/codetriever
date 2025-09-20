//! Search service implementation

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};

use super::{RepositoryMetadata, SearchProvider, SearchResult};
use crate::{CorrelationId, Indexer, Result};
use codetriever_data::{DataClient, FileRepository, ProjectBranch};

// Type aliases to simplify complex types
type RepoBranchPairs = Vec<(String, String)>;
type ProjectBranchMap = std::collections::HashMap<(String, String), ProjectBranch>;
type SearchCache = Arc<std::sync::Mutex<lru::LruCache<String, Vec<SearchResult>>>>;

/// Search service that provides semantic code search with optional repository metadata
/// This is the unified search service that works with or without database integration
/// Includes built-in resilience with retry logic and graceful degradation
pub struct SearchService {
    indexer: Arc<RwLock<Indexer>>,
    db_client: Option<Arc<DataClient>>,
    max_retries: usize,
    retry_delay: Duration,
    search_timeout: Duration,
    // Simple in-memory cache for search results
    cache: SearchCache,
}

impl SearchService {
    /// Create a new search service with database integration and default resilience
    pub fn new(indexer: Arc<RwLock<Indexer>>, db_client: Arc<DataClient>) -> Self {
        Self {
            indexer,
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
    pub fn without_database(indexer: Arc<RwLock<Indexer>>) -> Self {
        Self {
            indexer,
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
        indexer: Arc<RwLock<Indexer>>,
        db_client: Option<Arc<DataClient>>,
        max_retries: usize,
        retry_delay: Duration,
        search_timeout: Duration,
    ) -> Self {
        Self {
            indexer,
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
        mut results: Vec<SearchResult>,
        correlation_id: &CorrelationId,
    ) -> Result<Vec<SearchResult>> {
        // Record correlation ID in tracing span
        tracing::Span::current().record("correlation_id", correlation_id);
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
    async fn search_with_correlation_id(
        &self,
        query: &str,
        limit: usize,
        correlation_id: CorrelationId,
    ) -> Result<Vec<SearchResult>> {
        // Record correlation ID in tracing span
        tracing::Span::current().record("correlation_id", &correlation_id);
        let start_time = std::time::Instant::now();

        // Check cache first
        let cache_key = format!("{query}:{limit}");
        if let Ok(mut cache) = self.cache.lock()
            && let Some(cached_results) = cache.get(&cache_key)
        {
            tracing::Span::current().record("cached", true);
            tracing::info!("Cache hit for query: {}", query);
            metrics::counter!("search_cache_hits").increment(1);
            metrics::histogram!("search_duration_ms")
                .record(start_time.elapsed().as_millis() as f64);
            return Ok(cached_results.clone());
        }

        // Retry search with exponential backoff for resilience
        for attempt in 0..=self.max_retries {
            match self.try_search(query, limit, &correlation_id).await {
                Ok(results) => {
                    // Cache successful results
                    if let Ok(mut cache) = self.cache.lock() {
                        cache.put(cache_key, results.clone());
                    }
                    tracing::info!("Search completed successfully on attempt {}", attempt + 1);

                    // Record metrics
                    metrics::counter!("search_requests_total").increment(1);
                    metrics::counter!("search_cache_misses").increment(1);
                    metrics::histogram!("search_duration_ms")
                        .record(start_time.elapsed().as_millis() as f64);
                    metrics::histogram!("search_results_count").record(results.len() as f64);

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
                    // Record failure metrics
                    metrics::counter!("search_requests_total").increment(1);
                    metrics::counter!("search_failures_total").increment(1);
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
    ) -> Result<Vec<SearchResult>> {
        // Record correlation ID in tracing span
        tracing::Span::current().record("correlation_id", correlation_id);
        // Wrap entire search operation in timeout for production resilience
        tokio::time::timeout(self.search_timeout, async {
            // First get basic search results from indexer
            tracing::debug!("Acquiring indexer write lock");
            let mut indexer = self.indexer.write().await;
            tracing::debug!("Performing vector search");
            let results = indexer.search(query, limit).await?;
            drop(indexer); // Release lock early
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
            crate::Error::search_timeout(query, self.search_timeout, correlation_id.clone())
        })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CodeChunk;
    use chrono::Utc;

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

        let result = SearchResult {
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
