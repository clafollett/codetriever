//! `OpenAPI` documentation generation and Swagger UI setup

use axum::{Json, Router, response::IntoResponse, routing::get};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// `OpenAPI` documentation for Codetriever API
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::routes::search::search_handler,
        crate::routes::index::index_handler,
        // Note: status endpoint path excluded from OpenAPI due to impl Trait limitation
        // Schemas are still documented for manual API documentation
    ),
    components(
        schemas(
            // Search schemas
            crate::routes::search::SearchRequest,
            crate::routes::search::SearchResponse,
            crate::routes::search::SearchMetadata,
            crate::routes::search::Match,
            crate::routes::search::LineRange,
            crate::routes::search::Context,
            crate::routes::search::Range,
            crate::routes::search::CommitInfo,

            // Index schemas
            crate::routes::index::IndexRequest,
            crate::routes::index::IndexResponse,
            crate::routes::index::FileContent,

            // Status schemas
            crate::routes::status::StatusResponse,
            crate::routes::status::ServerInfo,
            crate::routes::status::ServiceHealth,
            crate::routes::status::IndexInfo,

            // Common schemas
            crate::routes::response::ResponseStatus,
        )
    ),
    tags(
        (name = "search", description = "Code search operations"),
        (name = "index", description = "Code indexing operations"),
        (name = "status", description = "System health and monitoring"),
    ),
    info(
        title = "Codetriever API",
        version = "0.1.0",
        description = "Semantic code search and indexing service",
        license(
            name = "MIT",
            url = "https://opensource.org/licenses/MIT"
        )
    ),
    servers(
        (url = "http://localhost:8080/api", description = "Local development server"),
        (url = "https://api.codetriever.io", description = "Production server")
    )
)]
pub struct ApiDoc;

/// Returns configured Swagger UI service
pub fn swagger_ui() -> SwaggerUi {
    SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi())
}

/// Returns `OpenAPI` JSON as a response
pub async fn openapi_json() -> impl IntoResponse {
    Json(ApiDoc::openapi())
}

/// Returns `OpenAPI` documentation routes
pub fn routes() -> Router {
    Router::new().route("/openapi.json", get(openapi_json)) // Standard OpenAPI endpoint
}
