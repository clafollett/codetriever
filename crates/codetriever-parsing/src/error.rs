//! Error types for the parsing crate
//!
//! Defines parsing-specific errors and result types for code parsing and chunking operations.

use thiserror::Error;

/// Parsing-specific error types
#[derive(Error, Debug)]
pub enum ParsingError {
    /// Tree-sitter parsing error
    #[error("Tree-sitter parsing error: {0}")]
    TreeSitterError(String),

    /// Code chunking error
    #[error("Chunking error: {0}")]
    ChunkingError(String),

    /// Unsupported language error
    #[error("Language not supported: {0}")]
    LanguageUnsupported(String),

    /// Token counting error
    #[error("Token counting error: {0}")]
    TokenCountingError(String),

    /// Query compilation error
    #[error("Query compilation error: {0}")]
    QueryCompilationError(String),

    /// Cache error (query cache, language registry, etc.)
    #[error("Cache error: {0}")]
    CacheError(String),

    /// IO error wrapper
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// Anyhow error wrapper
    #[error("Generic error: {0}")]
    AnyhowError(#[from] anyhow::Error),

    /// Generic parsing error
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Other error (fallback)
    #[error("Other error: {0}")]
    Other(String),
}

impl ParsingError {
    /// Create a parse error
    pub fn parse_error(msg: String) -> Self {
        Self::ParseError(msg)
    }

    /// Create a tree-sitter error
    pub fn tree_sitter_error(msg: String) -> Self {
        Self::TreeSitterError(msg)
    }

    /// Create a chunking error
    pub fn chunking_error(msg: String) -> Self {
        Self::ChunkingError(msg)
    }

    /// Create a token counting error
    pub fn token_counting_error(msg: String) -> Self {
        Self::TokenCountingError(msg)
    }

    /// Create a cache error
    pub fn cache_error(msg: String) -> Self {
        Self::CacheError(msg)
    }
}

/// Result type alias for parsing operations
pub type ParsingResult<T> = Result<T, ParsingError>;
