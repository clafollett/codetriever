//! Builder pattern for Config struct
//!
//! Provides a fluent API for constructing Config instances with sensible defaults.

use super::api::Config;
use std::path::PathBuf;

/// Builder for creating Config instances with a fluent API
///
/// # Examples
///
/// ```no_run
/// use codetriever_indexer::config::ConfigBuilder;
///
/// let config = ConfigBuilder::new()
///     .qdrant_url("http://localhost:6334")
///     .collection_name("my_code")
///     .embedding_model("jinaai/jina-embeddings-v2-base-code")
///     .use_metal(true)
///     .build();
/// ```
#[derive(Debug, Clone)]
#[must_use]
pub struct ConfigBuilder {
    qdrant_url: Option<String>,
    qdrant_collection: Option<String>,
    embedding_model: Option<String>,
    use_metal: bool,
    cache_dir: Option<PathBuf>,
    max_embedding_tokens: usize,
    embedding_batch_size: usize,
    chunk_overlap_tokens: usize,
    split_large_semantic_units: bool,
}

impl ConfigBuilder {
    /// Create a new ConfigBuilder with defaults
    pub fn new() -> Self {
        Self {
            qdrant_url: None,
            qdrant_collection: None,
            embedding_model: None,
            use_metal: false,
            cache_dir: None,
            max_embedding_tokens: 8192,
            embedding_batch_size: 32,
            chunk_overlap_tokens: 100,
            split_large_semantic_units: true,
        }
    }

    /// Set the Qdrant URL
    pub fn qdrant_url(mut self, url: impl Into<String>) -> Self {
        self.qdrant_url = Some(url.into());
        self
    }

    /// Set the Qdrant collection name
    pub fn collection_name(mut self, name: impl Into<String>) -> Self {
        self.qdrant_collection = Some(name.into());
        self
    }

    /// Set the embedding model
    pub fn embedding_model(mut self, model: impl Into<String>) -> Self {
        self.embedding_model = Some(model.into());
        self
    }

    /// Enable or disable Metal GPU acceleration
    pub fn use_metal(mut self, use_metal: bool) -> Self {
        self.use_metal = use_metal;
        self
    }

    /// Set the cache directory
    pub fn cache_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(dir.into());
        self
    }

    /// Set the maximum embedding tokens
    pub fn max_embedding_tokens(mut self, tokens: usize) -> Self {
        self.max_embedding_tokens = tokens;
        self
    }

    /// Set the embedding batch size
    pub fn embedding_batch_size(mut self, size: usize) -> Self {
        self.embedding_batch_size = size;
        self
    }

    /// Set the chunk overlap tokens
    pub fn chunk_overlap_tokens(mut self, tokens: usize) -> Self {
        self.chunk_overlap_tokens = tokens;
        self
    }

    /// Set whether to split large semantic units
    pub fn split_large_semantic_units(mut self, split: bool) -> Self {
        self.split_large_semantic_units = split;
        self
    }

    /// Build the Config instance
    ///
    /// Uses defaults for any unset values:
    /// - qdrant_url: "http://localhost:6334"
    /// - qdrant_collection: "codetriever_embeddings"
    /// - embedding_model: "jinaai/jina-embeddings-v2-base-code"
    /// - cache_dir: "~/.cache/codetriever"
    pub fn build(self) -> Config {
        let cache_dir = self.cache_dir.unwrap_or_else(|| {
            dirs::cache_dir()
                .map(|d| d.join("codetriever"))
                .unwrap_or_else(|| PathBuf::from(".cache"))
        });

        Config {
            qdrant_url: self
                .qdrant_url
                .unwrap_or_else(|| "http://localhost:6334".to_string()),
            qdrant_collection: self
                .qdrant_collection
                .unwrap_or_else(|| "codetriever_embeddings".to_string()),
            embedding_model: self
                .embedding_model
                .unwrap_or_else(|| "jinaai/jina-embeddings-v2-base-code".to_string()),
            use_metal: self.use_metal,
            cache_dir,
            max_embedding_tokens: self.max_embedding_tokens,
            embedding_batch_size: self.embedding_batch_size,
            chunk_overlap_tokens: self.chunk_overlap_tokens,
            split_large_semantic_units: self.split_large_semantic_units,
        }
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder_with_defaults() {
        let config = ConfigBuilder::new().build();
        assert_eq!(config.qdrant_url, "http://localhost:6334");
        assert_eq!(config.qdrant_collection, "codetriever_embeddings");
        assert_eq!(
            config.embedding_model,
            "jinaai/jina-embeddings-v2-base-code"
        );
        assert_eq!(config.max_embedding_tokens, 8192);
    }

    #[test]
    fn test_config_builder_custom_values() {
        let config = ConfigBuilder::new()
            .qdrant_url("http://remote:6334")
            .collection_name("my_code")
            .embedding_model("custom-model")
            .use_metal(true)
            .max_embedding_tokens(4096)
            .build();

        assert_eq!(config.qdrant_url, "http://remote:6334");
        assert_eq!(config.qdrant_collection, "my_code");
        assert_eq!(config.embedding_model, "custom-model");
        assert!(config.use_metal);
        assert_eq!(config.max_embedding_tokens, 4096);
    }
}
