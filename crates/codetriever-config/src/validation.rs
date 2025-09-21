//! Configuration validation framework

use crate::{ConfigError, ConfigResult};
use regex::Regex;

/// URL validation regex
/// Get URL validation regex - returns None if regex compilation fails
fn get_url_regex() -> Option<&'static Regex> {
    static URL_REGEX: std::sync::OnceLock<Option<Regex>> = std::sync::OnceLock::new();
    URL_REGEX
        .get_or_init(|| Regex::new(r"^https?://[^\s/$.?#].[^\s]*$").ok())
        .as_ref()
}

/// Trait for validating configuration values
pub trait Validate {
    /// Validate this configuration object
    ///
    /// # Errors
    /// Returns validation errors if the configuration is invalid
    fn validate(&self) -> ConfigResult<()>;
}

/// Validate a URL string
///
/// # Errors
/// Returns `ConfigError::InvalidUrl` if the URL format is invalid
pub fn validate_url(url: &str, _field_name: &str) -> ConfigResult<()> {
    get_url_regex().map_or_else(
        || {
            // If regex compilation failed, do basic validation
            if url.starts_with("http://") || url.starts_with("https://") {
                Ok(())
            } else {
                Err(ConfigError::InvalidUrl {
                    url: url.to_string(),
                })
            }
        },
        |regex| {
            if regex.is_match(url) {
                Ok(())
            } else {
                Err(ConfigError::InvalidUrl {
                    url: url.to_string(),
                })
            }
        },
    )
}

/// Validate a port number
///
/// # Errors
/// Returns `ConfigError::InvalidPort` if port is 0
pub const fn validate_port(port: u16, _field_name: &str) -> ConfigResult<()> {
    if port == 0 {
        Err(ConfigError::InvalidPort { port })
    } else {
        Ok(())
    }
}

/// Validate a value is within a range
///
/// # Errors
/// Returns `ConfigError::OutOfRange` if value is outside the specified range
pub fn validate_range(value: u64, min: u64, max: u64, field_name: &str) -> ConfigResult<()> {
    if value < min || value > max {
        Err(ConfigError::OutOfRange {
            field: field_name.to_string(),
            value,
            min,
            max,
        })
    } else {
        Ok(())
    }
}

/// Validate a string is not empty
///
/// # Errors
/// Returns `ConfigError::MissingField` if the string is empty or whitespace-only
pub fn validate_non_empty(value: &str, field_name: &str) -> ConfigResult<()> {
    if value.trim().is_empty() {
        Err(ConfigError::MissingField {
            field: field_name.to_string(),
        })
    } else {
        Ok(())
    }
}
