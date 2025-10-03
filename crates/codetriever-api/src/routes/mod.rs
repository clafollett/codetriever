pub mod health;
pub mod index;
pub mod response;
pub mod search;
pub mod status;

pub use response::{HasStatus, ResponseStatus};

use axum::{Router, middleware};

pub fn create_router() -> Router {
    Router::new()
        .merge(health::routes())
        .merge(index::routes())
        .merge(search::routes())
        .merge(status::routes())
        .merge(crate::openapi::routes()) // OpenAPI JSON endpoints
        .merge(crate::openapi::swagger_ui()) // Swagger UI
        // Add correlation ID middleware to all routes
        .layer(middleware::from_fn(
            crate::middleware::correlation_id_middleware,
        ))
}
