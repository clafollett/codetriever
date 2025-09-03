//! Qdrant vector database storage backend for semantic code search.
//!
//! This module provides the [`QdrantStorage`] client for storing and retrieving
//! code embeddings using Qdrant's vector database. It implements the storage
//! layer for the codetriever system, enabling semantic search across codebases
//! through high-dimensional vector similarity matching.
//!
//! # Storage Strategy
//!
//! The storage layer uses Qdrant to:
//! - Store high-dimensional embeddings (vectors) of code chunks
//! - Associate metadata with each vector (file path, function name, etc.)
//! - Perform fast approximate nearest neighbor search for semantic similarity
//! - Scale horizontally for large codebases
//!
//! # Example
//!
//! ```rust,no_run
//! use codetriever_indexer::storage::QdrantStorage;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let storage = QdrantStorage::new(
//!     "http://localhost:6333".to_string(),
//!     "codetriever".to_string()
//! ).await?;
//! let query_embedding = vec![0.1, 0.2, 0.3]; // Example embedding
//! let results = storage.search(query_embedding, 10).await?;
//! # Ok(())
//! # }
//! ```

use crate::{Error, Result, indexing::CodeChunk};
use qdrant_client::qdrant::{
    CollectionExistsRequest, CreateCollection, DeleteCollection, Distance, PointStruct,
    SearchPoints, UpsertPoints, Value, VectorParams,
};
use qdrant_client::{Payload, Qdrant};
use std::collections::HashMap;

/// Vector database client for storing and searching code embeddings using Qdrant.
///
/// `QdrantStorage` serves as the primary interface to the Qdrant vector database,
/// handling the storage and retrieval of code embeddings for semantic search.
/// It abstracts away the complexities of vector database operations while
/// providing a clean API for the indexing and search pipeline.
///
/// # Architecture
///
/// The client is designed to be:
/// - **Async-first**: All operations are non-blocking
/// - **Lightweight**: Minimal overhead for high-performance search
/// - **Scalable**: Can handle large codebases efficiently
/// - **Fault-tolerant**: Graceful error handling and recovery
///
/// # Future Implementation
///
/// Currently a stub implementation. Will include:
/// - Qdrant gRPC client connection
/// - Collection management for different projects
/// - Batch operations for efficient indexing
/// - Connection pooling and retry logic
#[derive(Clone)]
pub struct QdrantStorage {
    client: Qdrant,
    collection_name: String,
}

