//! Mock implementation of VectorStorage for testing
//!
//! This module provides a mock storage backend that stores data in memory,
//! useful for unit tests and development without requiring a real Qdrant instance.

use crate::{
    VectorDataError, VectorDataResult,
    storage::{CodeChunk, StorageSearchResult, StorageStats, VectorStorage},
};
use async_trait::async_trait;
use codetriever_common::CorrelationId;
use codetriever_meta_data::generate_chunk_id;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// Type aliases to simplify complex types
type ChunkStore = Arc<Mutex<Vec<StoredChunk>>>;

/// Chunk with repository context for mock storage
#[derive(Debug, Clone)]
pub struct StoredChunk {
    pub chunk: CodeChunk,
    pub repository_id: String,
    pub branch: String,
    pub generation: i64,
    pub chunk_id: Uuid,
    pub correlation_id: CorrelationId,
}

/// Mock storage backend for testing
#[derive(Clone)]
pub struct MockStorage {
    chunks: ChunkStore,
    collection_exists: Arc<Mutex<bool>>,
    fail_on_store: bool,
    fail_on_search: bool,
}

impl MockStorage {
    /// Create a new mock storage instance
    pub fn new() -> Self {
        Self {
            chunks: Arc::new(Mutex::new(Vec::new())),
            collection_exists: Arc::new(Mutex::new(false)),
            fail_on_store: false,
            fail_on_search: false,
        }
    }

    /// Configure to fail on store operations (for testing error handling)
    pub fn with_store_failure(mut self) -> Self {
        self.fail_on_store = true;
        self
    }

    /// Configure to fail on search operations (for testing error handling)
    pub fn with_search_failure(mut self) -> Self {
        self.fail_on_search = true;
        self
    }

    /// Get the stored chunks (for test assertions)
    pub fn get_chunks(&self) -> Vec<CodeChunk> {
        self.chunks
            .lock()
            .unwrap()
            .iter()
            .map(|stored_chunk| stored_chunk.chunk.clone())
            .collect()
    }

    /// Get chunks for a specific repository and branch (for repository isolation testing)
    pub fn get_chunks_for_repo(&self, repo_id: &str, branch: &str) -> Vec<CodeChunk> {
        self.chunks
            .lock()
            .unwrap()
            .iter()
            .filter(|stored_chunk| {
                stored_chunk.repository_id == repo_id && stored_chunk.branch == branch
            })
            .map(|stored_chunk| stored_chunk.chunk.clone())
            .collect()
    }

    /// Count chunks by generation (for version testing)
    pub fn chunk_count_by_generation(&self, generation: i64) -> usize {
        self.chunks
            .lock()
            .unwrap()
            .iter()
            .filter(|stored_chunk| stored_chunk.generation == generation)
            .count()
    }

    /// Get the most recent correlation ID used (for tracing verification)
    pub fn last_correlation_id(&self) -> Option<CorrelationId> {
        self.chunks
            .lock()
            .unwrap()
            .last()
            .map(|stored_chunk| stored_chunk.correlation_id.clone())
    }

    /// Get stored chunks with full context (for advanced testing)
    pub fn get_stored_chunks_with_context(&self) -> Vec<StoredChunk> {
        self.chunks.lock().unwrap().clone()
    }
}

impl Default for MockStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VectorStorage for MockStorage {
    async fn store_chunks(
        &self,
        repository_id: &str,
        branch: &str,
        chunks: &[CodeChunk],
        generation: i64,
        correlation_id: &CorrelationId,
    ) -> VectorDataResult<Vec<Uuid>> {
        if self.fail_on_store {
            return Err(VectorDataError::Storage(
                "Mock storage configured to fail".into(),
            ));
        }

        let mut stored = self.chunks.lock().unwrap();
        let mut ids = Vec::new();

        for chunk in chunks.iter() {
            // Use proper chunk ID generation from meta-data crate
            let chunk_id = generate_chunk_id(
                repository_id,
                branch,
                &chunk.file_path,
                generation,
                chunk.byte_start,
                chunk.byte_end,
            );

            ids.push(chunk_id);

            // Store chunk with full repository context and correlation ID
            let stored_chunk = StoredChunk {
                chunk: chunk.clone(),
                repository_id: repository_id.to_string(),
                branch: branch.to_string(),
                generation,
                chunk_id,
                correlation_id: correlation_id.clone(),
            };
            stored.push(stored_chunk);
        }

        Ok(ids)
    }

