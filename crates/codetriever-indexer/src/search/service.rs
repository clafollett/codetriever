//! Search service implementation

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::{RepositoryMetadata, SearchProvider, SearchResult};
use crate::{Indexer, Result};
use codetriever_data::{DataClient, FileRepository};

/// API search service that uses the indexer
pub struct ApiSearchService {
    indexer: Arc<Mutex<Indexer>>,
}

impl ApiSearchService {
    /// Create a new search service
    pub fn new() -> Self {
        Self {
            indexer: Arc::new(Mutex::new(Indexer::new())),
        }
    }

    /// Create with existing indexer
    pub fn with_indexer(indexer: Arc<Mutex<Indexer>>) -> Self {
        Self { indexer }
    }
}

impl Default for ApiSearchService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchProvider for ApiSearchService {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let mut indexer = self.indexer.lock().await;
        // Now indexer.search returns Vec<SearchResult> with actual scores!
        indexer.search(query, limit).await
    }
}

/// Enhanced search service that enriches results with database metadata
pub struct EnhancedSearchService {
    indexer: Arc<Mutex<Indexer>>,
    db_client: Arc<DataClient>,
}

impl EnhancedSearchService {
    /// Create a new enhanced search service
    pub fn new(indexer: Arc<Mutex<Indexer>>, db_client: Arc<DataClient>) -> Self {
        Self { indexer, db_client }
    }

    /// Enrich search results with repository metadata from database
    async fn enrich_with_metadata(
        &self,
        mut results: Vec<SearchResult>,
    ) -> Result<Vec<SearchResult>> {
        // Extract unique file paths from results
        let file_paths: Vec<&str> = results.iter().map(|r| r.chunk.file_path.as_str()).collect();

        // Batch query database for file metadata
        let files_metadata = self
            .db_client
            .repository()
            .get_files_metadata(&file_paths)
            .await?;

        // Create a lookup map for efficient metadata retrieval
        let mut metadata_map = std::collections::HashMap::new();
        for file in files_metadata {
            // Also fetch project branch info for repository URL
            if let Ok(Some(project_branch)) = self
                .db_client
                .repository()
                .get_project_branch(&file.repository_id, &file.branch)
                .await
            {
                let repo_metadata = RepositoryMetadata {
                    repository_id: file.repository_id.clone(),
                    repository_url: project_branch.repository_url,
                    branch: file.branch.clone(),
                    commit_sha: file.commit_sha,
                    commit_message: file.commit_message,
                    commit_date: file.commit_date,
                    author: file.author,
                };
                metadata_map.insert(file.file_path, repo_metadata);
            }
        }

        // Enrich results with metadata
        for result in &mut results {
            result.repository_metadata = metadata_map.remove(&result.chunk.file_path);
        }

        Ok(results)
    }
}

#[async_trait]
impl SearchProvider for EnhancedSearchService {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // First get basic search results from indexer
        let mut indexer = self.indexer.lock().await;
        let results = indexer.search(query, limit).await?;
        drop(indexer); // Release lock early

        // Enrich with database metadata
        self.enrich_with_metadata(results).await
    }
}

/// Trait for services that can perform search operations
#[async_trait]
pub trait SearchService: Send + Sync {
    /// Search for code matching the query
    async fn search(&mut self, query: &str, limit: Option<usize>) -> Result<Vec<SearchResult>>;
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
