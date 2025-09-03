//! Error types for the indexer crate

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(String),

    #[error("Parsing error: {0}")]
    Parse(String),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Qdrant error: {0}")]
    Qdrant(Box<qdrant_client::QdrantError>),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Other error: {0}")]
    Other(String),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Other(e.to_string())
    }
}

impl From<qdrant_client::QdrantError> for Error {
    fn from(e: qdrant_client::QdrantError) -> Self {
        Error::Qdrant(Box::new(e))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
