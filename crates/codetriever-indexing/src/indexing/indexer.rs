use crate::{
    IndexerResult,
    queues::{ChunkQueue, FileContentQueue},
};
use codetriever_common::CorrelationId;
use codetriever_embeddings::EmbeddingService;
use codetriever_parsing::CodeChunk as ParsingCodeChunk;
use codetriever_parsing::{CodeParser, get_language_from_extension};
use codetriever_vector_data::{CodeChunk, VectorStorage};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

// Type alias for the repository trait object
type RepositoryRef = Arc<dyn codetriever_meta_data::traits::FileRepository>;

/// Result of encoding detection and conversion
struct EncodingResult {
    content: String,
    encoding_name: String,
}

/// Detect file encoding and convert to UTF-8
///
/// Returns None for binary files that can't be represented as text
fn detect_and_convert_to_utf8(content: &str) -> Option<EncodingResult> {
    let bytes = content.as_bytes();

    // Check for NULL bytes (binary files)
    if bytes.contains(&0) {
        tracing::debug!("File contains NULL bytes - skipping as binary");
        return None;
    }

    // Fast path: Check if already valid UTF-8
    if std::str::from_utf8(bytes).is_ok() {
        return Some(EncodingResult {
            content: content.to_string(),
            encoding_name: "UTF-8".to_string(),
        });
    }

    // Try auto-detection with encoding_rs
    let (encoding, _bom_length) =
        encoding_rs::Encoding::for_bom(bytes).unwrap_or((encoding_rs::UTF_8, 0));

    let (decoded, actual_encoding, malformed) = encoding.decode(bytes);

    // If decoding had errors, it's likely binary - skip it
    if malformed {
        tracing::debug!(
            "File appears to be binary (encoding errors detected with {})",
            actual_encoding.name()
        );
        return None;
    }

    // Successfully decoded
    Some(EncodingResult {
        content: decoded.into_owned(),
        encoding_name: actual_encoding.name().to_string(),
    })
}

/// Convert from parsing CodeChunk to vector data CodeChunk
fn convert_chunk(parsing_chunk: ParsingCodeChunk) -> CodeChunk {
    CodeChunk {
        file_path: parsing_chunk.file_path,
        content: parsing_chunk.content,
        start_line: parsing_chunk.start_line,
        end_line: parsing_chunk.end_line,
        byte_start: parsing_chunk.byte_start,
        byte_end: parsing_chunk.byte_end,
        kind: parsing_chunk.kind,
        language: parsing_chunk.language,
        name: parsing_chunk.name,
        token_count: parsing_chunk.token_count,
        embedding: parsing_chunk.embedding,
    }
}

type EmbeddingServiceRef = Arc<dyn EmbeddingService>;
type VectorStorageRef = Arc<dyn VectorStorage>;

/// Indexer for processing and storing code chunks
///
/// This struct implements IndexerService and handles job creation (not file processing).
/// File processing is handled by BackgroundWorker which uses the dependencies directly.
pub struct Indexer {
    embedding_service: EmbeddingServiceRef,
    storage: VectorStorageRef,
    repository: RepositoryRef,
}

