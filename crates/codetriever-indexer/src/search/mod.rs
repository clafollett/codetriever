//! Search service module for querying indexed code

use crate::{CodeChunk, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};

pub mod service;

pub use service::{ApiSearchService, EnhancedSearchService, SearchService};

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

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
pub struct SearchResult {
    pub chunk: CodeChunk,
    pub similarity: f32,
    /// Repository metadata populated from database
    pub repository_metadata: Option<RepositoryMetadata>,
}

/// Trait for search operations
#[async_trait]
pub trait SearchProvider: Send + Sync {
    /// Search for code chunks matching the query
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;
}
