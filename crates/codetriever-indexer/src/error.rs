//! Error types for the indexer crate
//!
//! This module uses the common error patterns from codetriever-common
//! to reduce duplication while maintaining crate-specific error variants.

use codetriever_common::CommonError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    // Common error variants
    #[error("IO error: {0}")]
    Io(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Parsing error: {0}")]
    Parse(String),

    #[error("Other error: {0}")]
    Other(String),

    // Crate-specific variants
    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// Implement the CommonError trait
impl CommonError for Error {
    fn io_error(msg: impl Into<String>) -> Self {
        Self::Io(msg.into())
    }

    fn config_error(msg: impl Into<String>) -> Self {
        Self::Configuration(msg.into())
    }

    fn parse_error(msg: impl Into<String>) -> Self {
        Self::Parse(msg.into())
    }

    fn other_error(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}

// Use the macro for common conversions
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::io_error(e.to_string())
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::other_error(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