/// Parser worker: pulls files from queue, parses them, pushes chunks to chunk queue
#[allow(clippy::too_many_arguments)]
pub async fn parser_worker(
    worker_id: usize,
    file_queue: Arc<dyn FileContentQueue>,
    chunk_queue: Arc<dyn ChunkQueue>,
    code_parser: Arc<CodeParser>,
    repository: RepositoryRef,
    tenant_id: uuid::Uuid,
    repository_id: String,
    branch: String,
    files_indexed: Arc<AtomicUsize>,
) -> IndexerResult<()> {
    tracing::debug!("Parser worker {worker_id} starting");

    loop {
        // Pull file from queue (blocks until available)
        let file = match file_queue.pop().await {
            Ok(f) => f,
            Err(_) => {
                tracing::debug!("Parser worker {worker_id}: queue closed, shutting down");
                break; // Queue closed - no more files
            }
        };

        tracing::debug!("Parser worker {worker_id}: processing {}", file.path);

        // Detect encoding and convert to UTF-8 (skip binary files)
        let encoding_result = match detect_and_convert_to_utf8(&file.content) {
            Some(result) => result,
            None => {
                tracing::warn!(
                    "Parser worker {worker_id}: skipping binary file {}",
                    file.path
                );
                files_indexed.fetch_add(1, Ordering::Relaxed); // Count as processed
                continue;
            }
        };

        // Use converted UTF-8 content for hashing and indexing
        let utf8_content = encoding_result.content;
        let detected_encoding = encoding_result.encoding_name;

        // Check file state in database
        let content_hash = codetriever_meta_data::hash_content(&utf8_content);
        let state = repository
            .check_file_state(
                &tenant_id,
                &repository_id,
                &branch,
                &file.path,
                &content_hash,
            )
            .await?;

        let generation = match state {
            codetriever_meta_data::models::FileState::Unchanged => {
                tracing::debug!(
                    "Parser worker {worker_id}: skipping unchanged file {}",
                    file.path
                );
                continue;
            }
            codetriever_meta_data::models::FileState::New { generation }
            | codetriever_meta_data::models::FileState::Updated {
                new_generation: generation,
                ..
            } => {
                // Record file indexing with detected encoding
                #[allow(clippy::cast_possible_wrap)]
                let size_bytes = file.content.len() as i64; // Original size before UTF-8 conversion
                let metadata = codetriever_meta_data::models::FileMetadata {
                    path: file.path.clone(),
                    content: utf8_content.clone(), // Converted to UTF-8
                    content_hash: content_hash.clone(),
                    encoding: detected_encoding.clone(),
                    size_bytes,
                    generation,
                    commit_sha: None,
                    commit_message: None,
                    commit_date: None,
                    author: None,
                };
                repository
                    .record_file_indexing(&tenant_id, &repository_id, &branch, &metadata)
                    .await?;

                // For updated files, delete old chunks
                if matches!(
                    state,
                    codetriever_meta_data::models::FileState::Updated { .. }
                ) {
                    let deleted_ids = repository
                        .replace_file_chunks(
                            &tenant_id,
                            &repository_id,
                            &branch,
                            &file.path,
                            generation,
                        )
                        .await?;
                    tracing::debug!(
                        "Parser worker {worker_id}: deleted {} old chunks",
                        deleted_ids.len()
                    );
                }

                generation
            }
        };

        // Parse file into chunks (use UTF-8 converted content)
        let ext = file.path.rsplit('.').next().unwrap_or("");
        let language = get_language_from_extension(ext).unwrap_or(ext);
        let chunks = code_parser.parse(&utf8_content, language, &file.path)?;

        if !chunks.is_empty() {
            files_indexed.fetch_add(1, Ordering::Relaxed);
            tracing::info!(
                "Parser worker {worker_id}: parsed {} chunks from {} (gen={})",
                chunks.len(),
                file.path,
                generation
            );

            // Wrap chunks with metadata (generation and file-specific index)
            let chunks_with_metadata: Vec<crate::queues::ChunkWithMetadata> = chunks
                .into_iter()
                .enumerate()
                .map(|(idx, chunk)| crate::queues::ChunkWithMetadata {
                    chunk,
                    generation,
                    file_chunk_index: idx,
                })
                .collect();

            // Push to chunk queue (blocks if queue full - back pressure!)
            chunk_queue.push_batch(chunks_with_metadata).await?;
        } else {
            tracing::warn!(
                "Parser worker {worker_id}: {} produced zero chunks",
                file.path
            );
        }
    }

    tracing::debug!("Parser worker {worker_id} shutting down");
    Ok(())
}

