//! Test utilities for search services

use super::search::{SearchMatch, SearchResult};
use super::service::SearchService;

use async_trait::async_trait;
use codetriever_common::CorrelationId;
use codetriever_vector_data::CodeChunk;

/// Type alias for test search results (file_path, content, similarity)
type TestSearchResult = (String, String, f32);

/// Mock search service for testing
pub struct MockSearch {
    results: Vec<SearchMatch>,
}

impl MockSearch {
    /// Create a mock that returns specific results
    pub fn with_results(results: Vec<TestSearchResult>) -> Self {
        let results = results
            .into_iter()
            .map(|(file, content, similarity)| SearchMatch {
                chunk: CodeChunk {
                    file_path: file,
                    content,
                    start_line: 1,
                    end_line: 10,
                    byte_start: 0,
                    byte_end: 100,
                    kind: Some("function".to_string()),
                    language: "rust".to_string(),
                    name: None,
                    token_count: Some(50),
                    embedding: None,
                },
                similarity,
                repository_metadata: None, // Mock doesn't populate repository metadata
            })
            .collect();

        Self { results }
    }

    /// Create a mock that returns no results
    pub fn empty() -> Self {
        Self { results: vec![] }
    }
}

#[async_trait]
impl SearchService for MockSearch {
    async fn search(
        &self,
        _tenant_id: &uuid::Uuid,
        query: &str,
        limit: usize,
        correlation_id: &CorrelationId,
    ) -> SearchResult<Vec<SearchMatch>> {
        tracing::Span::current().record("query", query);
        tracing::Span::current().record("limit", limit);
        tracing::Span::current().record("correlation_id", correlation_id.to_string());
        let results: Vec<SearchMatch> = self.results.iter().take(limit).cloned().collect();
        Ok(results)
    }

    async fn get_context(
        &self,
        _repository_id: Option<&str>,
        _branch: Option<&str>,
        _file_path: &str,
        _correlation_id: &CorrelationId,
    ) -> SearchResult<super::service::ContextResult> {
        // Mock returns test file content
        Ok(super::service::ContextResult {
            repository_id: "test-repo".to_string(),
            branch: "main".to_string(),
            file_content: "fn test() {}\n".to_string(),
        })
    }
}
