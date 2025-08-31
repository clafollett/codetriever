//! API Configuration Module
//!
//! This module provides configuration structs and defaults for the Codetriever API server.
//! The configuration manages various aspects of the system including vector database connections,
//! embedding models, chunking strategies, and platform-specific optimizations.
//!
//! # Examples
//!
//! ```
//! use codetriever_api::config::api::Config;
//!
//! // Use default configuration
//! let config = Config::default();
//!
//! // Create custom configuration
//! let config = Config {
//!     qdrant_url: "http://my-qdrant:6334".to_string(),
//!     embedding_model: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
//!     max_chunk_size: 1024,
//!     ..Default::default()
//! };
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for the Codetriever API server
///
/// This struct holds all necessary configuration parameters for running the Codetriever API,
/// including vector database settings, embedding model configuration, text chunking parameters,
/// and caching options.
///
/// # Fields
///
/// * `qdrant_url` - URL endpoint for the Qdrant vector database
/// * `qdrant_collection` - Name of the collection to store embeddings in Qdrant
/// * `embedding_model` - HuggingFace model identifier for generating embeddings
/// * `use_metal` - Whether to use Metal Performance Shaders on macOS for acceleration
/// * `cache_dir` - Directory path for storing cached embeddings and model data
/// * `max_chunk_size` - Maximum number of tokens per text chunk
/// * `chunk_overlap` - Number of overlapping tokens between adjacent chunks
///
/// # Examples
///
/// ```
/// use codetriever_api::config::api::Config;
/// use std::path::PathBuf;
///
/// let config = Config {
///     qdrant_url: "http://localhost:6334".to_string(),
///     qdrant_collection: "my_code_collection".to_string(),
///     embedding_model: "jinaai/jina-embeddings-v2-base-code".to_string(),
///     use_metal: true,
///     cache_dir: PathBuf::from("/tmp/codetriever"),
///     max_chunk_size: 512,
///     chunk_overlap: 50,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// URL endpoint for the Qdrant vector database server
    ///
    /// This should be a complete HTTP/HTTPS URL including protocol and port.
    /// Example: "http://localhost:6334" or "https://my-qdrant.example.com"
    pub qdrant_url: String,
    /// Name of the Qdrant collection to store code embeddings
    ///
    /// This collection will be created if it doesn't exist. Choose a descriptive
    /// name that reflects the purpose or scope of the code being indexed.
    pub qdrant_collection: String,
    /// HuggingFace model identifier for generating code embeddings
    ///
    /// Should be a valid model path on HuggingFace Hub that supports embeddings.
    /// Recommended models for code:
    /// - "jinaai/jina-embeddings-v2-base-code" (optimized for code)
    /// - "sentence-transformers/all-MiniLM-L6-v2" (general purpose, smaller)
    /// - "microsoft/codebert-base" (Microsoft's code-specific model)
    pub embedding_model: String,
    /// Whether to use Metal Performance Shaders for GPU acceleration on macOS
    ///
    /// When enabled on macOS systems with compatible GPUs, this can significantly
    /// speed up embedding generation. Automatically disabled on non-macOS platforms.
    /// Falls back to CPU computation if Metal is unavailable.
    pub use_metal: bool,
    /// Directory path for storing cached embeddings and downloaded models
    ///
    /// This directory will be created if it doesn't exist. The cache improves
    /// performance by avoiding re-computation of embeddings and re-downloading
    /// of models. Should have sufficient disk space for model files (~100MB-1GB).
    pub cache_dir: PathBuf,
    /// Maximum number of tokens per text chunk when splitting documents
    ///
    /// Larger chunks preserve more context but may exceed model token limits.
    /// Smaller chunks provide more granular search results but less context.
    /// Should be less than the embedding model's maximum sequence length.
    /// Typical values: 256-1024 tokens.
    pub max_chunk_size: usize,
    /// Number of overlapping tokens between adjacent chunks
    ///
    /// Overlap helps preserve context across chunk boundaries, which is especially
    /// important for code where function definitions might span multiple chunks.
    /// Should be significantly smaller than `max_chunk_size`.
    /// Typical values: 10-20% of `max_chunk_size`.
    pub chunk_overlap: usize,
}

impl Default for Config {
    /// Creates a new Config with sensible defaults for local development
    ///
    /// # Default Values
    ///
    /// * `qdrant_url`: "http://localhost:6334" - Local Qdrant instance
    /// * `qdrant_collection`: "codetriever" - Default collection name
    /// * `embedding_model`: "jinaai/jina-embeddings-v2-base-code" - Code-optimized model
    /// * `use_metal`: Enabled on macOS, disabled elsewhere
    /// * `cache_dir`: System cache directory + "codetriever" (falls back to ".cache/codetriever")
    /// * `max_chunk_size`: 512 tokens - Balance between context and model limits
    /// * `chunk_overlap`: 50 tokens - ~10% overlap for context preservation
    ///
    /// # Examples
    ///
    /// ```
    /// use codetriever_api::config::api::Config;
    ///
    /// let config = Config::default();
    /// assert_eq!(config.qdrant_url, "http://localhost:6334");
    /// assert_eq!(config.max_chunk_size, 512);
    /// assert_eq!(config.chunk_overlap, 50);
    /// ```
    fn default() -> Self {
        Self {
            // Local Qdrant instance - standard port for development
            qdrant_url: "http://localhost:6334".to_string(),
            // Default collection name for code embeddings
            qdrant_collection: "codetriever".to_string(),
            // Jina's code-optimized embedding model - good balance of quality and performance
            embedding_model: "jinaai/jina-embeddings-v2-base-code".to_string(),
            // Enable Metal acceleration on macOS, CPU elsewhere
            use_metal: cfg!(target_os = "macos"),
            // Use appropriate cache directory for each platform
            cache_dir: if cfg!(target_os = "macos") {
                PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
                    .join("Library/Caches/codetriever")
            } else {
                // Linux and others use ~/.cache or /tmp/.cache
                PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
                    .join(".cache/codetriever")
            },
            // 512 tokens provides good context while staying under most model limits
            max_chunk_size: 512,
            // ~10% overlap helps preserve context across chunk boundaries
            chunk_overlap: 50,
        }
    }
}
