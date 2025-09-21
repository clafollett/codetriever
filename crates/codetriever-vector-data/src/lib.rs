//! Codetriever vector data storage crate
//!
//! This crate provides vector database operations for storing and retrieving
//! code embeddings. It supports multiple backends like Qdrant and includes
//! mock implementations for testing.

pub mod error;
pub mod storage;

// Re-export main types
pub use error::{VectorDataError, VectorDataResult};
pub use storage::{
    CodeChunk, MockStorage, QdrantStorage, StorageSearchResult, StorageStats, VectorStorage,
};
// Use unified configuration from codetriever-config
pub use codetriever_config::VectorStorageConfig;