impl QdrantStorage {
    /// Creates a new `QdrantStorage` client instance and ensures collection exists.
    ///
    /// Initializes the Qdrant storage client with the given URL and collection name.
    /// Automatically creates the collection with proper vector configuration if it doesn't exist.
    /// Configured for 768-dimensional Jina embeddings with cosine similarity.
    ///
    /// # Parameters
    ///
    /// * `url` - Qdrant server URL (e.g., "http://localhost:6334")
    /// * `collection_name` - Name of the collection to store vectors in
    ///
    /// # Returns
    ///
    /// A new `QdrantStorage` instance ready for vector operations.
    ///
    /// # Errors
    ///
    /// Returns `Error::Qdrant` if:
    /// - Cannot connect to Qdrant server
    /// - Collection creation fails
    /// - Server returns error response
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use codetriever_indexer::storage::QdrantStorage;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let storage = QdrantStorage::new("http://localhost:6334".to_string(), "code_embeddings".to_string()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(url: String, collection_name: String) -> Result<Self> {
        // Connect to Qdrant server
        let client = Qdrant::from_url(&url)
            .build()
            .map_err(|e| Error::Storage(format!("Failed to create Qdrant client: {e}")))?;

        let storage = Self {
            client,
            collection_name: collection_name.clone(),
        };

        // Ensure collection exists with proper vector configuration
        storage.ensure_collection_exists().await?;

        Ok(storage)
    }

    /// Checks if the collection exists.
    ///
    /// # Returns
    ///
    /// Ok(true) if the collection exists, Ok(false) if it doesn't exist
    ///
    /// # Errors
    ///
    /// Returns error if the check fails for reasons other than non-existence.
    pub async fn collection_exists(&self) -> Result<bool> {
        let request = CollectionExistsRequest {
            collection_name: self.collection_name.clone(),
        };

        match self.client.collection_exists(request).await {
            Ok(response) => Ok(response),
            Err(e) => Err(crate::Error::Storage(format!(
                "Failed to check collection exists: {e}"
            ))),
        }
    }

    /// Creates the collection with proper vector configuration.
    ///
    /// # Returns
    ///
    /// Ok(()) if the collection was created successfully
    ///
    /// # Errors
    ///
    /// Returns error if the collection creation fails for reasons other than non-existence.
    pub async fn create_collection(&self) -> Result<()> {
        if self.collection_exists().await? {
            return Ok(());
        }

        let request = CreateCollection {
            collection_name: self.collection_name.clone(),
            vectors_config: Some(
                VectorParams {
                    size: 768, // Jina BERT v2 embedding dimensions
                    distance: Distance::Cosine as i32,
                    ..Default::default()
                }
                .into(),
            ),
            ..Default::default()
        };

        match self.client.create_collection(request).await {
            Ok(_) => Ok(()),
            Err(e) => Err(crate::Error::Storage(format!(
                "Failed to create collection '{}': {e}",
                self.collection_name
            ))),
        }
    }

    /// Drops the collection if it exists.
    ///
    /// Completely removes the collection and all its data from Qdrant.
    /// Useful for resetting the index or cleaning up test data.
    ///
    /// # Returns
    ///
    /// Ok(true) if collection was dropped, Ok(false) if it didn't exist
    ///
    /// # Errors
    ///
    /// Returns error if the drop operation fails for reasons other than non-existence.
    pub async fn drop_collection(&self) -> Result<bool> {
        if !self.collection_exists().await? {
            return Ok(false);
        }

        // Drop the collection
        let request = DeleteCollection {
            collection_name: self.collection_name.clone(),
            ..Default::default()
        };

        match self.client.delete_collection(request).await {
            Ok(_) => {
                println!("Dropped collection '{}'", self.collection_name);
                Ok(true)
            }
            Err(e) => Err(crate::Error::Storage(format!(
                "Failed to drop collection '{}': {e}",
                self.collection_name
            ))),
        }
    }

    /// Ensures that the collection exists with proper vector configuration.
    ///
    /// Creates a collection with 768-dimensional vectors using cosine similarity
    /// if it doesn't already exist. This is called automatically during initialization.
    ///
    /// # Errors
    ///
    /// Returns `Error::Qdrant` if collection creation fails or server is unreachable.
    async fn ensure_collection_exists(&self) -> Result<()> {
        if self.collection_exists().await? {
            return Ok(());
        }

        self.create_collection().await?;

        Ok(())
    }

    /// Stores code chunks with their embeddings in the vector database.
    ///
    /// Performs batch insertion of code chunks into Qdrant. Each chunk is stored
    /// as a point with its embedding vector and metadata (file path, content, line numbers).
    /// Only chunks with embeddings are stored - chunks without embeddings are skipped.
    ///
    /// # Parameters
    ///
    /// * `chunks` - Slice of CodeChunk instances to store
    ///
    /// # Returns
    ///
    /// Number of chunks successfully stored (excludes chunks without embeddings).
    ///
    /// # Errors
    ///
    /// Returns `Error::Qdrant` if:
    /// - Vector database is unreachable
    /// - Batch insertion fails
    /// - Invalid vector dimensions
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use codetriever_indexer::{storage::QdrantStorage, indexing::CodeChunk};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let storage = QdrantStorage::new("http://localhost:6334".to_string(), "test".to_string()).await?;
    /// let chunks = vec![
    ///     CodeChunk {
    ///         file_path: "main.rs".to_string(),
    ///         content: "fn main() {}".to_string(),
    ///         start_line: 1,
    ///         end_line: 1,
    ///         embedding: Some(vec![0.1; 768]), // 768-dim embedding
    ///     }
    /// ];
    /// let stored_count = storage.store_chunks(&chunks).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn store_chunks(&self, chunks: &[CodeChunk]) -> Result<usize> {
        let mut points = Vec::new();
        let mut point_id = 0u64;

        // Convert chunks to Qdrant points
        for chunk in chunks {
            if let Some(ref embedding) = chunk.embedding {
                let mut payload = HashMap::new();
                payload.insert(
                    "file_path".to_string(),
                    Value::from(chunk.file_path.clone()),
                );
                payload.insert("content".to_string(), Value::from(chunk.content.clone()));
                payload.insert(
                    "start_line".to_string(),
                    Value::from(chunk.start_line as i64),
                );
                payload.insert("end_line".to_string(), Value::from(chunk.end_line as i64));

                points.push(PointStruct::new(
                    point_id,
                    embedding.clone(),
                    Payload::from(payload),
                ));
                point_id += 1;
            }
        }

        if points.is_empty() {
            return Ok(0);
        }

        // Batch upsert points using new API
        let upsert_request = UpsertPoints {
            collection_name: self.collection_name.clone(),
            points,
            ..Default::default()
        };

        self.client
            .upsert_points(upsert_request)
            .await
            .map_err(|e| Error::Storage(format!("Failed to store chunks: {e}")))?;

        Ok(point_id as usize)
    }

