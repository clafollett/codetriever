pub mod health;
pub mod index;
pub mod search;

use axum::Router;

pub fn create_router() -> Router {
    Router::new()
        .merge(health::routes())
        .merge(index::routes())
        .merge(search::routes())
}
