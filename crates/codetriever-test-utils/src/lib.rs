//! Shared test utilities for all Codetriever integration tests
//!
//! Provides a persistent Tokio runtime and atomic counter shared across
//! ALL integration tests in ALL crates, preventing race conditions and
//! ensuring resource isolation.
//!
//! ## Usage
//!
//! In your test crate's `Cargo.toml`:
//! ```toml
//! [dev-dependencies]
//! codetriever-test-utils = { path = "../codetriever-test-utils" }
//! ```
//!
//! In your tests:
//! ```no_run
//! #[test]
//! fn my_integration_test() {
//!     codetriever_test_utils::get_test_runtime().block_on(async {
//!         let counter = codetriever_test_utils::next_collection_counter();
//!         // ... test logic ...
//!     })
//! }
//! ```

use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Shared Tokio runtime for ALL integration tests across ALL crates
///
/// This runtime persists for the entire test suite lifetime, preventing:
/// - "Tokio context is being shutdown" errors
/// - Premature disposal of shared database/embedding pools
/// - Resource conflicts when spawned tasks outlive their originating test
///
/// All integration tests MUST use this runtime via `get_test_runtime()`.
static TEST_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// Global atomic counter for unique collection names across ALL test crates
///
/// Prevents collection name collisions when tests run in parallel across
/// multiple crates (e.g., codetriever-api and codetriever-indexing).
static COLLECTION_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Get the shared test runtime (creates on first call, reuses thereafter)
///
/// The runtime is shared across ALL test crates to ensure:
/// - One runtime context for all async operations
/// - Database pools stay valid across all tests
/// - Embedding services don't get killed prematurely
///
/// **Configuration:**
/// - Workers default to CPU count for optimal parallelism
/// - Override with `TEST_RUNTIME_WORKERS` environment variable
///
/// # Panics
/// Panics if the runtime cannot be created (should never happen in normal conditions)
#[allow(clippy::expect_used)] // Test infrastructure - panic on init failure is acceptable
pub fn get_test_runtime() -> &'static tokio::runtime::Runtime {
    TEST_RUNTIME.get_or_init(|| {
        // Allow override via env var, default to CPU count for optimal parallelism
        let workers = std::env::var("TEST_RUNTIME_WORKERS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(std::num::NonZero::get)
                    .unwrap_or(4)
            });

        eprintln!(
            "🚀 Creating shared test runtime with {workers} workers (override with TEST_RUNTIME_WORKERS)"
        );

        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("test-runtime")
            .worker_threads(workers)
            .build()
            .expect("Failed to create test runtime")
    })
}

/// Get next unique collection counter value
///
/// Returns a monotonically increasing counter value that's unique across
/// ALL test crates. Use this with timestamp and test name to generate
/// unique collection names:
///
/// ```ignore
/// use std::time::{SystemTime, UNIX_EPOCH};
/// use codetriever_test_utils::next_collection_counter;
///
/// let test_name = "my_test";
/// let counter = next_collection_counter();
/// let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
/// let collection_name = format!("{test_name}_{timestamp}_{counter}");
/// ```
///
/// # Example
/// ```
/// use codetriever_test_utils::next_collection_counter;
///
/// let id1 = next_collection_counter(); // 0
/// let id2 = next_collection_counter(); // 1
/// let id3 = next_collection_counter(); // 2
/// ```
pub fn next_collection_counter() -> usize {
    COLLECTION_COUNTER.fetch_add(1, Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_is_reusable() {
        let rt1 = get_test_runtime();
        let rt2 = get_test_runtime();

        // Should be same instance
        assert!(std::ptr::eq(rt1, rt2));
    }

    #[test]
    fn test_counter_increments() {
        let start = next_collection_counter();
        let next = next_collection_counter();

        assert_eq!(next, start + 1);
    }

    #[test]
    fn test_runtime_executes_async() {
        let result = get_test_runtime().block_on(async {
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
            42
        });

        assert_eq!(result, 42);
    }
}
