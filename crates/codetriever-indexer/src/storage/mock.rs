//! Mock implementation of VectorStorage for testing
//!
//! This module provides a mock storage backend that stores data in memory,
//! useful for unit tests and development without requiring a real Qdrant instance.

use crate::{
    IndexerResult,
    parsing::CodeChunk,
    storage::{StorageSearchResult, StorageStats, VectorStorage},
};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// Type alias to simplify the complex type
type ChunkStore = Arc<Mutex<Vec<CodeChunk>>>;

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
    async fn store_chunks(&self, chunks: &[CodeChunk]) -> IndexerResult<usize> {
        if self.fail_on_store {
            return Err(crate::IndexerError::Storage(
                "Mock storage configured to fail".into(),
            ));
        }

        let mut stored = self.chunks.lock().unwrap();
        stored.extend_from_slice(chunks);
        Ok(chunks.len())
    }

    async fn store_chunks_with_ids(
        &self,
        repository_id: &str,
        branch: &str,
        chunks: &[CodeChunk],
        generation: i64,
    ) -> IndexerResult<Vec<Uuid>> {
        if self.fail_on_store {
            return Err(crate::IndexerError::Storage(
                "Mock storage configured to fail".into(),
            ));
        }

        let mut stored = self.chunks.lock().unwrap();
        let mut ids = Vec::new();

        for (index, chunk) in chunks.iter().enumerate() {
            // Generate deterministic UUID v5 like the real implementation
            let namespace = Uuid::NAMESPACE_URL;
            let key = format!("{repository_id}:{branch}:{generation}:{index}");
            let chunk_id = Uuid::new_v5(&namespace, key.as_bytes());

            ids.push(chunk_id);
            stored.push(chunk.clone());
        }

        Ok(ids)
    }

    async fn search(
        &self,
        _query_embedding: Vec<f32>,
        limit: usize,
    ) -> IndexerResult<Vec<StorageSearchResult>> {
        if self.fail_on_search {
            return Err(crate::IndexerError::Storage(
                "Mock storage configured to fail".into(),
            ));
        }

        let stored = self.chunks.lock().unwrap();

        // Return up to 'limit' chunks with mock similarity scores
        let results: Vec<StorageSearchResult> = stored
            .iter()
            .take(limit)
            .enumerate()
            .map(|(i, chunk)| StorageSearchResult {
                chunk: chunk.clone(),
                // Mock decreasing similarity scores
                similarity: 1.0 - (i as f32 * 0.1),
            })
            .collect();

        Ok(results)
    }

    async fn delete_chunks(&self, chunk_ids: &[Uuid]) -> IndexerResult<()> {
        // In a real mock, we'd track chunks by ID and remove them
        // For simplicity, just clear all chunks when delete is called
        if !chunk_ids.is_empty() {
            let mut stored = self.chunks.lock().unwrap();
            stored.clear();
        }
        Ok(())
    }

    async fn collection_exists(&self) -> IndexerResult<bool> {
        Ok(*self.collection_exists.lock().unwrap())
    }

    async fn ensure_collection(&self) -> IndexerResult<()> {
        *self.collection_exists.lock().unwrap() = true;
        Ok(())
    }

    async fn drop_collection(&self) -> IndexerResult<bool> {
        let existed = *self.collection_exists.lock().unwrap();
        *self.collection_exists.lock().unwrap() = false;
        self.chunks.lock().unwrap().clear();
        Ok(existed)
    }

    async fn get_stats(&self) -> IndexerResult<StorageStats> {
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

        let count = storage.store_chunks(&chunks).await.unwrap();
        assert_eq!(count, 1);

        // Test search
        let results = storage.search(vec![0.1; 768], 10).await.unwrap();
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

        // Should fail to store
        assert!(storage.store_chunks(&chunks).await.is_err());

        let storage = MockStorage::new().with_search_failure();
        // Should fail to search
        assert!(storage.search(vec![0.1; 768], 10).await.is_err());
    }
}
