//! Test utilities for search services

use super::search::{SearchMatch, SearchResult};
use super::service::SearchService;

use async_trait::async_trait;
use codetriever_common::CorrelationId;
use codetriever_vector_data::CodeChunk;

/// Type alias for test search results (file_path, content, similarity)
type TestSearchResult = (String, String, f32);

/// Rich test search result with optional name and kind
pub struct TestSearchMatch {
    pub file_path: String,
    pub content: String,
    pub similarity: f32,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub language: String,
    pub start_line: usize,
    pub end_line: usize,
}

impl TestSearchMatch {
    /// Create a minimal test match with name and kind
    pub fn new(
        file_path: &str,
        content: &str,
        similarity: f32,
        name: Option<&str>,
        kind: Option<&str>,
    ) -> Self {
        Self {
            file_path: file_path.to_string(),
            content: content.to_string(),
            similarity,
            name: name.map(String::from),
            kind: kind.map(String::from),
            language: "rust".to_string(),
            start_line: 1,
            end_line: 10,
        }
    }
}

/// Mock search service for testing
pub struct MockSearch {
    results: Vec<SearchMatch>,
}

impl MockSearch {
    /// Create a mock that returns specific results.
    ///
    /// Note: chunks will have `name: None` and `kind: Some("function")`.
    /// For tests that need named chunks (e.g., usages classification), use
    /// `with_matches` and `TestSearchMatch` instead.
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
                repository_metadata: None,
            })
            .collect();

        Self { results }
    }

    /// Create a mock with rich test data including names and kinds
    pub fn with_matches(matches: Vec<TestSearchMatch>) -> Self {
        let results = matches
            .into_iter()
            .map(|m| SearchMatch {
                chunk: CodeChunk {
                    file_path: m.file_path,
                    content: m.content,
                    start_line: m.start_line,
                    end_line: m.end_line,
                    byte_start: 0,
                    byte_end: 100,
                    kind: m.kind,
                    language: m.language,
                    name: m.name,
                    token_count: Some(50),
                    embedding: None,
                },
                similarity: m.similarity,
                repository_metadata: None,
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
        _repository_id: Option<&str>,
        _branch: Option<&str>,
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
