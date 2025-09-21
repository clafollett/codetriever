//! Codetriever search orchestration crate
//!
//! This crate provides semantic code search functionality by orchestrating
//! embedding generation, vector similarity search, and metadata enrichment.

pub mod search;

// Re-export main types
pub use search::{
    RepositoryMetadata, SearchError, SearchMatch, SearchProvider, SearchResult, SearchService,
};

// Re-export test utilities when test-utils feature is enabled
#[cfg(any(test, feature = "test-utils"))]
pub mod test_mocks {
    pub use crate::search::test_utils::MockSearchService;
}
