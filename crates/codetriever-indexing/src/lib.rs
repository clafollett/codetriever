//! Indexing orchestration crate for Codetriever
//!
//! This crate orchestrates the indexing process by coordinating between
//! the parsing, embedding, and vector storage services.

pub mod config;
pub mod error;
pub mod indexing;
pub mod queues;
pub mod security;
pub mod worker;

// Re-export error types
pub use error::{IndexerError, IndexerResult};

// Re-export main orchestration types
pub use indexing::{Indexer, IndexerService};
pub use worker::{BackgroundWorker, WorkerConfig};

// Re-export external crate types for convenience
pub use codetriever_embeddings::{EmbeddingError, EmbeddingResult};
pub use codetriever_meta_data::{MetaDataError, MetaDataResult};
pub use codetriever_parsing::{CodeChunk, CodeParser, ParsingError, ParsingResult};
pub use codetriever_vector_data::{VectorDataError, VectorDataResult};

// Re-export test utilities when test-utils feature is enabled
#[cfg(any(test, feature = "test-utils"))]
pub mod test_mocks {
    pub use crate::indexing::test_utils::MockIndexerService;
}
