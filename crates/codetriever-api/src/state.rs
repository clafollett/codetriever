//! Application state for Axum handlers
//!
//! Contains shared resources like database, vector storage, search, and indexing services
//! that are initialized once at startup and passed to all handlers.

use std::sync::Arc;

use codetriever_indexing::IndexerService;
use codetriever_meta_data::DataClient;
use codetriever_search::SearchService;
use codetriever_vector_data::VectorStorage;

/// Type alias for indexer service handle (no mutex - API methods use &self!)
pub type IndexerServiceHandle = Arc<dyn IndexerService>;

/// Application state containing all shared services
///
/// This state is initialized once at application startup and passed to all
/// Axum handlers via dependency injection, avoiding expensive pool/service
/// creation on every request.
///
/// # Performance
///
/// By sharing connection pools and services across all handlers:
/// - `/status` endpoint: ~157ms → <20ms (no pool creation)
/// - `/search` endpoint: Eliminates lazy initialization overhead
/// - `/index` endpoint: Reuses same DB+Qdrant connections
#[derive(Clone)]
pub struct AppState {
    /// Database client for `PostgreSQL` operations
    pub db_client: Arc<DataClient>,
    /// Vector storage client for Qdrant operations
    pub vector_storage: Arc<dyn VectorStorage>,
    /// Search service for semantic code search
    pub search_service: Arc<dyn SearchService>,
    /// Indexing service for processing and storing code chunks
    pub indexer_service: IndexerServiceHandle,
    /// Vector storage namespace for job routing
    pub vector_namespace: String,
}

impl AppState {
    /// Create new application state with all services
    #[must_use]
    pub fn new(
        db_client: Arc<DataClient>,
        vector_storage: Arc<dyn VectorStorage>,
        search_service: Arc<dyn SearchService>,
        indexer_service: IndexerServiceHandle,
        vector_namespace: String,
    ) -> Self {
        Self {
            db_client,
            vector_storage,
            search_service,
            indexer_service,
            vector_namespace,
        }
    }
}
