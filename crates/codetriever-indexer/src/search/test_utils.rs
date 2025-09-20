//! Test utilities for search services

use super::{SearchProvider, SearchResult};
use crate::{CodeChunk, CorrelationId, Result};
use async_trait::async_trait;

/// Type alias for test search results (file_path, content, similarity)
type TestSearchResult = (String, String, f32);

/// Mock search service for testing
pub struct MockSearchService {
    results: Vec<SearchResult>,
}

impl MockSearchService {
    /// Create a mock that returns specific results
    pub fn with_results(results: Vec<TestSearchResult>) -> Self {
        let results = results
            .into_iter()
            .map(|(file, content, similarity)| SearchResult {
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
impl SearchProvider for MockSearchService {
    async fn search_with_correlation_id(
        &self,
        _query: &str,
        limit: usize,
        _correlation_id: CorrelationId,
    ) -> Result<Vec<SearchResult>> {
        let results: Vec<SearchResult> = self.results.iter().take(limit).cloned().collect();
        Ok(results)
    }
}
