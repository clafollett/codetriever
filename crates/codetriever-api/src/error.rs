//! Error handling for the Codetriever API.
//!
//! This module provides a centralized error type that represents all possible failures
//! that can occur during code retrieval and processing operations. The error handling
//! strategy follows Rust best practices by:
//!
//! - Using `thiserror` for automatic `Error` trait implementations
//! - Providing descriptive error messages with context
//! - Supporting error chaining via `#[from]` attributes
//! - Offering a convenient `Result` type alias for API operations
//! - Leveraging common error patterns from codetriever-common
//!
//! # Error Categories
//!
//! The errors are organized into logical categories:
//! - **I/O Operations**: File system and network errors
//! - **Vector Database**: Qdrant-specific errors  
//! - **AI/ML**: Embedding generation failures
//! - **Parsing**: Code parsing and analysis errors
//! - **Resource Lookup**: Missing resources or files
//!
//! # Usage
//!
//! ```rust
//! use codetriever_api::{Error, Result};
//!
//! fn process_code() -> Result<String> {
//!     // Operations that may fail
//!     Ok("processed code".to_string())
//! }
//! ```

use codetriever_common::CommonError;
use thiserror::Error;

/// The main error type for all Codetriever API operations.
///
/// This enum represents all possible errors that can occur during code retrieval,
/// processing, and storage operations. Each variant provides context-specific
/// error information and is designed for easy error propagation and handling.
///
/// # Design Principles
///
/// - **Contextual**: Each error includes descriptive context about what failed
/// - **Composable**: Errors can be chained and converted using `#[from]`
/// - **User-friendly**: Error messages are designed to be helpful for debugging
/// - **Exhaustive**: Covers all failure modes in the code retrieval pipeline
///
/// # Examples
///
/// ```rust
/// use codetriever_api::Error;
/// use std::fs;
///
/// // IO errors are automatically converted
/// let result: Result<String, Error> = fs::read_to_string("missing.txt")
///     .map_err(Error::from);
///
/// // Manual error construction
/// let parse_error = Error::Parser("Invalid syntax in function".to_string());
/// ```
#[derive(Debug, Error)]
pub enum Error {
    // Common error variants (implementing CommonError trait)
    /// I/O operation failed.
    ///
    /// This variant can be either a wrapped standard library I/O error or
    /// a custom string message for I/O-related failures.
    ///
    /// Common scenarios:
    /// - File not found or permission denied
    /// - Network timeouts or connection failures
    /// - Disk space or memory issues
    /// - Custom I/O error messages with context
    #[error("IO error: {0}")]
    Io(String),

    /// Configuration error.
    ///
    /// This variant indicates missing or invalid configuration required for
    /// the application to function properly.
    ///
    /// Common scenarios:
    /// - Missing environment variables (e.g., HF_TOKEN for Hugging Face)
    /// - Invalid API keys or credentials
    /// - Misconfigured service endpoints
    /// - Missing required configuration files
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Code parsing or analysis failed.
    ///
    /// This variant represents errors during code parsing, AST generation,
    /// or semantic analysis of source files.
    ///
    /// Common scenarios:
    /// - Unsupported programming language
    /// - Malformed or invalid syntax
    /// - Parser library errors or limitations
    /// - Unicode or encoding issues
    #[error("Parser error: {0}")]
    Parser(String),

    /// Other/generic error.
    ///
    /// This variant is used for errors that don't fit into specific categories
    /// or for wrapped anyhow errors.
    #[error("Other error: {0}")]
    Other(String),

    // API-specific error variants
    /// Vector database (Qdrant) operation failed.
    ///
    /// This variant represents errors from Qdrant vector database operations
    /// including connection issues, query failures, or data inconsistencies.
    ///
    /// Common scenarios:
    /// - Qdrant server unavailable or misconfigured
    /// - Collection doesn't exist or has wrong schema
    /// - Query timeout or invalid vector dimensions
    /// - Indexing or storage failures
    #[error("Qdrant error: {0}")]
    Qdrant(String),

    /// AI/ML embedding generation failed.
    ///
    /// This variant covers errors during code embedding generation, including
    /// model loading failures, inference errors, or dimension mismatches.
    ///
    /// Common scenarios:
    /// - Embedding model not found or corrupted
    /// - Input text too large for model context
    /// - GPU/compute resource exhaustion
    /// - Network errors when calling embedding APIs
    #[error("Embedding error: {0}")]
    Embedding(String),

    /// Requested resource was not found.
    ///
    /// This variant indicates that a requested code file, function, or other
    /// resource could not be located in the codebase or database.
    ///
    /// Common scenarios:
    /// - File path doesn't exist in repository
    /// - Function or symbol not found in codebase
    /// - Collection or index missing from database
    /// - Search query returned no results
    #[error("Not found: {0}")]
    NotFound(String),

    /// General anyhow error for flexible error handling
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

// Implement the CommonError trait for standardized error handling
impl CommonError for Error {
    fn io_error(msg: impl Into<String>) -> Self {
        Self::Io(msg.into())
    }

    fn config_error(msg: impl Into<String>) -> Self {
        Self::Configuration(msg.into())
    }

    fn parse_error(msg: impl Into<String>) -> Self {
        Self::Parser(msg.into())
    }

    fn other_error(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}

/// A specialized `Result` type for Codetriever API operations.
///
/// This type alias provides a convenient shorthand for `Result<T, Error>` that
/// is used throughout the Codetriever API. It follows the common Rust pattern
/// of providing a crate-specific Result type to reduce boilerplate.
///
/// # Usage
///
/// Instead of writing `std::result::Result<T, crate::Error>` everywhere,
/// you can simply use `Result<T>`:
///
/// ```rust
/// use codetriever_api::Result;
///
/// fn retrieve_code(path: &str) -> Result<String> {
///     // Function implementation that may return our Error type
///     Ok("code content".to_string())
/// }
///
/// fn process_multiple_files() -> Result<Vec<String>> {
///     let mut results = Vec::new();
///     results.push(retrieve_code("file1.rs")?);
///     results.push(retrieve_code("file2.rs")?);
///     Ok(results)
/// }
/// ```
///
/// The `?` operator works seamlessly with this Result type, automatically
/// propagating any `Error` variants up the call stack.
pub type Result<T> = std::result::Result<T, Error>;

// Standard From implementations using CommonError trait methods
impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::io_error(err.to_string())
    }
}
