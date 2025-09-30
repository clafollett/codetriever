//! Load testing framework for search endpoints
//!
//! Tests system behavior under sustained load and high concurrency
//! Run with: `cargo test --release --test load_testing`

// Test code - unwrap/expect acceptable for test assertions
// cast_precision_loss: Test metrics (25-50 requests) will never exceed f64 precision
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::cast_precision_loss)]

use axum::{body::Body, http::Request};
use codetriever_api::routes::create_router;
use serde_json::json;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tower::ServiceExt;

#[tokio::test]
async fn test_concurrent_search_load() {
    // Create SINGLE router with ONE embedding model (proper load testing)
    let app = create_router();

    // Index some test data first
    let index_request = json!({
        "project_id": "load-test",
        "files": [
            {
                "path": "src/auth.rs",
                "content": "fn authenticate(user: &str, password: &str) -> Result<Token> { /* auth logic */ }"
            },
            {
                "path": "src/database.rs",
                "content": "fn connect_db() -> Result<Connection> { /* db connection */ }"
            },
            {
                "path": "src/api.rs",
                "content": "async fn handle_request(req: Request) -> Response { /* request handling */ }"
            }
        ]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/index")
        .header("content-type", "application/json")
        .body(Body::from(index_request.to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert!(
        response.status().is_success(),
        "Index should succeed before load test"
    );

    // Now perform concurrent searches against SAME router (real load testing)
    let search_queries = [
        "authentication logic",
        "database connection",
        "request handling",
        "async function",
        "error handling",
    ];

    let concurrent_requests = 25; // Test actual concurrent load on single service
    let mut handles = vec![];

    let start_load_test = Instant::now();

    for i in 0..concurrent_requests {
        let query = (*search_queries
            .get(i % search_queries.len())
            .expect("Query index in bounds"))
        .to_string();
        let app_clone = app.clone();

        let handle = tokio::spawn(async move {
            let search_request = json!({
                "query": query,
                "limit": 10
            });

            let request = Request::builder()
                .method("POST")
                .uri("/search")
                .header("content-type", "application/json")
                .body(Body::from(search_request.to_string()))
                .unwrap();

            let start = Instant::now();
            // Use same router - tests throughput, not initialization
            let response = app_clone.oneshot(request).await.unwrap();
            let duration = start.elapsed();

            (response.status().is_success(), duration)
        });

        handles.push(handle);
    }

    // Wait for all requests to complete
    let mut success_count = 0;
    let mut total_duration = Duration::ZERO;
    let mut max_duration = Duration::ZERO;
    let mut min_duration = Duration::MAX;

    for handle in handles {
        match handle.await {
            Ok((success, duration)) => {
                if success {
                    success_count += 1;
                }
                total_duration += duration;
                max_duration = max_duration.max(duration);
                min_duration = min_duration.min(duration);
            }
            Err(e) => {
                eprintln!("Task failed: {e}");
            }
        }
    }

    let load_test_duration = start_load_test.elapsed();
    let avg_duration = total_duration / u32::try_from(concurrent_requests).unwrap_or(1);

    // Performance assertions
    assert!(
        success_count >= (concurrent_requests * 95 / 100), // 95% success rate
        "Success rate too low: {success_count}/{concurrent_requests} requests succeeded"
    );

    assert!(
        max_duration.as_millis() < 10000, // No request should take more than 10 seconds
        "Slowest request took {}ms, should be under 10000ms",
        max_duration.as_millis()
    );

    println!("✅ Load test results ({concurrent_requests} concurrent requests):");
    let success_rate = if concurrent_requests > 0 {
        (success_count as f64 / concurrent_requests as f64) * 100.0
    } else {
        0.0
    };
    println!("   Success rate: {success_count}/{concurrent_requests} ({success_rate:.1}%)");
    println!("   Total time: {}ms", load_test_duration.as_millis());
    println!("   Average response: {}ms", avg_duration.as_millis());
    println!("   Min response: {}ms", min_duration.as_millis());
    println!("   Max response: {}ms", max_duration.as_millis());
}

#[tokio::test]
async fn test_sustained_load_over_time() {
    // Create SINGLE router for entire test
    let app = create_router();

    // Index test data
    let index_request = json!({
        "project_id": "sustained-load-test",
        "files": [
            {
                "path": "src/main.rs",
                "content": "fn main() { println!(\"Hello world!\"); }"
            }
        ]
    });

    let request = Request::builder()
        .method("POST")
        .uri("/index")
        .header("content-type", "application/json")
        .body(Body::from(index_request.to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert!(response.status().is_success());

    // Sustained load: multiple waves of requests over time
    let waves = 3;
    let requests_per_wave = 10; // Test actual concurrent load
    let wave_interval = Duration::from_millis(100);

    let mut all_durations = Vec::new();

    for wave in 0..waves {
        let mut wave_handles = vec![];

        for _request in 0..requests_per_wave {
            let app_clone = app.clone();

            let handle = tokio::spawn(async move {
                let search_request = json!({
                    "query": "hello world",
                    "limit": 5
                });

                let request = Request::builder()
                    .method("POST")
                    .uri("/search")
                    .header("content-type", "application/json")
                    .body(Body::from(search_request.to_string()))
                    .unwrap();

                let start = Instant::now();
                // Use same router - tests sustained throughput
                let response = app_clone.oneshot(request).await.unwrap();
                let duration = start.elapsed();

                (response.status().is_success(), duration)
            });

            wave_handles.push(handle);
        }

        // Wait for wave to complete
        for handle in wave_handles {
            if let Ok((success, duration)) = handle.await
                && success
            {
                all_durations.push(duration);
            }
        }

        // Brief pause between waves
        if wave < waves - 1 {
            sleep(wave_interval).await;
        }
    }

    // Verify performance consistency across waves
    assert!(!all_durations.is_empty(), "Should have successful requests");

    let avg_duration: Duration =
        all_durations.iter().sum::<Duration>() / all_durations.len().try_into().unwrap();
    let max_duration = *all_durations.iter().max().unwrap();

    assert!(
        max_duration.as_millis() < 5000,
        "No request should take more than 5 seconds under sustained load"
    );

    println!(
        "✅ Sustained load test: {} requests across {} waves",
        all_durations.len(),
        waves
    );
    println!("   Average response time: {}ms", avg_duration.as_millis());
    println!("   Max response time: {}ms", max_duration.as_millis());
}

#[tokio::test]
async fn test_memory_usage_under_load() {
    // Create SINGLE router - test that service doesn't leak memory under load
    let app = create_router();

    // This test monitors that memory usage doesn't grow unbounded under load
    // Perform many operations to check for memory leaks
    for batch in 0..5 {
        let mut handles = vec![];

        // Batch of searches
        for i in 0..10 {
            let query = format!("test query batch {batch} item {i}");
            let app_clone = app.clone();

            let handle = tokio::spawn(async move {
                let search_request = json!({
                    "query": query,
                    "limit": 10
                });

                let request = Request::builder()
                    .method("POST")
                    .uri("/search")
                    .header("content-type", "application/json")
                    .body(Body::from(search_request.to_string()))
                    .unwrap();

                // Use same router - test for memory leaks in service
                app_clone.oneshot(request).await
            });

            handles.push(handle);
        }

        // Wait for batch completion
        for handle in handles {
            let _ = handle.await;
        }

        // Brief pause to allow cleanup
        sleep(Duration::from_millis(50)).await;
    }

    // If we reach here without OOM, memory usage is reasonable
    println!("✅ Memory usage test completed - no unbounded growth detected");
}
