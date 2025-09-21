//! Configuration error types

use thiserror::Error;

/// Configuration-related errors
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Invalid URL format
    #[error("Invalid URL: {url}")]
    InvalidUrl { url: String },

    /// Invalid port number
    #[error("Invalid port: {port}")]
    InvalidPort { port: u16 },

    /// Missing required field
    #[error("Missing required field: {field}")]
    MissingField { field: String },

    /// Invalid range value
    #[error("Value {value} is out of range for {field} (expected {min}-{max})")]
    OutOfRange {
        field: String,
        value: u64,
        min: u64,
        max: u64,
    },

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// TOML parsing error
    #[error("TOML parsing error: {0}")]
    TomlParsing(#[from] toml::de::Error),

    /// YAML parsing error
    #[error("YAML parsing error: {0}")]
    YamlParsing(#[from] serde_yaml::Error),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic error message
    #[error("Configuration error: {message}")]
    Generic { message: String },
}

/// Result type for configuration operations
pub type ConfigResult<T> = Result<T, ConfigError>;
