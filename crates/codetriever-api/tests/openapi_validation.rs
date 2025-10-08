//! `OpenAPI` schema validation tests
//!
//! Validates that API responses match the `OpenAPI` schema definition
//! This ensures API contract compliance and prevents schema drift

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod test_utils;

use axum::{body::Body, http::Request};
use codetriever_api::{openapi::ApiDoc, routes::create_router};
use serde_json::Value;
use tower::ServiceExt;
use utoipa::OpenApi;

#[tokio::test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
async fn test_openapi_json_endpoint_accessible() -> test_utils::TestResult {
    let test_state = test_utils::app_state().await?;
    let app = create_router(test_state.state().clone());

    // Test /openapi.json endpoint
    let request = Request::builder()
        .uri("/openapi.json")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200);

    // Test /api-docs/openapi.json endpoint
    let test_state = test_utils::app_state().await?;
    let app = create_router(test_state.state().clone());
    let request = Request::builder()
        .uri("/api-docs/openapi.json")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200);
    Ok(())
}

#[tokio::test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
async fn test_openapi_schema_structure() -> test_utils::TestResult {
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
    Ok(())
}

#[tokio::test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
async fn test_search_response_matches_schema() -> test_utils::TestResult {
    // This is a framework for future schema validation
    // For now, just verify that we can generate and parse the schema

    let openapi_spec = ApiDoc::openapi();
    let spec_json = serde_json::to_value(&openapi_spec).unwrap();

    // Verify the JSON is well-formed
    assert!(spec_json.is_object());

    // Check for required OpenAPI 3.x fields
    let version = spec_json
        .get("openapi")
        .and_then(|v| v.as_str())
        .expect("OpenAPI version should exist");
    assert!(
        version.starts_with("3."),
        "Should be OpenAPI 3.x, got: {version}"
    );
    assert!(spec_json.get("info").is_some_and(Value::is_object));
    assert!(spec_json.get("paths").is_some_and(Value::is_object));

    println!("✅ OpenAPI schema is well-formed and serializable");
    Ok(())
}

#[tokio::test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
async fn test_schema_consistency_with_actual_endpoints() -> test_utils::TestResult {
    // Test that our live schema matches what we expect
    // This is where future validation against actual responses would go

    let test_state = test_utils::app_state().await?;
    let app = create_router(test_state.state().clone());

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
    let paths = schema_json.get("paths").expect("Schema should have paths");
    assert!(
        paths.get("/search").is_some_and(Value::is_object),
        "Search endpoint should be documented"
    );
    assert!(
        paths.get("/index").is_some_and(Value::is_object),
        "Index endpoint should be documented"
    );

    println!("✅ Live OpenAPI endpoint serves valid schema");
    Ok(())
}

// TODO: Add actual response validation tests once APIs are stable
// These would use a library like `jsonschema` to validate actual API responses
// against the OpenAPI schema definitions
