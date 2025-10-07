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
//! use codetriever_vector_data::{QdrantStorage, VectorStorage};
//! use codetriever_common::CorrelationId;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let storage = QdrantStorage::new(
//!     "http://localhost:6333".to_string(),
//!     "codetriever".to_string()
//! ).await?;
//! let query_embedding = vec![0.1, 0.2, 0.3]; // Example embedding
//! let correlation_id = CorrelationId::new();
//! let results = storage.search(query_embedding, 10, &correlation_id).await?;
//! # Ok(())
//! # }
//! ```

use crate::{
    VectorDataError, VectorDataResult,
    storage::{CodeChunk, StorageSearchResult, VectorStorage},
};
use anyhow::Context;
use async_trait::async_trait;
use codetriever_common::CorrelationId;
use codetriever_meta_data::generate_chunk_id;
use qdrant_client::qdrant::{
    CollectionExistsRequest, CreateCollection, DeleteCollection, Distance, PointId, PointStruct,
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
    /// Returns `Error::Storage` if:
    /// - Cannot connect to Qdrant server
    /// - Collection creation fails
    /// - Server returns error response
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use codetriever_vector_data::QdrantStorage;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let storage = QdrantStorage::new("http://localhost:6334".to_string(), "code_embeddings".to_string()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn new(url: String, collection_name: String) -> VectorDataResult<Self> {
        // Connect to Qdrant server
        // Note: The message "Failed to obtain server version" is expected and can be ignored
        // as it's just a compatibility check that may fail with certain network configurations

        // Check for API key from environment
        let mut builder = Qdrant::from_url(&url);

        // If QDRANT_API_KEY is set, use it for authentication
        if let Ok(api_key) = std::env::var("QDRANT_API_KEY") {
            builder = builder.api_key(api_key);
        }

        let client = builder.build().map_err(|e| {
            VectorDataError::Storage(format!("Failed to create Qdrant client: {e}"))
        })?;

        let storage = Self {
            client,
            collection_name: collection_name.clone(),
        };

        // Ensure collection exists with proper vector configuration
        storage.ensure_collection().await?;

        Ok(storage)
    }
}

#[async_trait]
impl VectorStorage for QdrantStorage {
    /// Checks if the collection exists.
    ///
    /// # Returns
    ///
    /// Ok(true) if the collection exists, Ok(false) if it doesn't exist
    ///
    /// # Errors
    ///
    /// Returns error if the check fails for reasons other than non-existence.
    #[tracing::instrument(skip(self))]
    async fn collection_exists(&self) -> VectorDataResult<bool> {
        let request = CollectionExistsRequest {
            collection_name: self.collection_name.clone(),
        };

        match self.client.collection_exists(request).await {
            Ok(response) => Ok(response),
            Err(e) => Err(VectorDataError::Storage(format!(
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
    async fn ensure_collection(&self) -> VectorDataResult<()> {
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
            Err(e) => {
                // Handle race condition: collection created by another process/thread
                let err_msg = e.to_string();
                if err_msg.contains("already exists") {
                    // Collection exists - this is fine (idempotent operation)
                    Ok(())
                } else {
                    Err(VectorDataError::Storage(format!(
                        "Failed to create collection '{}': {e}",
                        self.collection_name
                    )))
                }
            }
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
    async fn drop_collection(&self) -> VectorDataResult<bool> {
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
            Err(e) => Err(VectorDataError::Storage(format!(
                "Failed to drop collection '{}': {e}",
                self.collection_name
            ))),
        }
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
    /// Returns `Error::Storage` if:
    /// - Vector database is unreachable
    /// - Batch insertion fails
    /// - Invalid vector dimensions
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use codetriever_vector_data::{QdrantStorage, VectorStorage, CodeChunk};
    /// use codetriever_common::CorrelationId;
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
    ///         byte_start: 0,
    ///         byte_end: 12,
    ///         kind: Some("function".to_string()),
    ///         language: "rust".to_string(),
    ///         name: Some("main".to_string()),
    ///         token_count: Some(5),
    ///         embedding: Some(vec![0.1; 768]), // 768-dim embedding
    ///     }
    /// ];
    /// let correlation_id = CorrelationId::new();
    /// let stored_ids = storage.store_chunks("repo", "main", &chunks, 1, &correlation_id).await?;
    /// # Ok(())
    /// # }
    /// ```
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
    /// Returns `Error::Storage` if:
    /// - The vector database is unreachable
    /// - The query vector has incorrect dimensions (must be 768)
    /// - The collection doesn't exist or is corrupted
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use codetriever_vector_data::{QdrantStorage, VectorStorage};
    /// use codetriever_common::CorrelationId;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let storage = QdrantStorage::new("http://localhost:6334".to_string(), "test".to_string()).await?;
    /// let query_vector = vec![0.1; 768]; // 768-dimensional query embedding
    /// let correlation_id = CorrelationId::new();
    /// let results = storage.search(query_vector, 5, &correlation_id).await?;
    ///
    /// for result in results {
    ///     println!("Found: {} (lines {}-{}) - score: {:.3}",
    ///         result.chunk.file_path, result.chunk.start_line, result.chunk.end_line, result.similarity);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(skip(self, query), fields(query_dim = query.len(), limit))]
    async fn search(
        &self,
        query: Vec<f32>,
        limit: usize,
        correlation_id: &CorrelationId,
    ) -> VectorDataResult<Vec<StorageSearchResult>> {
        // Log search operation with correlation ID for tracing
        tracing::info!(
            correlation_id = %correlation_id,
            query_dim = query.len(),
            limit = %limit,
            collection = %self.collection_name,
            "Performing vector search"
        );

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
            .map_err(|e| VectorDataError::Storage(format!("Search failed: {e}")))?;

        let mut results = Vec::new();

        for scored_point in search_result.result {
            let payload = &scored_point.payload;
            let similarity = scored_point.score; // Extract the actual similarity score!

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

            // Extract byte ranges from payload
            let byte_start = payload
                .get("byte_start")
                .and_then(|v| v.as_integer())
                .map(|v| v as usize)
                .unwrap_or(0);

            let byte_end = payload
                .get("byte_end")
                .and_then(|v| v.as_integer())
                .map(|v| v as usize)
                .unwrap_or(content.len());

            let kind = payload
                .get("kind")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let language = payload
                .get("language")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default();

            let name = payload
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let token_count = payload
                .get("token_count")
                .and_then(|v| v.as_integer())
                .map(|v| v as usize);

            results.push(StorageSearchResult {
                chunk: CodeChunk {
                    file_path: file_path.clone(),
                    content,
                    start_line,
                    end_line,
                    byte_start,
                    byte_end,
                    kind,
                    language,
                    name,
                    token_count,
                    embedding: None, // Don't return embeddings to save memory
                },
                similarity, // Use the actual score from Qdrant!
            });
        }

        Ok(results)
    }

    /// Store chunks with deterministic IDs based on repository, branch, file, and generation
    #[tracing::instrument(skip(self, chunks), fields(repository_id, branch, chunk_count = chunks.len(), generation))]
    async fn store_chunks(
        &self,
        repository_id: &str,
        branch: &str,
        chunks: &[CodeChunk],
        generation: i64,
        correlation_id: &CorrelationId,
    ) -> VectorDataResult<Vec<uuid::Uuid>> {
        let mut points = Vec::new();
        let mut chunk_ids = Vec::new();

        // Convert chunks to Qdrant points with deterministic IDs
        for (chunk_index, chunk) in chunks.iter().enumerate() {
            if let Some(ref embedding) = chunk.embedding {
                // Use proper chunk ID generation from meta-data crate
                let chunk_id = generate_chunk_id(
                    repository_id,
                    branch,
                    &chunk.file_path,
                    generation,
                    chunk.byte_start,
                    chunk.byte_end,
                );

                chunk_ids.push(chunk_id);

                let mut payload = HashMap::new();
                payload.insert("chunk_id".to_string(), Value::from(chunk_id.to_string()));
                payload.insert(
                    "repository_id".to_string(),
                    Value::from(repository_id.to_string()),
                );
                payload.insert("branch".to_string(), Value::from(branch.to_string()));
                payload.insert("generation".to_string(), Value::from(generation));
                payload.insert("chunk_index".to_string(), Value::from(chunk_index as i64));
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

                // Add byte range information
                payload.insert(
                    "byte_start".to_string(),
                    Value::from(chunk.byte_start as i64),
                );
                payload.insert("byte_end".to_string(), Value::from(chunk.byte_end as i64));

                payload.insert("language".to_string(), Value::from(chunk.language.clone()));

                // Store optional fields
                if let Some(ref kind) = chunk.kind {
                    payload.insert("kind".to_string(), Value::from(kind.clone()));
                }
                if let Some(ref name) = chunk.name {
                    payload.insert("name".to_string(), Value::from(name.clone()));
                }
                if let Some(token_count) = chunk.token_count {
                    payload.insert("token_count".to_string(), Value::from(token_count as i64));
                }

                // Use UUID string as the point ID for Qdrant
                points.push(PointStruct::new(
                    chunk_id.to_string(),
                    embedding.clone(),
                    Payload::from(payload),
                ));
            }
        }

        if points.is_empty() {
            return Ok(Vec::new());
        }

        // Log operation with correlation ID for tracing
        tracing::info!(
            correlation_id = %correlation_id,
            repository_id = %repository_id,
            branch = %branch,
            generation = %generation,
            chunk_count = chunks.len(),
            "Storing chunks with deterministic IDs"
        );

        // Batch upsert points using new API
        let upsert_request = UpsertPoints {
            collection_name: self.collection_name.clone(),
            points,
            ..Default::default()
        };

        self.client
            .upsert_points(upsert_request)
            .await
            .map_err(|e| VectorDataError::Storage(format!("Failed to store chunks: {e}")))?;

        Ok(chunk_ids)
    }

    /// Delete chunks from Qdrant by their IDs
    async fn delete_chunks(&self, chunk_ids: &[uuid::Uuid]) -> VectorDataResult<()> {
        if chunk_ids.is_empty() {
            return Ok(());
        }

        // Convert UUIDs to strings for Qdrant PointId
        let point_ids: Vec<PointId> = chunk_ids
            .iter()
            .map(|id| PointId::from(id.to_string()))
            .collect();

        // Delete points from Qdrant using the correct API
        use qdrant_client::qdrant::{DeletePoints, PointsIdsList};

        let delete_request = DeletePoints {
            collection_name: self.collection_name.clone(),
            points: Some(qdrant_client::qdrant::PointsSelector {
                points_selector_one_of: Some(
                    qdrant_client::qdrant::points_selector::PointsSelectorOneOf::Points(
                        PointsIdsList { ids: point_ids },
                    ),
                ),
            }),
            ..Default::default()
        };

        self.client
            .delete_points(delete_request)
            .await
            .context("Failed to delete chunks from Qdrant")?;

        Ok(())
    }

    async fn get_stats(&self) -> VectorDataResult<crate::storage::StorageStats> {
        // Get collection info from Qdrant
        use qdrant_client::qdrant::GetCollectionInfoRequest;

        let request = GetCollectionInfoRequest {
            collection_name: self.collection_name.clone(),
        };

        let info = self
            .client
            .collection_info(request)
            .await
            .context("Failed to get collection info")?;

        let result = info
            .result
            .ok_or_else(|| VectorDataError::Other("Missing collection info result".into()))?;

        Ok(crate::storage::StorageStats {
            vector_count: result.vectors_count.unwrap_or(0) as usize,
            storage_bytes: Some(result.payload_schema.len() as u64), // Approximation
            collection_name: self.collection_name.clone(),
            storage_type: "qdrant".to_string(),
        })
    }
}