    async fn search(
        &self,
        _query_embedding: Vec<f32>,
        limit: usize,
        correlation_id: &CorrelationId,
    ) -> VectorDataResult<Vec<StorageSearchResult>> {
        if self.fail_on_search {
            return Err(VectorDataError::Storage(
                "Mock storage configured to fail".into(),
            ));
        }

        let stored = self.chunks.lock().unwrap();

        // Log correlation ID for testing/debugging
        tracing::debug!(
            correlation_id = %correlation_id,
            chunk_count = stored.len(),
            "Mock search operation"
        );

        // Return up to 'limit' chunks with mock similarity scores
        let results: Vec<StorageSearchResult> = stored
            .iter()
            .take(limit)
            .enumerate()
            .map(|(i, stored_chunk)| StorageSearchResult {
                chunk: stored_chunk.chunk.clone(),
                // Mock decreasing similarity scores
                similarity: 1.0 - (i as f32 * 0.1),
            })
            .collect();

        Ok(results)
    }

    async fn delete_chunks(&self, chunk_ids: &[Uuid]) -> VectorDataResult<()> {
        if !chunk_ids.is_empty() {
            let mut stored = self.chunks.lock().unwrap();
            // Remove chunks by their IDs
            stored.retain(|stored_chunk| !chunk_ids.contains(&stored_chunk.chunk_id));
        }
        Ok(())
    }

    async fn collection_exists(&self) -> VectorDataResult<bool> {
        Ok(*self.collection_exists.lock().unwrap())
    }

    async fn ensure_collection(&self) -> VectorDataResult<()> {
        *self.collection_exists.lock().unwrap() = true;
        Ok(())
    }

    async fn drop_collection(&self) -> VectorDataResult<bool> {
        let existed = *self.collection_exists.lock().unwrap();
        *self.collection_exists.lock().unwrap() = false;
        self.chunks.lock().unwrap().clear();
        Ok(existed)
    }