/// Embedding worker: pulls chunk batches, embeds them, stores in Qdrant + Postgres
#[allow(clippy::too_many_arguments)]
pub async fn embedding_worker(
    worker_id: usize,
    chunk_queue: Arc<dyn ChunkQueue>,
    embedding_service: EmbeddingServiceRef,
    storage: VectorStorageRef,
    repository: RepositoryRef,
    tenant_id: uuid::Uuid,
    repository_id: String,
    branch: String,
    chunk_batch_size: usize,
    chunks_created: Arc<AtomicUsize>,
    chunks_stored: Arc<AtomicUsize>,
) -> IndexerResult<()> {
    tracing::debug!("Embedding worker {worker_id} starting");

    loop {
        // Pull batch of chunks from queue (blocks until available)
        let mut chunk_batch = match chunk_queue.pop_batch(chunk_batch_size).await {
            Ok(batch) => batch,
            Err(_) => {
                tracing::debug!("Embedding worker {worker_id}: queue closed, shutting down");
                break; // Queue closed - no more chunks
            }
        };

        let batch_size = chunk_batch.len();
        tracing::debug!("Embedding worker {worker_id}: got {batch_size} chunks");

        chunks_created.fetch_add(batch_size, Ordering::Relaxed);

        // Generate embeddings for this batch
        let texts: Vec<&str> = chunk_batch
            .iter()
            .map(|c| c.chunk.content.as_str())
            .collect();
        let embeddings = embedding_service.generate_embeddings(texts).await?;

        // Apply embeddings to the underlying chunks
        for (chunk_with_meta, embedding) in chunk_batch.iter_mut().zip(embeddings.into_iter()) {
            chunk_with_meta.chunk.embedding = Some(embedding);
        }

        // Convert to vector data chunks (extract the actual CodeChunk)
        let vector_chunks: Vec<CodeChunk> = chunk_batch
            .iter()
            .map(|c| convert_chunk(c.chunk.clone()))
            .collect();

        // Store in Qdrant with full metadata context
        let storage_context = codetriever_vector_data::ChunkStorageContext {
            tenant_id,
            repository_id: repository_id.clone(),
            branch: branch.clone(),
            generation: 1, // TODO: Track generation through queue
            repository_url: None,
            commit_sha: None,
            commit_message: None,
            commit_date: None,
            author: None,
        };

        let correlation_id = CorrelationId::new();
        let chunk_ids = storage
            .store_chunks(&storage_context, &vector_chunks, &correlation_id)
            .await?;

        // Store metadata in Postgres
        // Use the generation and file_chunk_index from ChunkWithMetadata
        let chunk_metadata: Vec<codetriever_meta_data::models::ChunkMetadata> = chunk_batch
            .iter()
            .zip(chunk_ids.iter())
            .map(
                |(chunk_with_meta, chunk_id)| codetriever_meta_data::models::ChunkMetadata {
                    chunk_id: *chunk_id,
                    tenant_id,
                    repository_id: repository_id.clone(),
                    branch: branch.clone(),
                    file_path: chunk_with_meta.chunk.file_path.clone(),
                    chunk_index: chunk_with_meta.file_chunk_index as i32,
                    generation: chunk_with_meta.generation,
                    start_line: chunk_with_meta.chunk.start_line as i32,
                    end_line: chunk_with_meta.chunk.end_line as i32,
                    byte_start: chunk_with_meta.chunk.byte_start as i64,
                    byte_end: chunk_with_meta.chunk.byte_end as i64,
                    kind: chunk_with_meta.chunk.kind.clone(),
                    name: chunk_with_meta.chunk.name.clone(),
                    created_at: chrono::Utc::now(),
                },
            )
            .collect();

        repository
            .insert_chunks(&tenant_id, &repository_id, &branch, chunk_metadata)
            .await?;

        chunks_stored.fetch_add(batch_size, Ordering::Relaxed);
        tracing::debug!("Embedding worker {worker_id}: stored {batch_size} chunks");
    }

    tracing::debug!("Embedding worker {worker_id} shutting down");
    Ok(())
}

impl Indexer {
    /// Creates a new indexer with required dependencies.
    ///
    /// This creates an IndexerService implementation that handles job creation.
    /// For file processing, use BackgroundWorker with the same dependencies.
    ///
    /// # Arguments
    ///
    /// * `embedding_service` - Service for generating embeddings (shared with SearchService)
    /// * `vector_storage` - Qdrant storage backend (shared with SearchService)
    /// * `repository` - PostgreSQL repository for job tracking
    pub fn new(
        embedding_service: Arc<dyn EmbeddingService>,
        vector_storage: Arc<dyn VectorStorage>,
        repository: Arc<dyn codetriever_meta_data::traits::FileRepository>,
    ) -> Self {
        Self {
            embedding_service,
            storage: vector_storage,
            repository,
        }
    }

    /// Drop the collection from storage
    pub async fn drop_collection(&mut self) -> IndexerResult<bool> {
        // Storage is always available (required dependency)
        Ok(self.storage.drop_collection().await?)
    }

    /// Get reference to embedding service (for SearchService)
    pub fn embedding_service(&self) -> Arc<dyn EmbeddingService> {
        Arc::clone(&self.embedding_service)
    }

    /// Get reference to vector storage (for SearchService)
    pub fn vector_storage(&self) -> VectorStorageRef {
        Arc::clone(&self.storage)
    }
}

// Implement IndexerService trait for Indexer to allow it to be used directly in API
use super::service::{FileContent as ServiceFileContent, IndexerService};
use async_trait::async_trait;

#[async_trait]
impl IndexerService for Indexer {
    async fn start_indexing_job(
        &self,
        tenant_id: uuid::Uuid,
        project_id: &str,
        files: Vec<ServiceFileContent>,
    ) -> crate::IndexerResult<uuid::Uuid> {
        // Parse project_id as "repository_id:branch"
        let (repository_id, branch) = project_id.split_once(':').unwrap_or((project_id, "main"));

        // Ensure project branch exists (required for FK constraint)
        let ctx = codetriever_meta_data::models::RepositoryContext {
            tenant_id,
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
            repository_url: None,
            commit_sha: None,
            commit_message: None,
            commit_date: None,
            author: None,
            is_dirty: false,
            root_path: std::path::PathBuf::from("."),
        };
        self.repository.ensure_project_branch(&ctx).await?;

        // Create job in database
        let job = self
            .repository
            .create_indexing_job(&tenant_id, repository_id, branch, None)
            .await?;

        // Enqueue all files to persistent queue (skip binary files)
        for file in files {
            // Skip files with null bytes (binary files) - PostgreSQL text columns reject them
            if file.content.as_bytes().contains(&0) {
                tracing::debug!("Skipping binary file {} (contains null bytes)", file.path);
                continue;
            }

            self.repository
                .enqueue_file(
                    &job.job_id,
                    &tenant_id,
                    repository_id,
                    branch,
                    &file.path,
                    &file.content,
                    &file.hash,
                )
                .await?;
        }

        Ok(job.job_id)
    }

