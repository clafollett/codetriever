//! Search service implementation

use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::{RepositoryMetadata, SearchProvider, SearchResult};
use crate::{Indexer, Result};
use codetriever_data::{DataClient, FileRepository, ProjectBranch};

// Type aliases to simplify complex types
type RepoBranchPairs = Vec<(String, String)>;
type ProjectBranchMap = std::collections::HashMap<(String, String), ProjectBranch>;

/// Search service that provides semantic code search with optional repository metadata
/// This is the unified search service that works with or without database integration
pub struct SearchService {
    indexer: Arc<Mutex<Indexer>>,
    db_client: Option<Arc<DataClient>>,
}

impl SearchService {
    /// Create a new search service with database integration
    pub fn new(indexer: Arc<Mutex<Indexer>>, db_client: Arc<DataClient>) -> Self {
        Self {
            indexer,
            db_client: Some(db_client),
        }
    }

    /// Create a search service without database integration (for dev/testing)
    pub fn without_database(indexer: Arc<Mutex<Indexer>>) -> Self {
        Self {
            indexer,
            db_client: None,
        }
    }

    /// Enrich search results with repository metadata from database (if available)
    async fn enrich_with_metadata(
        &self,
        mut results: Vec<SearchResult>,
    ) -> Result<Vec<SearchResult>> {
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
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // First get basic search results from indexer
        let mut indexer = self.indexer.lock().await;
        let results = indexer.search(query, limit).await?;
        drop(indexer); // Release lock early

        // Enrich with database metadata
        self.enrich_with_metadata(results).await
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
