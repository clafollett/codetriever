pub mod health;
pub mod index;
pub mod response;
pub mod search;

pub use response::{HasStatus, ResponseStatus};

use axum::Router;

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
}
