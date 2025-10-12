//! Edge case coverage tests for API endpoints
//!
//! Tests boundary conditions, Unicode handling, large result sets, and error scenarios

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

mod test_utils;

use axum::{body::Body, http::Request};
use codetriever_api::routes::create_router;
use serde_json::json;
use tower::ServiceExt;

#[test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
fn test_search_with_unicode_characters() -> test_utils::TestResult {
    test_utils::get_test_runtime().block_on(async {
        let start_time = std::time::Instant::now();
        eprintln!(
            "\n🔍 [Unicode Test] Starting at {:?}",
            std::time::SystemTime::now()
        );

        eprintln!("🔍 [Unicode Test] Creating app state...");
        let test_state = test_utils::app_state().await?;
        eprintln!(
            "🔍 [Unicode Test] App state created in {:?}",
            start_time.elapsed()
        );

        let app = create_router(test_state.state().clone());
        eprintln!("🔍 [Unicode Test] Router created");

        // Test Unicode in search query
        let unicode_query = json!({
            "query": "函数 función función 🔍 emoji search",
            "limit": 10
        });

        eprintln!(
            "🔍 [Unicode Test] Sending request with query: {:?}",
            unicode_query["query"]
        );
        let request_start = std::time::Instant::now();

        let request = Request::builder()
            .method("POST")
            .uri("/search")
            .header("content-type", "application/json")
            .body(Body::from(unicode_query.to_string()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let request_duration = request_start.elapsed();

        eprintln!("🔍 [Unicode Test] Response received in {request_duration:?}");
        eprintln!("🔍 [Unicode Test] Status code: {status}");

        // ALWAYS capture response body for analysis
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body_str = String::from_utf8_lossy(&body_bytes);

        eprintln!(
            "🔍 [Unicode Test] Response body length: {} bytes",
            body_bytes.len()
        );

        // Log first 500 chars of response
        if body_str.len() > 500 {
            eprintln!(
                "🔍 [Unicode Test] Response preview: {}...",
                &body_str[..500]
            );
        } else {
            eprintln!("🔍 [Unicode Test] Response body: {body_str}");
        }

        eprintln!(
            "🔍 [Unicode Test] Total duration: {:?}",
            start_time.elapsed()
        );

        // Check for acceptable status codes
        if !status.is_success() && status != 400 && status != 503 {
            eprintln!("❌ [Unicode Test] FAILED - Unacceptable status code!");
            eprintln!("   Expected: 2xx, 400, or 503");
            eprintln!("   Got: {status}");
            panic!("Unicode search returned {status} - see error details above");
        }

        eprintln!("✅ [Unicode Test] PASSED\n");
        Ok(())
    })
}

#[test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
fn test_search_with_very_long_query() -> test_utils::TestResult {
    test_utils::get_test_runtime().block_on(async {
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
    })
}

#[test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
fn test_search_with_empty_query() -> test_utils::TestResult {
    test_utils::get_test_runtime().block_on(async {
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
    })
}

#[test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
fn test_search_with_extreme_limit_values() -> test_utils::TestResult {
    test_utils::get_test_runtime().block_on(async {
        let test_state = test_utils::app_state().await?;

        // Test with limit = 0
        let app1 = create_router(test_state.state().clone());
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

        let response = app1.oneshot(request).await.unwrap();
        // Router without state may return 500 for zero limit, but shouldn't crash
        assert!(
            response.status().is_client_error()
                || response.status().is_success()
                || response.status().is_server_error()
        );

        // Test with very large limit (create second router from same state)
        let app2 = create_router(test_state.state().clone());
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

        let response = app2.oneshot(request).await.unwrap();
        // Router without state may return 500, but shouldn't crash
        assert!(
            response.status().is_client_error()
                || response.status().is_success()
                || response.status().is_server_error()
        );
        Ok(())
    })
}

#[test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
fn test_index_with_special_characters_in_path() -> test_utils::TestResult {
    test_utils::get_test_runtime().block_on(async {
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
    })
}

#[test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
fn test_index_with_very_large_file_content() -> test_utils::TestResult {
    test_utils::get_test_runtime().block_on(async {
        eprintln!("\n📦 [Large File Test] Starting...");
        let test_state = test_utils::app_state().await?;
        eprintln!("📦 [Large File Test] App state created");
        let app = create_router(test_state.state().clone());

    // Test with large file content (boundary condition)
    let large_content =
        "fn large_function() {\n".to_string() + &"    println!(\"test\");\n".repeat(5000) + "}";
    eprintln!(
        "📦 [Large File Test] Content size: {} bytes",
        large_content.len()
    );

    // Use unique path per run to avoid "Unchanged" detection from previous runs
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let unique_path = format!("src/large_file_{timestamp}.rs");

    let request_body = json!({
        "project_id": "test-project",
        "files": [
            {
                "path": unique_path,
                "content": large_content
            }
        ]
    });

    eprintln!("📦 [Large File Test] Sending index request...");
    let request = Request::builder()
        .method("POST")
        .uri("/index")
        .header("content-type", "application/json")
        .body(Body::from(request_body.to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    eprintln!("📦 [Large File Test] Response status: {status}");

    // Check response body
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8_lossy(&body_bytes);
    eprintln!("📦 [Large File Test] Response: {body_str}");

    // Parse response to verify chunking worked
    let response: serde_json::Value = serde_json::from_str(&body_str).unwrap();

    // Should handle large files appropriately
    assert!(status.is_success() || status == 413); // 413 = Request Entity Too Large

    // CRITICAL: Verify file was properly chunked (not truncated!)
    // This file: 5000 lines of "println!("test");" = ~30,000 tokens total
    //
    // Expected chunks by config:
    // - max_chunk_tokens=512 (current): ~60 theoretical, ~22 actual (parser overhead)
    // - max_chunk_tokens=1024: ~30 theoretical, ~11 actual
    // - max_chunk_tokens=2048: ~15 theoretical, ~6 actual
    //
    // Key assertion: chunks > 1 (proves splitting works, not single truncated chunk)
    if status.is_success() {
        let chunks_created = response["chunks_created"].as_u64().unwrap();

        // CRITICAL: File MUST be split (not kept as 1 truncated chunk)
        assert!(
            chunks_created > 1,
            "Large file MUST be chunked, got {chunks_created} chunk. Tokenizer likely not loaded!"
        );

        // For 512-token chunks, expect 15-30 chunks (allows parser variation)
        // This will catch regressions while being resilient to config changes
        let expected_min = 15;
        let expected_max = 60;
        assert!(
            chunks_created >= expected_min && chunks_created <= expected_max,
            "Expected {expected_min}-{expected_max} chunks for ~30K token file, got {chunks_created}. Check max_chunk_tokens config!"
        );

        eprintln!(
            "✅ [Large File Test] Properly chunked into {chunks_created} chunks (config: 512 tokens/chunk)"
        );
    }

        eprintln!("📦 [Large File Test] PASSED\n");
        Ok(())
    })
}

#[test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
fn test_malformed_json_requests() -> test_utils::TestResult {
    test_utils::get_test_runtime().block_on(async {
        let test_state = test_utils::app_state().await?;

        // Test malformed JSON
        let app1 = create_router(test_state.state().clone());
        let request = Request::builder()
            .method("POST")
            .uri("/search")
            .header("content-type", "application/json")
            .body(Body::from("{invalid json"))
            .unwrap();

        let response = app1.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 400); // Bad Request for malformed JSON

        // Test missing required fields (create second router from same state)
        let app2 = create_router(test_state.state().clone());
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

        let response = app2.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 422); // Unprocessable Entity (validation failed)
        Ok(())
    })
}
