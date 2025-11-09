use crate::middleware::RequestContext;
use axum::{Extension, Json, Router, routing::get};
use codetriever_common::CorrelationId;
use serde_json::json;
use tracing::{info, instrument};

pub fn routes() -> Router {
    Router::new().route("/health", get(health_check))
}

/// Health check endpoint with correlation ID tracking
///
/// # Errors
///
/// Currently cannot fail (always returns healthy)
/// TODO: Add real service connectivity checks
#[instrument(fields(correlation_id))]
async fn health_check(context: Option<Extension<RequestContext>>) -> Json<serde_json::Value> {
    // Extract correlation ID
    let correlation_id = context
        .as_ref()
        .map_or_else(CorrelationId::new, |ctx| ctx.correlation_id.clone());

    tracing::Span::current().record("correlation_id", correlation_id.to_string());

    info!(
        correlation_id = %correlation_id,
        "Health check request"
    );

    Json(json!({
        "status": "healthy",
        "service": "codetriever-api",
        "correlation_id": correlation_id.to_string()
    }))
}
