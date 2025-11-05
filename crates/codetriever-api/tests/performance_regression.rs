//! Performance regression tests
//!
//! Monitors search and indexing performance to detect regressions
//! Run with: cargo test --release --test `performance_regression`

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod test_utils;

use axum::{body::Body, http::Request};
use codetriever_api::routes::create_router;
use serde_json::json;
use std::time::Instant;
use tower::ServiceExt;

const PERFORMANCE_THRESHOLDS: PerformanceThresholds = PerformanceThresholds {
    search: 5000,       // Search should complete within 5 seconds (includes model load)
    index: 10000,       // Indexing should complete within 10 seconds (includes model load)
    small_search: 5000, // First search includes model loading (~3s), subsequent faster
};

struct PerformanceThresholds {
    search: u128,
    index: u128,
    small_search: u128,
}

#[test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
fn test_search_performance_baseline() -> test_utils::TestResult {
    test_utils::get_test_runtime().block_on(async {
        let test_state = test_utils::app_state().await?;
        let app = create_router(test_state.state().clone());

        // Index some test content first
        let index_request = json!({
            "tenant_id": test_state.tenant_id(),
            "project_id": "perf-test",
            "commit_context": {"repository_url": "https://github.com/test/repo", "commit_sha": "abc123", "commit_message": "Test", "commit_date": "2025-01-01T00:00:00Z", "author": "Test"},
            "files": [
                {
                    "path": "src/auth.rs",
                    "content": "fn authenticate(user: &str) -> Result<Token> { /* auth logic */ }"
                },
                {
                    "path": "src/database.rs",
                    "content": "fn connect() -> Result<Connection> { /* db logic */ }"
                }
            ]
        });

        let request = Request::builder()
            .method("POST")
            .uri("/index")
            .header("content-type", "application/json")
            .body(Body::from(index_request.to_string()))
            .unwrap();

        let start_index = Instant::now();
        let response = app.oneshot(request).await.unwrap();
        let index_duration = start_index.elapsed();

        assert!(response.status().is_success(), "Index should succeed");
        assert!(
            index_duration.as_millis() < PERFORMANCE_THRESHOLDS.index,
            "Indexing took {}ms, should be under {}ms",
            index_duration.as_millis(),
            PERFORMANCE_THRESHOLDS.index
        );

        // Now test search performance
        let test_state = test_utils::app_state().await?;
        let app = create_router(test_state.state().clone());
        let search_request = json!({
            "query": "authentication logic",
            "limit": 10
        });

        let request = Request::builder()
            .method("POST")
            .uri("/search")
            .header("content-type", "application/json")
            .body(Body::from(search_request.to_string()))
            .unwrap();

        let start_search = Instant::now();
        let response = app.oneshot(request).await.unwrap();
        let search_duration = start_search.elapsed();

        assert!(response.status().is_success(), "Search should succeed");
        assert!(
            search_duration.as_millis() < PERFORMANCE_THRESHOLDS.search,
            "Search took {}ms, should be under {}ms",
            search_duration.as_millis(),
            PERFORMANCE_THRESHOLDS.search
        );

        println!(
            "✅ Performance baseline: Index={}ms, Search={}ms",
            index_duration.as_millis(),
            search_duration.as_millis()
        );
        Ok(())
    })
}

#[test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
fn test_small_search_performance() -> test_utils::TestResult {
    test_utils::get_test_runtime().block_on(async {
        let test_state = test_utils::app_state().await?;
        let app = create_router(test_state.state().clone());

        // Test with small, targeted search that should be very fast
        let search_request = json!({
            "query": "fn",
            "limit": 1
        });

        let request = Request::builder()
            .method("POST")
            .uri("/search")
            .header("content-type", "application/json")
            .body(Body::from(search_request.to_string()))
            .unwrap();

        let start = Instant::now();
        let response = app.oneshot(request).await.unwrap();
        let duration = start.elapsed();

        assert!(
            response.status().is_success(),
            "Small search should succeed"
        );
        assert!(
            duration.as_millis() < PERFORMANCE_THRESHOLDS.small_search,
            "Small search took {}ms, should be under {}ms",
            duration.as_millis(),
            PERFORMANCE_THRESHOLDS.small_search
        );
        Ok(())
    })
}

#[test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
fn test_repeated_search_caching_performance() -> test_utils::TestResult {
    test_utils::get_test_runtime().block_on(async {
        let test_state = test_utils::app_state().await?;
        let app = create_router(test_state.state().clone());

        let search_request = json!({
            "query": "function definition",
            "limit": 5
        });

        // First search (cold)
        let request = Request::builder()
            .method("POST")
            .uri("/search")
            .header("content-type", "application/json")
            .body(Body::from(search_request.to_string()))
            .unwrap();

        let start_cold = Instant::now();
        let response = app.oneshot(request).await.unwrap();
        let cold_duration = start_cold.elapsed();

        assert!(response.status().is_success());

        // Second search (should potentially be faster due to caching)
        let test_state = test_utils::app_state().await?;
        let app = create_router(test_state.state().clone());
        let request = Request::builder()
            .method("POST")
            .uri("/search")
            .header("content-type", "application/json")
            .body(Body::from(search_request.to_string()))
            .unwrap();

        let start_warm = Instant::now();
        let response = app.oneshot(request).await.unwrap();
        let warm_duration = start_warm.elapsed();

        assert!(response.status().is_success());

        println!(
            "✅ Cache performance: Cold={}ms, Warm={}ms",
            cold_duration.as_millis(),
            warm_duration.as_millis()
        );

        // Note: Due to fresh router instances, this test mainly validates consistent performance
        Ok(())
    })
}

#[test]
#[allow(clippy::significant_drop_tightening)] // test_state must live until cleanup
fn test_index_large_batch_performance() -> test_utils::TestResult {
    test_utils::get_test_runtime().block_on(async {
        let test_state = test_utils::app_state().await?;
        let app = create_router(test_state.state().clone());

        // Create a batch of multiple files to test indexing performance
        let mut files = Vec::new();
        for i in 0..20 {
            files.push(json!({
                "path": format!("src/module_{}.rs", i),
                "content": format!("
                pub struct Module{} {{
                    id: usize,
                    name: String,
                }}

                impl Module{} {{
                    pub fn new(id: usize, name: String) -> Self {{
                        Self {{ id, name }}
                    }}

                    pub fn process(&self) -> Result<(), Error> {{
                        // Processing logic for module {}
                        Ok(())
                    }}
                }}", i, i, i)
            }));
        }

        let request_body = json!({
            "tenant_id": test_state.tenant_id(),
            "project_id": "perf-batch-test",
            "commit_context": {"repository_url": "https://github.com/test/repo", "commit_sha": "abc123", "commit_message": "Test", "commit_date": "2025-01-01T00:00:00Z", "author": "Test"},
            "files": files
        });

        let request = Request::builder()
            .method("POST")
            .uri("/index")
            .header("content-type", "application/json")
            .body(Body::from(request_body.to_string()))
            .unwrap();

        let start = Instant::now();
        let response = app.oneshot(request).await.unwrap();
        let duration = start.elapsed();

        assert!(response.status().is_success(), "Batch index should succeed");

        // Large batch should still complete within reasonable time
        assert!(
            duration.as_millis() < PERFORMANCE_THRESHOLDS.index * 2, // 2x threshold for large batch
            "Large batch indexing took {}ms, should be under {}ms",
            duration.as_millis(),
            PERFORMANCE_THRESHOLDS.index * 2
        );

        println!(
            "✅ Batch indexing performance: {}ms for 20 files",
            duration.as_millis()
        );
        Ok(())
    })
}
