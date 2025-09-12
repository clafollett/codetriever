//! Error sanitization utilities for security
//!
//! Provides utilities to sanitize error messages before returning them to users,
//! preventing information disclosure while maintaining debugging capabilities.

use tracing::error;

/// Sanitize an error message for external consumption
///
/// Logs the detailed error internally and returns a generic message
pub fn sanitize_error<E: std::fmt::Display>(error: E, context: &str) -> String {
    // Log the detailed error internally with correlation ID
    let correlation_id = uuid::Uuid::new_v4();
    error!(
        correlation_id = %correlation_id,
        error = %error,
        context = %context,
        "Internal error occurred"
    );

    // Return generic message with correlation ID for debugging
    format!("Operation failed (ref: {correlation_id})")
}

/// Sanitize an error with a user-friendly message
///
/// Logs the detailed error internally and returns a safe user message
pub fn sanitize_with_message<E: std::fmt::Display>(
    error: E,
    context: &str,
    user_message: &str,
) -> String {
    // Log the detailed error internally
    let correlation_id = uuid::Uuid::new_v4();
    error!(
        correlation_id = %correlation_id,
        error = %error,
        context = %context,
        "Internal error occurred"
    );

    // Return user-friendly message with correlation ID
    format!("{user_message} (ref: {correlation_id})")
}

/// Create a sanitized error result
#[macro_export]
macro_rules! sanitized_error {
    ($error:expr, $context:expr) => {
        Err($crate::error_sanitizer::sanitize_error($error, $context))
    };
    ($error:expr, $context:expr, $message:expr) => {
        Err($crate::error_sanitizer::sanitize_with_message(
            $error, $context, $message,
        ))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_error() {
        let error = "Database connection failed: password incorrect";
        let result = sanitize_error(error, "database_connection");
        assert!(result.starts_with("Operation failed (ref: "));
        assert!(!result.contains("password"));
    }

    #[test]
    fn test_sanitize_with_message() {
        let error = "File not found: /secret/path/to/file.txt";
        let result =
            sanitize_with_message(error, "file_read", "Unable to process the requested file");
        assert!(result.starts_with("Unable to process the requested file (ref: "));
        assert!(!result.contains("/secret/path"));
    }
}
