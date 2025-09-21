pub mod health;
pub mod index;
pub mod response;
pub mod search;

pub use response::{HasStatus, ResponseStatus};

use axum::{Router, middleware};

pub fn create_router() -> Router {
    Router::new()
        .merge(health::routes())
        .merge(index::routes())
        .merge(search::routes())
        .merge(crate::openapi::swagger_ui())
        .route(
            "/api-docs/openapi.json",
            axum::routing::get(crate::openapi::openapi_json),
        )
        // Add correlation ID middleware to all routes
        .layer(middleware::from_fn(
            crate::middleware::correlation_id_middleware,
        ))
}
