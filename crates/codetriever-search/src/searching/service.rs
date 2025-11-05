//! Search service implementation

use super::{SearchMatch, SearchResult};
use async_trait::async_trait;
use codetriever_common::CorrelationId;
use uuid::Uuid;

/// Context retrieval result containing file metadata and content
#[derive(Debug, Clone)]
pub struct ContextResult {
    /// Repository identifier
    pub repository_id: String,
    /// Branch name
    pub branch: String,
    /// Full file content (UTF-8)
    pub file_content: String,
}

// Type aliases to simplify complex types
/// Trait for search operations with correlation ID support
#[async_trait]
pub trait SearchService: Send + Sync {
    /// Search for code chunks matching the query with tenant isolation
    async fn search(
        &self,
        tenant_id: &Uuid,
        query: &str,
        limit: usize,
        correlation_id: &CorrelationId,
    ) -> SearchResult<Vec<SearchMatch>>;

    /// Get file context from indexed repository
    ///
    /// Retrieves full file content with metadata. If repository_id or branch
    /// are None, returns the most recently indexed version of the file.
    async fn get_context(
        &self,
        repository_id: Option<&str>,
        branch: Option<&str>,
        file_path: &str,
        correlation_id: &CorrelationId,
    ) -> SearchResult<ContextResult>;
}
