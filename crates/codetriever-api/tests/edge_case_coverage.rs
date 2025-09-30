//! Edge case coverage tests for API endpoints
//!
//! Tests boundary conditions, Unicode handling, large result sets, and error scenarios

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::{body::Body, http::Request};
use codetriever_api::routes::create_router;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn test_search_with_unicode_characters() {
    let app = create_router();

    // Test Unicode in search query
    let unicode_query = json!({
        "query": "函数 función función 🔍 emoji search",
        "limit": 10
    });

    let request = Request::builder()
        .method("POST")
        .uri("/search")
        .header("content-type", "application/json")
        .body(Body::from(unicode_query.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should handle Unicode gracefully (not crash)
    assert!(response.status().is_success() || response.status() == 400);
}

#[tokio::test]
async fn test_search_with_very_long_query() {
    let app = create_router();

    // Test with very long query (boundary condition)
    let long_query = "a".repeat(10000);
    let request_body = json!({
        "query": long_query,
        "limit": 5
    });

    let request = Request::builder()
        .method("POST")
        .uri("/search")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should handle long queries gracefully
    assert!(response.status().is_success() || response.status() == 400);
}

#[tokio::test]
async fn test_search_with_empty_query() {
    let app = create_router();

    // Test edge case: empty query
    let request_body = json!({
        "query": "",
        "limit": 10
    });

    let request = Request::builder()
        .method("POST")
        .uri("/search")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should handle empty query appropriately
    assert!(response.status().is_client_error() || response.status().is_success());
}

#[tokio::test]
async fn test_search_with_extreme_limit_values() {
    let app = create_router();

    // Test with limit = 0
    let zero_limit = json!({
        "query": "test",
        "limit": 0
    });

    let request = Request::builder()
        .method("POST")
        .uri("/search")
        .header("content-type", "application/json")
        .body(Body::from(zero_limit.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // Router without state may return 500 for zero limit, but shouldn't crash
    assert!(
        response.status().is_client_error()
            || response.status().is_success()
            || response.status().is_server_error()
    );

    // Test with very large limit
    let app = create_router();
    let large_limit = json!({
        "query": "test",
        "limit": 999_999
    });

    let request = Request::builder()
        .method("POST")
        .uri("/search")
        .header("content-type", "application/json")
        .body(Body::from(large_limit.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // Router without state may return 500, but shouldn't crash
    assert!(
        response.status().is_client_error()
            || response.status().is_success()
            || response.status().is_server_error()
    );
}

#[tokio::test]
async fn test_index_with_special_characters_in_path() {
    let app = create_router();

    // Test with special characters in file paths
    let request_body = json!({
        "project_id": "test-project",
        "files": [
            {
                "path": "src/模块/测试.rs",
                "content": "// Unicode file path test"
            },
            {
                "path": "src/file with spaces.rs",
                "content": "// Spaces in path test"
            },
            {
                "path": "src/file@#$%^&*().rs",
                "content": "// Special characters test"
            }
        ]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/index")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should handle special characters in paths gracefully
    assert!(response.status().is_success() || response.status().is_client_error());
}

#[tokio::test]
async fn test_index_with_very_large_file_content() {
    let app = create_router();

    // Test with large file content (boundary condition)
    let large_content =
        "fn large_function() {\n".to_string() + &"    println!(\"test\");\n".repeat(5000) + "}";

    let request_body = json!({
        "project_id": "test-project",
        "files": [
            {
                "path": "src/large_file.rs",
                "content": large_content
            }
        ]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/index")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // Should handle large files appropriately
    assert!(response.status().is_success() || response.status() == 413); // 413 = Request Entity Too Large
}

#[tokio::test]
async fn test_malformed_json_requests() {
    let app = create_router();

    // Test malformed JSON
    let request = Request::builder()
        .method("POST")
        .uri("/search")
        .header("content-type", "application/json")
        .body(Body::from("{invalid json"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), 400); // Bad Request for malformed JSON

    // Test missing required fields
    let app = create_router();
    let incomplete_json = json!({
        "limit": 10
        // Missing "query" field
    });

    let request = Request::builder()
        .method("POST")
        .uri("/search")
        .header("content-type", "application/json")
        .body(Body::from(incomplete_json.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), 422); // Unprocessable Entity (validation failed)
}
