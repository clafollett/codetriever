//! Core indexing logic for Codetriever
//!
//! This crate contains all the indexing, parsing, and embedding logic
//! separated from the API layer for better testing and modularity.

pub mod chunking;
pub mod config;
pub mod embedding;
pub mod error;
pub mod indexing;
pub mod parsing;
pub mod storage;

// Re-export error types
pub use error::{Error, Result};

// Re-export main types
pub use embedding::EmbeddingModel;
pub use indexing::{ApiIndexerService, IndexResult, Indexer, IndexerService};
pub use parsing::{CodeChunk, CodeParser};

// Re-export test utilities when test-utils feature is enabled
#[cfg(any(test, feature = "test-utils"))]
pub mod test_mocks {
    pub use crate::indexing::test_utils::MockIndexerService;
}
