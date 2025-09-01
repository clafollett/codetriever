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
//!     fallback_chunk_overlap_tokens: 512,
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
///     fallback_chunk_overlap_tokens: 256,
///     split_large_semantic_units: true,
///     ..Default::default()
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

    /// Number of overlapping tokens for fallback chunking
    ///
    /// Only used when tree-sitter parsing fails and we fall back to naive chunking.
    /// Helps preserve context across chunk boundaries.
    /// Typical value: 256 tokens (provides good context continuity)
    pub fallback_chunk_overlap_tokens: usize,

    /// Whether to split large semantic units that exceed token limits
    ///
    /// When true, functions/classes larger than MAX_CHUNK_TOKENS will be split
    /// at logical boundaries (e.g., method boundaries for classes).
    /// When false, they will be truncated with a warning.
    pub split_large_semantic_units: bool,
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
    /// * `fallback_chunk_overlap_tokens`: 512 tokens - Context preservation for naive chunking
    /// * `split_large_semantic_units`: true - Split large functions/classes at logical boundaries
    ///
    /// # Examples
    ///
    /// ```
    /// use codetriever_api::config::api::Config;
    ///
    /// let config = Config::default();
    /// assert_eq!(config.qdrant_url, "http://localhost:6334");
    /// assert_eq!(config.fallback_chunk_overlap_tokens, 512);
    /// assert!(config.split_large_semantic_units);
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
                    .join(".codetriever")
            },
            // 512 tokens overlap for fallback chunking provides good context
            fallback_chunk_overlap_tokens: 512,
            // Split large functions/classes at logical boundaries
            split_large_semantic_units: true,
        }
    }
}
