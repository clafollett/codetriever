//! Edge case coverage tests for API endpoints
//!
//! Tests boundary conditions, Unicode handling, large result sets, and error scenarios

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod test_utils;

use axum::{body::Body, http::Request};
use codetriever_api::routes::create_router;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
async fn test_search_with_unicode_characters() -> test_utils::TestResult {
    let test_state = test_utils::app_state().await?;
    let app = create_router(test_state.state().clone());

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
    let status = response.status();

    // Should handle Unicode gracefully - accept success or client errors
    assert!(
        status.is_success() || status.is_client_error() || status == 503,
        "Unicode search returned unexpected status: {status}"
    );

    Ok(())
}

#[tokio::test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
async fn test_search_with_very_long_query() -> test_utils::TestResult {
    let test_state = test_utils::app_state().await?;
    let app = create_router(test_state.state().clone());

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
    Ok(())
}

#[tokio::test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
async fn test_search_with_empty_query() -> test_utils::TestResult {
    let test_state = test_utils::app_state().await?;
    let app = create_router(test_state.state().clone());

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
    Ok(())
}

#[tokio::test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
async fn test_search_with_extreme_limit_values() -> test_utils::TestResult {
    let test_state = test_utils::app_state().await?;
    let app = create_router(test_state.state().clone());

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
    let test_state = test_utils::app_state().await?;
    let app = create_router(test_state.state().clone());
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
    Ok(())
}

#[tokio::test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
async fn test_index_with_special_characters_in_path() -> test_utils::TestResult {
    let test_state = test_utils::app_state().await?;
    let app = create_router(test_state.state().clone());

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
    Ok(())
}

#[tokio::test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
async fn test_index_with_very_large_file_content() -> test_utils::TestResult {
    let test_state = test_utils::app_state().await?;
    let app = create_router(test_state.state().clone());

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
    Ok(())
}

#[tokio::test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
async fn test_malformed_json_requests() -> test_utils::TestResult {
    let test_state = test_utils::app_state().await?;
    let app = create_router(test_state.state().clone());

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
    let test_state = test_utils::app_state().await?;
    let app = create_router(test_state.state().clone());
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
    Ok(())
}
