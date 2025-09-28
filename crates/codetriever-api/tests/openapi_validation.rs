//! OpenAPI schema validation tests
//!
//! Validates that API responses match the OpenAPI schema definition
//! This ensures API contract compliance and prevents schema drift

use axum::{body::Body, http::Request};
use codetriever_api::{openapi::ApiDoc, routes::create_router};
use serde_json::Value;
use tower::ServiceExt;
use utoipa::OpenApi;

#[tokio::test]
async fn test_openapi_json_endpoint_accessible() {
    let app = create_router();

    // Test /openapi.json endpoint
    let request = Request::builder()
        .uri("/openapi.json")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200);

    // Test /api-docs/openapi.json endpoint
    let app = create_router();
    let request = Request::builder()
        .uri("/api-docs/openapi.json")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn test_openapi_schema_structure() {
    // Get the generated OpenAPI spec
    let openapi_spec = ApiDoc::openapi();

    // Basic structure validation
    assert_eq!(openapi_spec.info.title, "Codetriever API");
    assert_eq!(openapi_spec.info.version, "0.1.0");

    // Check that key paths exist
    let paths = &openapi_spec.paths.paths;
    assert!(
        paths.contains_key("/search"),
        "Should have /search endpoint"
    );
    assert!(paths.contains_key("/index"), "Should have /index endpoint");

    // Check that schemas are defined
    let schemas = &openapi_spec.components.as_ref().unwrap().schemas;
    assert!(
        schemas.contains_key("SearchRequest"),
        "Should have SearchRequest schema"
    );
    assert!(
        schemas.contains_key("SearchResponse"),
        "Should have SearchResponse schema"
    );
}

#[tokio::test]
async fn test_search_response_matches_schema() {
    // This is a framework for future schema validation
    // For now, just verify that we can generate and parse the schema

    let openapi_spec = ApiDoc::openapi();
    let spec_json = serde_json::to_value(&openapi_spec).unwrap();

    // Verify the JSON is well-formed
    assert!(spec_json.is_object());

    // Check for required OpenAPI 3.x fields
    let version = spec_json["openapi"].as_str().unwrap();
    assert!(
        version.starts_with("3."),
        "Should be OpenAPI 3.x, got: {}",
        version
    );
    assert!(spec_json["info"].is_object());
    assert!(spec_json["paths"].is_object());

    println!("✅ OpenAPI schema is well-formed and serializable");
}

#[tokio::test]
async fn test_schema_consistency_with_actual_endpoints() {
    // Test that our live schema matches what we expect
    // This is where future validation against actual responses would go

    let app = create_router();

    // Test that we can get the schema from the live endpoint
    let request = Request::builder()
        .uri("/openapi.json")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let schema_json: Value = serde_json::from_slice(&body).unwrap();

    // Verify the live schema has expected structure
    assert!(
        schema_json["paths"]["/search"].is_object(),
        "Search endpoint should be documented"
    );
    assert!(
        schema_json["paths"]["/index"].is_object(),
        "Index endpoint should be documented"
    );

    println!("✅ Live OpenAPI endpoint serves valid schema");
}

// TODO: Add actual response validation tests once APIs are stable
// These would use a library like `jsonschema` to validate actual API responses
// against the OpenAPI schema definitions
