use axum::{Json, Router, routing::get};
use serde_json::json;

pub fn routes() -> Router {
    Router::new().route("/health", get(health_check))
}

async fn health_check() -> Json<serde_json::Value> {
    Json(json!({
        "status": "healthy",
        "service": "codetriever-api"
    }))
}
