pub mod error;
pub mod middleware;
pub mod openapi;
pub mod routes;
pub mod state;

#[cfg(test)]
pub mod test_utils;

// Export new structured error types
pub use error::{ApiError, ApiErrorResponse, ApiResult};
pub use middleware::{RequestContext, correlation_id_middleware};
pub use state::AppState;
