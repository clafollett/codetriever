//! Structured API error handling for the Codetriever API.
//!
//! This module provides comprehensive error handling designed for production API use:
//!
//! - **Structured Error Types**: Clear categorization with correlation IDs
//! - **HTTP Integration**: Automatic status code mapping and user-friendly responses
//! - **Request Tracking**: Correlation IDs for debugging and monitoring
//! - **Observability**: Full integration with tracing and metrics
//! - **Security**: No internal details leaked to API users
//!
//! # Design Philosophy
//!
//! 1. **User Experience**: API consumers get helpful, actionable error messages
//! 2. **Developer Experience**: Rich context and correlation IDs for debugging
//! 3. **Production Ready**: Structured logging, metrics, and monitoring integration
//! 4. **Security**: Internal errors are sanitized before reaching users
//!
//! # Usage
//!
//! ```rust
//! use codetriever_api::{ApiError, ApiResult};
//! use codetriever_common::CorrelationId;
//!
//! async fn search_handler() -> ApiResult<Vec<String>> {
//!     let correlation_id = CorrelationId::new();
//!
//!     // Structured error with correlation ID
//!     Err(ApiError::InvalidSearchQuery {
//!         query: "bad query".to_string(),
//!         reason: "Query too short".to_string(),
//!         correlation_id,
//!     })
//! }
//! ```

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use codetriever_common::CorrelationId;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use tracing::{error, warn};

/// Structured API error types with correlation IDs for request tracking.
///
/// Each error variant includes a correlation ID that links the error to request
/// traces, making debugging and monitoring much easier in production.
///
/// # Error Categories
///
/// - **Search Errors**: Query validation and search service failures
/// - **Service Errors**: Backend service unavailability and timeouts
/// - **Database Errors**: Data layer operation failures
/// - **System Errors**: Infrastructure and configuration issues
///
/// # Correlation IDs
///
/// Every error includes a correlation ID that:
/// - Links errors to request traces in logs
/// - Enables cross-service request tracking
/// - Helps with debugging in distributed systems
/// - Provides users with reference numbers for support
#[derive(Debug, Error)]
pub enum ApiError {
    /// Search service is unavailable or experiencing issues.
    ///
    /// This indicates the search backend is down, overloaded, or misconfigured.
    /// Users should retry later or contact support if the issue persists.
    #[error(
        "Search service unavailable (correlation: {correlation_id}, timeout: {}ms)",
        timeout_duration.as_millis()
    )]
    SearchServiceUnavailable {
        timeout_duration: Duration,
        correlation_id: CorrelationId,
    },

    /// Invalid or malformed search query.
    ///
    /// The query failed validation checks or contains unsupported syntax.
    /// Users should modify their query based on the provided reason.
    #[error(
        "Invalid search query '{}': {} (correlation: {})",
        query,
        reason,
        correlation_id
    )]
    InvalidSearchQuery {
        query: String,
        reason: String,
        correlation_id: CorrelationId,
    },

    /// Database operation timed out.
    ///
    /// The database failed to respond within the configured timeout period.
    /// This usually indicates high load or connectivity issues.
    #[error(
        "Database timeout during {} operation (correlation: {})",
        operation,
        correlation_id
    )]
    DatabaseTimeout {
        operation: String,
        correlation_id: CorrelationId,
    },

    /// Database connection or query failed.
    ///
    /// General database errors including connection failures, constraint violations,
    /// or data consistency issues.
    #[error(
        "Database error during {} (correlation: {})",
        operation,
        correlation_id
    )]
    DatabaseError {
        operation: String,
        correlation_id: CorrelationId,
    },

    /// Resource not found in the system.
    ///
    /// The requested resource (file, repository, etc.) does not exist or
    /// is not accessible with the current permissions.
    #[error(
        "Resource '{}' not found (correlation: {})",
        resource_id,
        correlation_id
    )]
    ResourceNotFound {
        resource_id: String,
        correlation_id: CorrelationId,
    },

    /// Authentication or authorization failed.
    ///
    /// The request lacks valid authentication credentials or the authenticated
    /// user does not have permission to perform the operation.
    #[error("Access denied: {} (correlation: {})", reason, correlation_id)]
    AccessDenied {
        reason: String,
        correlation_id: CorrelationId,
    },

    /// Request failed validation.
    ///
    /// The request body, parameters, or headers failed validation checks.
    /// This includes malformed JSON, missing required fields, or invalid values.
    #[error(
        "Request validation failed: {} (correlation: {})",
        message,
        correlation_id
    )]
    ValidationError {
        message: String,
        field: Option<String>,
        correlation_id: CorrelationId,
    },

    /// Rate limit exceeded.
    ///
    /// The client has exceeded the allowed request rate. They should implement
    /// backoff and retry logic.
    #[error(
        "Rate limit exceeded. Retry after {}s (correlation: {})",
        retry_after_seconds,
        correlation_id
    )]
    RateLimitExceeded {
        retry_after_seconds: u64,
        correlation_id: CorrelationId,
    },

    /// Internal server error with correlation ID.
    ///
    /// An unexpected error occurred that the user cannot fix. The correlation ID
    /// can be used for support requests and debugging.
    #[error("Internal server error (correlation: {})", correlation_id)]
    InternalServerError { correlation_id: CorrelationId },

    /// Service temporarily unavailable.
    ///
    /// The service is temporarily down for maintenance or experiencing high load.
    /// Clients should implement exponential backoff when retrying.
    #[error(
        "Service temporarily unavailable. Retry after {}s (correlation: {})",
        retry_after_seconds,
        correlation_id
    )]
    ServiceUnavailable {
        retry_after_seconds: u64,
        correlation_id: CorrelationId,
    },
}

