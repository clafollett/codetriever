//! Codetriever search orchestration crate
//!
//! This crate provides semantic code search functionality by orchestrating
//! embedding generation, vector similarity search, and metadata enrichment.

pub mod error;
pub mod searching;

// Re-export main types
pub use error::SearchError;
pub use searching::{
    search::{RepositoryMetadata, Search, SearchMatch, SearchResult},
    service::{ContextResult, SearchService},
};

// Re-export test utilities when test-utils feature is enabled
#[cfg(any(test, feature = "test-utils"))]
pub mod test_mocks {
    pub use crate::searching::test_utils::MockSearch;
}