    async fn get_stats(&self) -> VectorDataResult<StorageStats> {
        let stored = self.chunks.lock().unwrap();
        Ok(StorageStats {
            vector_count: stored.len(),
            storage_bytes: Some((stored.len() * 1024) as u64), // Rough estimate
            collection_name: "mock_collection".to_string(),
            storage_type: "mock".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_storage_basic_operations() {
        let storage = MockStorage::new();

        // Test collection operations
        assert!(!storage.collection_exists().await.unwrap());
        storage.ensure_collection().await.unwrap();
        assert!(storage.collection_exists().await.unwrap());

        // Test storing chunks
        let chunks = vec![CodeChunk {
            file_path: "test.rs".to_string(),
            content: "fn main() {}".to_string(),
            start_line: 1,
            end_line: 1,
            byte_start: 0,
            byte_end: 12,
            language: "rust".to_string(),
            kind: Some("function".to_string()),
            name: Some("main".to_string()),
            token_count: Some(5),
            embedding: Some(vec![0.1; 768]),
        }];

        let correlation_id = CorrelationId::new();
        let chunk_ids = storage
            .store_chunks("test_repo", "main", &chunks, 1, &correlation_id)
            .await
            .unwrap();
        assert_eq!(chunk_ids.len(), 1);

        // Test search
        let results = storage
            .search(vec![0.1; 768], 10, &correlation_id)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);

        // Test stats
        let stats = storage.get_stats().await.unwrap();
        assert_eq!(stats.vector_count, 1);
        assert_eq!(stats.storage_type, "mock");

        // Test drop collection
        let existed = storage.drop_collection().await.unwrap();
        assert!(existed);
        assert!(!storage.collection_exists().await.unwrap());
    }

    #[tokio::test]
    async fn test_mock_storage_failure_modes() {
        let storage = MockStorage::new().with_store_failure();

        let chunks = vec![CodeChunk {
            file_path: "test.rs".to_string(),
            content: "fn main() {}".to_string(),
            start_line: 1,
            end_line: 1,
            byte_start: 0,
            byte_end: 12,
            language: "rust".to_string(),
            kind: Some("function".to_string()),
            name: Some("main".to_string()),
            token_count: Some(5),
            embedding: Some(vec![0.1; 768]),
        }];

        let correlation_id = CorrelationId::new();
        // Should fail to store
        assert!(
            storage
                .store_chunks("test_repo", "main", &chunks, 1, &correlation_id)
                .await
                .is_err()
        );

        let storage = MockStorage::new().with_search_failure();
        // Should fail to search
        assert!(
            storage
                .search(vec![0.1; 768], 10, &correlation_id)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_repository_isolation() {
        let storage = MockStorage::new();

        let chunk1 = CodeChunk {
            file_path: "main.rs".to_string(),
            content: "fn main() {}".to_string(),
            start_line: 1,
            end_line: 1,
            byte_start: 0,
            byte_end: 12,
            language: "rust".to_string(),
            kind: Some("function".to_string()),
            name: Some("main".to_string()),
            token_count: Some(5),
            embedding: Some(vec![0.1; 768]),
        };

        // Store in different repositories
        let correlation_id1 = CorrelationId::new();
        let correlation_id2 = CorrelationId::new();

        storage
            .store_chunks(
                "repo1",
                "main",
                std::slice::from_ref(&chunk1),
                1,
                &correlation_id1,
            )
            .await
            .unwrap();
        storage
            .store_chunks(
                "repo2",
                "dev",
                std::slice::from_ref(&chunk1),
                1,
                &correlation_id2,
            )
            .await
            .unwrap();

        // Verify repository isolation
        assert_eq!(storage.get_chunks_for_repo("repo1", "main").len(), 1);
        assert_eq!(storage.get_chunks_for_repo("repo2", "dev").len(), 1);
        assert_eq!(storage.get_chunks_for_repo("repo1", "dev").len(), 0);
        assert_eq!(storage.get_chunks_for_repo("repo3", "main").len(), 0);
    }

    #[tokio::test]
    async fn test_generation_tracking() {
        let storage = MockStorage::new();

        let chunk = CodeChunk {
            file_path: "test.rs".to_string(),
            content: "test".to_string(),
            start_line: 1,
            end_line: 1,
            byte_start: 0,
            byte_end: 4,
            language: "rust".to_string(),
            kind: None,
            name: None,
            token_count: Some(1),
            embedding: Some(vec![0.1; 768]),
        };

        let correlation_id = CorrelationId::new();

        // Store chunks with different generations
        storage
            .store_chunks(
                "repo",
                "main",
                std::slice::from_ref(&chunk),
                1,
                &correlation_id,
            )
            .await
            .unwrap();
        storage
            .store_chunks(
                "repo",
                "main",
                std::slice::from_ref(&chunk),
                2,
                &correlation_id,
            )
            .await
            .unwrap();
        storage
            .store_chunks(
                "repo",
                "main",
                std::slice::from_ref(&chunk),
                1,
                &correlation_id,
            )
            .await
            .unwrap();

        // Verify generation counting
        assert_eq!(storage.chunk_count_by_generation(1), 2);
        assert_eq!(storage.chunk_count_by_generation(2), 1);
        assert_eq!(storage.chunk_count_by_generation(3), 0);
    }

    #[tokio::test]
    async fn test_correlation_id_tracking() {
        let storage = MockStorage::new();

        let chunk = CodeChunk {
            file_path: "test.rs".to_string(),
            content: "test".to_string(),
            start_line: 1,
            end_line: 1,
            byte_start: 0,
            byte_end: 4,
            language: "rust".to_string(),
            kind: None,
            name: None,
            token_count: Some(1),
            embedding: Some(vec![0.1; 768]),
        };

        // Test that correlation IDs are tracked
        assert!(storage.last_correlation_id().is_none());

        let correlation_id = CorrelationId::from("test-trace-123");
        storage
            .store_chunks(
                "repo",
                "main",
                std::slice::from_ref(&chunk),
                1,
                &correlation_id,
            )
            .await
            .unwrap();

        assert_eq!(storage.last_correlation_id(), Some(correlation_id));
    }
}