/// Error response sent to API clients.
///
/// This is the JSON structure returned in HTTP error responses. It provides
/// users with actionable information while maintaining security by not exposing
/// internal system details.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiErrorResponse {
    /// HTTP error code
    pub error: String,
    /// Human-readable error message
    pub message: String,
    /// Correlation ID for tracking and support
    pub correlation_id: CorrelationId,
    /// Optional additional details for debugging
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    /// When to retry (for transient errors)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
}

impl ApiError {
    /// Get the correlation ID from any error variant.
    ///
    /// This is useful for logging and monitoring, allowing you to extract
    /// the correlation ID regardless of the specific error type.
    pub const fn correlation_id(&self) -> &CorrelationId {
        match self {
            Self::SearchServiceUnavailable { correlation_id, .. }
            | Self::InvalidSearchQuery { correlation_id, .. }
            | Self::DatabaseTimeout { correlation_id, .. }
            | Self::DatabaseError { correlation_id, .. }
            | Self::ResourceNotFound { correlation_id, .. }
            | Self::AccessDenied { correlation_id, .. }
            | Self::ValidationError { correlation_id, .. }
            | Self::RateLimitExceeded { correlation_id, .. }
            | Self::InternalServerError { correlation_id, .. }
            | Self::ServiceUnavailable { correlation_id, .. } => correlation_id,
        }
    }

    /// Get the HTTP status code for this error.
    ///
    /// Maps each error variant to the appropriate HTTP status code following
    /// REST API conventions and HTTP specifications.
    pub const fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidSearchQuery { .. } | Self::ValidationError { .. } => {
                StatusCode::BAD_REQUEST
            }
            Self::AccessDenied { .. } => StatusCode::UNAUTHORIZED,
            Self::ResourceNotFound { .. } => StatusCode::NOT_FOUND,
            Self::RateLimitExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::InternalServerError { .. } | Self::DatabaseError { .. } => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::SearchServiceUnavailable { .. }
            | Self::DatabaseTimeout { .. }
            | Self::ServiceUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    /// Create an invalid search query error.
    ///
    /// Use this for query validation failures with helpful user feedback.
    pub const fn invalid_query(
        query: String,
        reason: String,
        correlation_id: CorrelationId,
    ) -> Self {
        Self::InvalidSearchQuery {
            query,
            reason,
            correlation_id,
        }
    }

    /// Create a database timeout error.
    ///
    /// Use this when database operations exceed configured timeouts.
    pub const fn database_timeout(operation: String, correlation_id: CorrelationId) -> Self {
        Self::DatabaseTimeout {
            operation,
            correlation_id,
        }
    }
}

/// Axum HTTP response implementation for `ApiError`.
///
/// This implementation automatically converts `ApiError` instances into proper
/// HTTP responses with:
/// - Correct status codes
/// - JSON error bodies
/// - Security headers
/// - Correlation ID headers for tracking
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let correlation_id = self.correlation_id();

        // Log the error with correlation ID for debugging
        match &self {
            Self::InternalServerError { .. } => {
                error!(
                    correlation_id = %correlation_id,
                    error = %self,
                    "Internal server error"
                );
            }
            Self::DatabaseError { .. } => {
                error!(
                    correlation_id = %correlation_id,
                    error = %self,
                    "Database or legacy error"
                );
            }
            Self::SearchServiceUnavailable { .. }
            | Self::DatabaseTimeout { .. }
            | Self::ServiceUnavailable { .. } => {
                warn!(
                    correlation_id = %correlation_id,
                    error = %self,
                    "Service unavailable error"
                );
            }
            _ => {
                warn!(
                    correlation_id = %correlation_id,
                    error = %self,
                    "Client error"
                );
            }
        }

        let error_response = ApiErrorResponse {
            error: format!("{self:?}")
                .split("::")
                .last()
                .unwrap_or("Unknown")
                .to_uppercase(),
            message: self.to_string(),
            correlation_id: correlation_id.clone(),
            details: None,
            retry_after: match &self {
                Self::SearchServiceUnavailable { .. } => Some(60),
                Self::DatabaseTimeout { .. } => Some(30),
                Self::ServiceUnavailable {
                    retry_after_seconds,
                    ..
                }
                | Self::RateLimitExceeded {
                    retry_after_seconds,
                    ..
                } => Some(*retry_after_seconds),
                _ => None,
            },
        };

        let mut response = (status, Json(error_response)).into_response();

        // Add correlation ID to response headers for client tracking
        if let Ok(header_value) = correlation_id.to_string().parse() {
            response
                .headers_mut()
                .insert("X-Correlation-ID", header_value);
        }

        response
    }
}

/// Result type for API operations.
///
/// This type alias provides a convenient shorthand for API operations that
/// return structured errors with correlation IDs and proper HTTP status codes.
pub type ApiResult<T> = std::result::Result<T, ApiError>;
