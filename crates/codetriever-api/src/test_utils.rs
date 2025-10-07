//! Unit test utilities
//!
//! Provides mock state for fast unit tests that don't require infrastructure.

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::AppState;

/// Standard test result type for all test functions
pub type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Create mock `AppState` for unit testing
///
/// Uses mock implementations for all services with empty data.
/// Fast, no infrastructure required.
#[must_use]
pub fn mock_app_state() -> AppState {
    let db_client = codetriever_meta_data::MockDataClient::new();
    let vector_storage = codetriever_vector_data::MockStorage::new();
    let search_service = codetriever_search::test_mocks::MockSearchService::empty();
    let indexer_service = codetriever_indexing::test_mocks::MockIndexerService::new(0, 0);

    AppState {
        db_client: Arc::new(db_client),
        vector_storage: Arc::new(vector_storage),
        search_service: Arc::new(search_service),
        indexer_service: Arc::new(Mutex::new(indexer_service)),
    }
}