    /// Performs semantic similarity search using vector embeddings.
    ///
    /// Searches for the most similar code chunks to the provided query vector
    /// using approximate nearest neighbor search with cosine similarity.
    /// Returns actual CodeChunk instances reconstructed from stored metadata.
    ///
    /// # Parameters
    ///
    /// * `query` - The query embedding vector (must be 768 dimensions for Jina)
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    ///
    /// Vector of CodeChunk instances ordered by similarity score (most similar first).
    /// The `embedding` field in returned chunks will be None to save memory.
    ///
    /// # Errors
    ///
    /// Returns `Error::Qdrant` if:
    /// - The vector database is unreachable
    /// - The query vector has incorrect dimensions (must be 768)
    /// - The collection doesn't exist or is corrupted
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use codetriever_indexer::storage::QdrantStorage;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let storage = QdrantStorage::new("http://localhost:6334".to_string(), "test".to_string()).await?;
    /// let query_vector = vec![0.1; 768]; // 768-dimensional query embedding
    /// let results = storage.search(query_vector, 5).await?;
    ///
    /// for chunk in results {
    ///     println!("Found: {} (lines {}-{})", chunk.file_path, chunk.start_line, chunk.end_line);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn search(&self, query: Vec<f32>, limit: usize) -> Result<Vec<CodeChunk>> {
        // Validate query vector dimensions
        if query.len() != 768 {
            return Err(Error::Storage(format!(
                "Query vector must be 768 dimensions, got {}",
                query.len()
            )));
        }

        let search_request = SearchPoints {
            collection_name: self.collection_name.clone(),
            vector: query,
            limit: limit as u64,
            with_payload: Some(true.into()),
            ..Default::default()
        };

        let search_result = self
            .client
            .search_points(search_request)
            .await
            .map_err(|e| Error::Storage(format!("Search failed: {e}")))?;

        let mut results = Vec::new();

        for scored_point in search_result.result {
            let payload = &scored_point.payload;

            // Extract metadata from payload
            let file_path = payload
                .get("file_path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default();

            let content = payload
                .get("content")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default();

            let start_line = payload
                .get("start_line")
                .and_then(|v| v.as_integer())
                .unwrap_or(0) as usize;

            let end_line = payload
                .get("end_line")
                .and_then(|v| v.as_integer())
                .unwrap_or(0) as usize;

            results.push(CodeChunk {
                file_path,
                content,
                start_line,
                end_line,
                embedding: None, // Don't return embeddings to save memory
            });
        }

        Ok(results)
    }
}
