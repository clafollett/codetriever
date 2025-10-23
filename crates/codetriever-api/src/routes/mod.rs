pub mod health;
pub mod index;
pub mod response;
pub mod search; // Now includes both /search and /context endpoints
pub mod status;

pub use response::{HasStatus, ResponseStatus};

use axum::{Router, middleware};
use std::sync::Arc;

use crate::AppState;

/// Create the main application router with all routes
///
/// # Arguments
/// * `state` - Application state containing database and vector storage clients
///
/// # Returns
/// Complete router with all API endpoints and middleware
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .merge(health::routes())
        .merge(index::routes_with_indexer(Arc::clone(
            &state.indexer_service,
        )))
        .merge(search::routes_with_search_service(Arc::clone(
            &state.search_service,
        ))) // Now includes both /search and /context
        .merge(status::routes(state))
        .merge(crate::openapi::routes()) // OpenAPI JSON endpoints
        .merge(crate::openapi::swagger_ui()) // Swagger UI
        // Add correlation ID middleware to all routes
        .layer(middleware::from_fn(
            crate::middleware::correlation_id_middleware,
        ))
}
