pub mod error;
pub mod middleware;
pub mod openapi;
pub mod routes;

#[cfg(test)]
pub mod test_utils;

// Export new structured error types
pub use error::{ApiError, ApiResult, ErrorResponse, generate_correlation_id};
pub use middleware::{RequestContext, correlation_id_middleware};

// Legacy exports for backward compatibility
pub use error::{Error, Result};