    async fn get_job_status(
        &self,
        job_id: &uuid::Uuid,
    ) -> crate::IndexerResult<Option<codetriever_meta_data::models::IndexingJob>> {
        Ok(self.repository.get_indexing_job(job_id).await?)
    }

    async fn list_jobs(
        &self,
        tenant_id: Option<uuid::Uuid>,
        project_id: Option<&str>,
    ) -> crate::IndexerResult<Vec<codetriever_meta_data::models::IndexingJob>> {
        Ok(self
            .repository
            .list_indexing_jobs(tenant_id.as_ref(), project_id)
            .await?)
    }

    async fn drop_collection(&mut self) -> crate::IndexerResult<bool> {
        self.drop_collection().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::service::FileContent;
    use async_trait::async_trait;
    use codetriever_embeddings::{
        EmbeddingProvider, EmbeddingResult, EmbeddingService, EmbeddingStats,
    };
    use codetriever_meta_data::{mock::MockFileRepository, traits::FileRepository};
    use codetriever_vector_data::MockStorage;
    use std::sync::Arc;

    /// Mock embedding service that doesn't require GPU or model downloads
    pub struct MockEmbeddingService;

    #[async_trait]
    impl EmbeddingService for MockEmbeddingService {
        async fn generate_embeddings(&self, texts: Vec<&str>) -> EmbeddingResult<Vec<Vec<f32>>> {
            // Return mock embeddings - deterministic but fake
            Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3, 0.4]).collect())
        }

        fn provider(&self) -> &dyn EmbeddingProvider {
            &MockEmbeddingProvider
        }

        async fn get_stats(&self) -> EmbeddingStats {
            EmbeddingStats::default()
        }
    }

    /// Mock embedding provider for unit tests
    pub struct MockEmbeddingProvider;

    #[async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed_batch(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3, 0.4]).collect())
        }

        fn embedding_dimension(&self) -> usize {
            4
        }

        fn max_tokens(&self) -> usize {
            8192
        }

        fn model_name(&self) -> &str {
            "mock-test-model"
        }

        async fn is_ready(&self) -> bool {
            true
        }

        async fn ensure_ready(&self) -> EmbeddingResult<()> {
            // No-op for mock - always ready
            Ok(())
        }

        async fn get_tokenizer(&self) -> Option<std::sync::Arc<tokenizers::Tokenizer>> {
            // Mock doesn't provide tokenizer
            None
        }
    }

    // Test tenant for all unit tests
    const TEST_TENANT: uuid::Uuid = uuid::Uuid::nil();

    #[tokio::test]
    async fn test_indexer_service_creates_job() {
        // Arrange - Create mock repository, storage, and embedding service
        let mock_repo = Arc::new(MockFileRepository::new()) as Arc<dyn FileRepository>;
        let mock_storage = Arc::new(MockStorage::new()) as Arc<dyn VectorStorage>;
        let mock_embedding_service = Arc::new(MockEmbeddingService);

        // Create indexer with required dependencies
        let indexer = Indexer::new(mock_embedding_service, mock_storage, mock_repo.clone());

        // Act - Start an indexing job
        let content = r#"fn main() { println!("Hello, world!"); }"#;
        let file_content = FileContent {
            path: "src/main.rs".to_string(),
            content: content.to_string(),
            hash: codetriever_meta_data::hash_content(content),
        };

        let job_id = indexer
            .start_indexing_job(TEST_TENANT, "test_repo:main", vec![file_content])
            .await;

        // Assert - Job should be created
        assert!(job_id.is_ok());

        // Verify job was created in repository
        let job = mock_repo.get_indexing_job(&job_id.unwrap()).await.unwrap();
        assert!(job.is_some());
        assert_eq!(job.unwrap().repository_id, "test_repo");
    }

    // NOTE: File-level behavior tests (unchanged files, generation) have been moved
    // to integration tests and BackgroundWorker tests, as the Indexer struct now
    // only handles job creation (not file processing).
}
