//! API Configuration Module
//!
//! This module provides configuration structs and defaults for the Codetriever API server.
//! The configuration manages various aspects of the system including vector database connections,
//! embedding models, chunking strategies, and platform-specific optimizations.
//!
//! # Examples
//!
//! ```
//! use codetriever_indexing::config::api::Config;
//!
//! // Use default configuration
//! let config = Config::default();
//!
//! // Create custom configuration
//! let config = Config {
//!     qdrant_url: "http://my-qdrant:6334".to_string(),
//!     embedding_model: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
//!     max_embedding_tokens: 1024,
//!     chunk_overlap_tokens: 256,
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
/// use codetriever_indexing::config::api::Config;
/// use std::path::PathBuf;
///
/// let config = Config {
///     qdrant_url: "http://localhost:6334".to_string(),
///     qdrant_collection: "my_code_collection".to_string(),
///     embedding_model: "jinaai/jina-embeddings-v2-base-code".to_string(),
///     use_metal: true,
///     cache_dir: PathBuf::from("/tmp/codetriever"),
///     max_embedding_tokens: 1024,
///     chunk_overlap_tokens: 256,
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

    /// Maximum number of tokens allowed per embedding input
    ///
    /// This must match the model's maximum context length. For Jina v2 models,
    /// this is 8192 tokens. Chunks larger than this will be split or truncated.
    /// Note: Higher values increase memory usage significantly, especially with
    /// larger batch sizes. Consider reducing if experiencing memory issues.
    /// TODO: Upgrade to jina-embeddings-v4 or jina-code-embeddings-1.5b for massive improvements:
    ///       - 32,768 token context (4x larger than current V2's 8192!)
    ///       - 2048-dimensional embeddings (vs V2's 768) for richer representations
    ///       - Matryoshka dimensions: 128, 256, 512, 1024, 2048 (flexibility!)
    ///       - FlashAttention2 for faster processing
    ///       - Supports code as first-class task type
    ///       - Still runs locally - no API costs or data privacy concerns!
    ///       - Would eliminate chunking for 99% of source files
    ///
    /// Alternative: voyage-code-3 (32K context; 256, 512, 1024 (default), 2048 dims; but requires API)
    pub max_embedding_tokens: usize,

    /// Number of overlapping tokens for fallback chunking
    ///
    /// Only used when tree-sitter parsing fails and we fall back to naive chunking.
    /// Helps preserve context across chunk boundaries.
    /// Typical value: 256 tokens (provides good context continuity)
    pub chunk_overlap_tokens: usize,

    /// Whether to split large semantic units that exceed token limits
    ///
    /// When true, functions/classes larger than MAX_CHUNK_TOKENS will be split
    /// at logical boundaries (e.g., method boundaries for classes).
    /// When false, they will be truncated with a warning.
    pub split_large_semantic_units: bool,

    /// Batch size for embedding generation
    ///
    /// Controls how many chunks are processed at once during embedding generation.
    /// Lower values reduce memory usage but may be slower. Higher values use more
    /// memory but can be faster due to better GPU utilization.
    /// Recommended: 1-2 for Metal on MacBooks (to avoid memory pressure),
    /// 8-16 for CUDA GPUs, 1 for CPU.
    pub embedding_batch_size: usize,
}

impl Default for Config {
    /// Creates a new Config with sensible defaults for local development
    ///
    /// # Default Values
    ///
    /// * `qdrant_url`: "http://localhost:6334" - Local Qdrant gRPC port
    /// * `qdrant_collection`: "codetriever" - Default collection name
    /// * `embedding_model`: "jinaai/jina-embeddings-v2-base-code" - Code-optimized model
    /// * `use_metal`: Enabled on macOS, disabled elsewhere
    /// * `cache_dir`: System cache directory + "codetriever" (falls back to ".cache/codetriever")
    /// * `max_embedding_tokens`: 1024 tokens - Context preservation for naive chunking
    /// * `chunk_overlap_tokens`: 256 tokens - Context preservation for naive chunking
    /// * `split_large_semantic_units`: true - Split large functions/classes at logical boundaries
    ///
    /// # Examples
    ///
    /// ```
    /// use codetriever_indexing::config::api::Config;
    ///
    /// let config = Config::default();
    /// assert_eq!(config.qdrant_url, "http://localhost:6334");
    /// assert_eq!(config.chunk_overlap_tokens, 512);
    /// assert!(config.split_large_semantic_units);
    /// ```
    fn default() -> Self {
        Self {
            // Local Qdrant instance - gRPC port (Rust client uses gRPC)
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
            // Conservative token limit to avoid memory issues with Metal
            // Can be increased to 8192 if you have sufficient RAM
            max_embedding_tokens: 4096,
            // 256 tokens overlap for fallback chunking provides good context
            chunk_overlap_tokens: 512,
            // Split large functions/classes at logical boundaries
            split_large_semantic_units: true,
            // Conservative batch size to avoid memory pressure on MacBooks
            embedding_batch_size: 1,
        }
    }
}
