//! Common error handling utilities and patterns
//!
//! This module provides traits and macros to reduce duplication in error handling
//! across Codetriever crates while maintaining flexibility for crate-specific needs.

use std::fmt;

/// Common error variants that appear across multiple crates
///
/// This trait provides a standardized interface for common error types
/// while allowing crates to add their own specific variants.
pub trait CommonError: std::error::Error + Send + Sync + 'static {
    /// Create an I/O error variant
    fn io_error(msg: impl Into<String>) -> Self
    where
        Self: Sized;

    /// Create a configuration error variant
    fn config_error(msg: impl Into<String>) -> Self
    where
        Self: Sized;

    /// Create a parsing error variant
    fn parse_error(msg: impl Into<String>) -> Self
    where
        Self: Sized;

    /// Create a generic "other" error variant
    fn other_error(msg: impl Into<String>) -> Self
    where
        Self: Sized;
}

/// Trait for adding context to errors
///
/// This trait provides a consistent way to add context to errors
/// across all crates, similar to anyhow's context() but for custom error types.
pub trait ErrorContext<T> {
    /// Add context to an error
    fn context<C>(self, context: C) -> Result<T, String>
    where
        C: fmt::Display + Send + Sync + 'static;

    /// Add context with a closure (lazy evaluation)
    fn with_context<C, F>(self, f: F) -> Result<T, String>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}

impl<T, E> ErrorContext<T> for Result<T, E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    fn context<C>(self, context: C) -> Result<T, String>
    where
        C: fmt::Display + Send + Sync + 'static,
    {
        self.map_err(|e| format!("{context}: {e}"))
    }

    fn with_context<C, F>(self, f: F) -> Result<T, String>
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        self.map_err(|e| format!("{}: {}", f(), e))
    }
}

/// Macro to implement common error conversions
///
/// This macro reduces boilerplate for implementing From traits for common error types.
///
/// # Example
/// ```no_run
/// # use codetriever_common::{CommonError, impl_common_conversions};
/// # use thiserror::Error;
/// #
/// # #[derive(Debug, Error)]
/// # enum MyError {
/// #     #[error("IO error: {0}")]
/// #     Io(String),
/// #     #[error("Other error: {0}")]
/// #     Other(String),
/// # }
/// #
/// # impl CommonError for MyError {
/// #     fn io_error(msg: impl Into<String>) -> Self { Self::Io(msg.into()) }
/// #     fn config_error(msg: impl Into<String>) -> Self { Self::Other(msg.into()) }
/// #     fn parse_error(msg: impl Into<String>) -> Self { Self::Other(msg.into()) }
/// #     fn other_error(msg: impl Into<String>) -> Self { Self::Other(msg.into()) }
/// # }
/// #
/// // This macro generates From implementations automatically
/// impl_common_conversions!(MyError);
/// ```
///
/// This will generate:
/// - From<std::io::Error> -> MyError::Io
/// - From<serde_json::Error> -> MyError::Serialization (if applicable)
/// - From<anyhow::Error> -> MyError::Other
#[macro_export]
macro_rules! impl_common_conversions {
    ($error_type:ident) => {
        impl From<std::io::Error> for $error_type {
            fn from(e: std::io::Error) -> Self {
                <$error_type as $crate::CommonError>::io_error(e.to_string())
            }
        }

        impl From<anyhow::Error> for $error_type {
            fn from(e: anyhow::Error) -> Self {
                <$error_type as $crate::CommonError>::other_error(e.to_string())
            }
        }
    };

    // Variant with serde_json support
    ($error_type:ident, with_serde) => {
        impl_common_conversions!($error_type);

        impl From<serde_json::Error> for $error_type {
            fn from(e: serde_json::Error) -> Self {
                <$error_type as $crate::CommonError>::parse_error(format!("JSON: {}", e))
            }
        }
    };
}

/// Macro to define a standard error enum with common variants
///
/// This macro creates an error enum with standard variants and automatically
/// implements the CommonError trait.
///
/// # Example
/// ```no_run
/// # use codetriever_common::define_error_enum;
/// define_error_enum! {
///     pub enum ApiError {
///         // Common variants (automatically included):
///         // Io, Configuration, Parse, Other
///         
///         // Custom variants:
///         #[error("Database error: {0}")]
///         Database(String),
///         
///         #[error("Not found: {0}")]
///         NotFound(String),
///     }
/// }
///
/// // The macro automatically creates:
/// // - ApiError enum with all variants
/// // - CommonError trait implementation
/// // - Result<T> type alias
/// ```
#[macro_export]
macro_rules! define_error_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident($variant_type:ty),
            )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, thiserror::Error)]
        pub enum $name {
            #[error("IO error: {0}")]
            Io(String),

            #[error("Configuration error: {0}")]
            Configuration(String),

            #[error("Parse error: {0}")]
            Parse(String),

            #[error("Other error: {0}")]
            Other(String),

            $(
                $(#[$variant_meta])*
                $variant($variant_type),
            )*
        }

        impl $crate::CommonError for $name {
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

        /// Specialized Result type
        pub type Result<T> = std::result::Result<T, $name>;
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use thiserror::Error;

    // Test error type
    #[derive(Debug, Error)]
    enum TestError {
        #[error("IO error: {0}")]
        Io(String),
        #[error("Configuration error: {0}")]
        Configuration(String),
        #[error("Parse error: {0}")]
        Parse(String),
        #[error("Other error: {0}")]
        Other(String),
    }

    impl CommonError for TestError {
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

    #[test]
    fn test_common_error_trait() {
        let io_err = TestError::io_error("file not found");
        assert_eq!(io_err.to_string(), "IO error: file not found");

        let config_err = TestError::config_error("missing API key");
        assert_eq!(
            config_err.to_string(),
            "Configuration error: missing API key"
        );
    }

    #[test]
    fn test_error_context() {
        let result: Result<(), TestError> = Err(TestError::io_error("original error"));
        let with_context = result.context("while reading file");
        assert!(with_context.is_err());
        assert!(with_context.unwrap_err().contains("while reading file"));
    }
}
