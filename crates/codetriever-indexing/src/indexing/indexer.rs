use crate::{
    IndexerError, IndexerResult,
    indexing::service::FileContent,
    queues::{ChunkQueue, FileContentQueue},
};
use codetriever_common::CorrelationId;
use codetriever_config::ApplicationConfig;
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

// NOTE: File extension filtering removed - index_file_content() API assumes
// caller pre-filters files. For future directory indexing, add filtering at API layer.

type EmbeddingServiceRef = Arc<dyn EmbeddingService>;
type VectorStorageRef = Arc<dyn VectorStorage>;

#[derive(Debug)]
pub struct IndexResult {
    pub files_indexed: usize,
    pub chunks_created: usize,
    pub chunks_stored: usize, // Track how many were stored in Qdrant
}

/// Indexer for processing and storing code chunks
///
/// All dependencies (embedding, storage, repository, config) are REQUIRED
pub struct Indexer {
    embedding_service: EmbeddingServiceRef,
    storage: VectorStorageRef,
    repository: RepositoryRef,
    code_parser: Arc<CodeParser>,
    config: ApplicationConfig,
}

/// Parser worker: pulls files from queue, parses them, pushes chunks to chunk queue
#[allow(clippy::too_many_arguments)]
async fn parser_worker(
    worker_id: usize,
    file_queue: Arc<dyn FileContentQueue>,
    chunk_queue: Arc<dyn ChunkQueue>,
    code_parser: Arc<CodeParser>,
    repository: RepositoryRef,
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
            .check_file_state(&repository_id, &branch, &file.path, &content_hash)
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
                    .record_file_indexing(&repository_id, &branch, &metadata)
                    .await?;

                // For updated files, delete old chunks
                if matches!(
                    state,
                    codetriever_meta_data::models::FileState::Updated { .. }
                ) {
                    let deleted_ids = repository
                        .replace_file_chunks(&repository_id, &branch, &file.path, generation)
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
async fn embedding_worker(
    worker_id: usize,
    chunk_queue: Arc<dyn ChunkQueue>,
    embedding_service: EmbeddingServiceRef,
    storage: VectorStorageRef,
    repository: RepositoryRef,
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

        // Store in Qdrant
        // TODO: Track generation metadata through queue (currently hardcoded to 1)
        let correlation_id = CorrelationId::new();
        let chunk_ids = storage
            .store_chunks(&repository_id, &branch, &vector_chunks, 1, &correlation_id)
            .await?;

        // Store metadata in Postgres
        // Use the generation and file_chunk_index from ChunkWithMetadata
        let chunk_metadata: Vec<codetriever_meta_data::models::ChunkMetadata> = chunk_batch
            .iter()
            .zip(chunk_ids.iter())
            .map(
                |(chunk_with_meta, chunk_id)| codetriever_meta_data::models::ChunkMetadata {
                    chunk_id: *chunk_id,
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
            .insert_chunks(&repository_id, &branch, chunk_metadata)
            .await?;

        chunks_stored.fetch_add(batch_size, Ordering::Relaxed);
        tracing::debug!("Embedding worker {worker_id}: stored {batch_size} chunks");
    }

    tracing::debug!("Embedding worker {worker_id} shutting down");
    Ok(())
}

impl Indexer {
    /// Creates a new indexer with all required dependencies.
    ///
    /// All dependencies are REQUIRED - no defaults, no fallbacks.
    /// This ensures proper dependency injection and prevents orphaned resources.
    ///
    /// # Arguments
    ///
    /// * `embedding_service` - Service for generating embeddings (contains model pool)
    /// * `vector_storage` - Qdrant storage backend for chunk vectors
    /// * `repository` - PostgreSQL repository for metadata
    /// * `config` - Application configuration
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use codetriever_config::ApplicationConfig;
    /// use codetriever_indexing::indexing::Indexer;
    /// use codetriever_embeddings::DefaultEmbeddingService;
    /// use codetriever_vector_data::QdrantStorage;
    /// use codetriever_meta_data::DbFileRepository;
    /// use codetriever_parsing::CodeParser;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = ApplicationConfig::from_env();
    /// let embedding_service = Arc::new(DefaultEmbeddingService::new(config.embedding.clone()));
    /// let storage = Arc::new(QdrantStorage::new("http://localhost:6334".to_string(), "collection".to_string()).await?);
    /// // Note: DbFileRepository::new() requires a PoolManager - see production code for full setup
    /// # let pools = unimplemented!(); // Example only
    /// let repository = Arc::new(DbFileRepository::new(pools));
    /// let code_parser = CodeParser::default(); // Or load tokenizer for accurate chunking
    ///
    /// let indexer = Indexer::new(embedding_service, storage, repository, code_parser, &config);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(
        embedding_service: Arc<dyn EmbeddingService>,
        vector_storage: Arc<dyn VectorStorage>,
        repository: Arc<dyn codetriever_meta_data::traits::FileRepository>,
        code_parser: CodeParser,
        config: &ApplicationConfig,
    ) -> Self {
        Self {
            embedding_service,
            storage: vector_storage,
            repository,
            code_parser: Arc::new(code_parser),
            config: config.clone(),
        }
    }

    /// Index file content directly without filesystem access
    /// If repository is set, will check file state and skip unchanged files
    pub async fn index_file_content(
        &mut self,
        project_id: &str,
        files: Vec<FileContent>,
    ) -> IndexerResult<IndexResult> {
        tracing::info!(
            "📝 INDEX START: project={project_id}, files={}",
            files.len()
        );

        // Parse project_id to extract repository_id and branch if using database
        let (repository_id, branch) = if project_id.contains(':') {
            let parts: Vec<&str> = project_id.splitn(2, ':').collect();
            (
                parts[0].to_string(),
                parts.get(1).unwrap_or(&"main").to_string(),
            )
        } else {
            (project_id.to_string(), "main".to_string())
        };

        // Ensure embedding provider is ready
        tracing::info!("📝 Ensuring embedding provider ready...");
        self.embedding_service.provider().ensure_ready().await?;
        tracing::info!("✅ Embedding provider ready");

        // Log actual config values for diagnostics
        tracing::info!(
            "📊 Config: pool_size={}, batch_size={}, concurrency_limit={}, max_chunk_tokens={}",
            self.config.embedding.performance.pool_size,
            self.config.embedding.performance.indexer_batch_size,
            self.config.indexing.concurrency_limit,
            self.config.indexing.max_chunk_tokens
        );

        // Ensure project branch exists (repository is always available as required dependency)
        let ctx = codetriever_meta_data::models::RepositoryContext {
            repository_id: repository_id.clone(),
            branch: branch.clone(),
            repository_url: None,
            commit_sha: None,
            commit_message: None,
            commit_date: None,
            author: None,
            is_dirty: false,
            root_path: std::path::PathBuf::from("."),
        };
        self.repository.ensure_project_branch(&ctx).await?;

        // Queue-based architecture: FileQueue → Parsers → ChunkQueue → Embedders → Storage
        use crate::queues::in_memory_queue::{InMemoryChunkQueue, InMemoryFileQueue};
        use crate::queues::postgres_queue::PostgresFileQueue;

        // Create file queue (PostgreSQL persistent or in-memory based on config)
        let file_queue: Arc<dyn FileContentQueue> = if self.config.indexing.use_persistent_queue {
            tracing::info!("Using PostgreSQL-backed persistent file queue");
            // Create a job for this indexing operation
            let job = self
                .repository
                .create_indexing_job(&repository_id, &branch, None)
                .await?;
            Arc::new(PostgresFileQueue::new(
                Arc::clone(&self.repository),
                job.job_id,
                repository_id.clone(),
                branch.clone(),
            ))
        } else {
            tracing::info!("Using in-memory file queue");
            Arc::new(InMemoryFileQueue::new())
        };
        let chunk_queue: Arc<dyn ChunkQueue> = Arc::new(InMemoryChunkQueue::new(
            self.config.indexing.chunk_queue_capacity,
        ));

        // Push all files to queue
        tracing::info!("Pushing {} files to queue", files.len());
        for file in files {
            file_queue.push(file).await?;
        }

        // Shared counters for results
        let files_indexed = Arc::new(AtomicUsize::new(0));
        let chunks_created = Arc::new(AtomicUsize::new(0));
        let chunks_stored = Arc::new(AtomicUsize::new(0));

        // Spawn parser workers
        let parser_count = self.config.indexing.concurrency_limit;
        tracing::info!("Spawning {parser_count} parser workers");
        let mut parser_handles = vec![];

        for worker_id in 0..parser_count {
            let handle = tokio::spawn(parser_worker(
                worker_id,
                file_queue.clone(),
                chunk_queue.clone(),
                self.code_parser.clone(),
                self.repository.clone(),
                repository_id.clone(),
                branch.clone(),
                files_indexed.clone(),
            ));
            parser_handles.push(handle);
        }

        // Spawn embedding workers
        let embedder_count = self.config.embedding.performance.pool_size;
        let chunk_batch_size = self.config.embedding.performance.indexer_batch_size;
        tracing::info!(
            "Spawning {embedder_count} embedding workers (batch_size={chunk_batch_size})"
        );
        let mut embedding_handles = vec![];

        for worker_id in 0..embedder_count {
            let handle = tokio::spawn(embedding_worker(
                worker_id,
                chunk_queue.clone(),
                self.embedding_service.clone(),
                self.storage.clone(),
                self.repository.clone(),
                repository_id.clone(),
                branch.clone(),
                chunk_batch_size,
                chunks_created.clone(),
                chunks_stored.clone(),
            ));
            embedding_handles.push(handle);
        }

        // Close file queue to signal parsers - they'll exit when queue is empty and closed
        file_queue.close();

        // Wait for all parsers to finish
        for handle in parser_handles {
            handle
                .await
                .map_err(|e| IndexerError::Other(format!("Parser worker panicked: {e}")))??;
        }
        tracing::info!("All parser workers complete");

        // Close chunk queue to signal embedders - they'll exit when queue is empty and closed
        chunk_queue.close();

        // Wait for chunk queue to drain
        for handle in embedding_handles {
            handle
                .await
                .map_err(|e| IndexerError::Other(format!("Embedding worker panicked: {e}")))??;
        }
        tracing::info!("All embedding workers complete");

        let result = IndexResult {
            files_indexed: files_indexed.load(Ordering::Relaxed),
            chunks_created: chunks_created.load(Ordering::Relaxed),
            chunks_stored: chunks_stored.load(Ordering::Relaxed),
        };

        tracing::info!(
            "📝 INDEX COMPLETE: files={}, chunks_created={}, chunks_stored={}",
            result.files_indexed,
            result.chunks_created,
            result.chunks_stored
        );

        Ok(result)
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
    async fn index_file_content(
        &mut self,
        project_id: &str,
        files: Vec<ServiceFileContent>,
    ) -> crate::IndexerResult<IndexResult> {
        self.index_file_content(project_id, files).await
    }

    async fn start_indexing_job(
        &mut self,
        project_id: &str,
        files: Vec<ServiceFileContent>,
    ) -> crate::IndexerResult<uuid::Uuid> {
        // Parse project_id as "repository_id:branch"
        let (repository_id, branch) = project_id.split_once(':').unwrap_or((project_id, "main"));

        // Create job in database
        let job = self
            .repository
            .create_indexing_job(repository_id, branch, None)
            .await?;

        // Enqueue all files to persistent queue
        for file in files {
            self.repository
                .enqueue_file(
                    &job.job_id,
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
        &mut self,
        job_id: &uuid::Uuid,
    ) -> crate::IndexerResult<Option<codetriever_meta_data::models::IndexingJob>> {
        Ok(self.repository.get_indexing_job(job_id).await?)
    }

    async fn list_jobs(
        &mut self,
        project_id: Option<&str>,
    ) -> crate::IndexerResult<Vec<codetriever_meta_data::models::IndexingJob>> {
        Ok(self.repository.list_indexing_jobs(project_id).await?)
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
    use codetriever_meta_data::{mock::MockFileRepository, models::*, traits::FileRepository};
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

    #[tokio::test]
    async fn test_indexer_uses_file_repository_to_check_state() {
        // Arrange - Create mock repository, storage, and embedding service
        let mock_repo = Arc::new(MockFileRepository::new()) as Arc<dyn FileRepository>;
        let mock_storage = Arc::new(MockStorage::new()) as Arc<dyn VectorStorage>;
        let mock_embedding_service = Arc::new(MockEmbeddingService);
        let mut config = ApplicationConfig::from_env();
        config.indexing.use_persistent_queue = false; // Unit tests use in-memory queue
        let code_parser = CodeParser::default(); // No tokenizer for unit tests

        // Create indexer with all required dependencies
        let mut indexer = Indexer::new(
            mock_embedding_service,
            mock_storage,
            mock_repo.clone(),
            code_parser,
            &config,
        );

        // Act - Index a file using index_file_content
        let content = r#"
fn main() {
    println!(\"Hello, world!\");
}
"#;
        let file_content = FileContent {
            path: "src/main.rs".to_string(),
            content: content.to_string(),
            hash: codetriever_meta_data::hash_content(content),
        };

        let result = indexer
            .index_file_content("test_repo:main", vec![file_content])
            .await;

        // Assert - Verify repository was called
        assert!(result.is_ok());
        assert_eq!(result.unwrap().files_indexed, 1);

        // Verify file state was checked
        let files = mock_repo
            .get_indexed_files("test_repo", "main")
            .await
            .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_path, "src/main.rs");
    }

    #[tokio::test]
    async fn test_indexer_handles_unchanged_files() {
        // Arrange - Create mocks and pre-populate with existing file
        let mock_repo = Arc::new(MockFileRepository::new());
        let mock_storage = Arc::new(MockStorage::new()) as Arc<dyn VectorStorage>;
        let mock_embedding_service = Arc::new(MockEmbeddingService);
        let config = ApplicationConfig::from_env();
        let code_parser = CodeParser::default();

        // Pre-populate with existing file with the hash we will use
        let content = "test content";
        let content_hash = codetriever_meta_data::hash_content(content);

        let existing_file = FileMetadata {
            path: "src/lib.rs".to_string(),
            content: content.to_string(),
            content_hash: content_hash.clone(),
            encoding: "UTF-8".to_string(),
            size_bytes: content.len() as i64,
            generation: 1,
            commit_sha: None,
            commit_message: None,
            commit_date: None,
            author: None,
        };

        mock_repo
            .record_file_indexing("test_repo", "main", &existing_file)
            .await
            .unwrap();

        let mut indexer = Indexer::new(
            mock_embedding_service,
            mock_storage,
            mock_repo.clone() as Arc<dyn FileRepository>,
            code_parser,
            &config,
        );

        // Act - Try to index same content (same hash)
        let file_content = FileContent {
            path: "src/lib.rs".to_string(),
            content: content.to_string(),
            hash: content_hash.clone(),
        };

        let result = indexer
            .index_file_content("test_repo:main", vec![file_content])
            .await;

        // Assert - File should be skipped
        assert!(result.is_ok());
        assert_eq!(result.unwrap().files_indexed, 0); // Should not index unchanged file

        let chunks = mock_repo
            .get_file_chunks("test_repo", "main", "src/lib.rs")
            .await
            .unwrap();
        assert_eq!(chunks.len(), 0); // No chunks should be created for unchanged file
    }

    #[tokio::test]
    async fn test_indexer_increments_generation_on_change() {
        // Arrange - Create all required mocks
        let mock_repo = Arc::new(MockFileRepository::new()) as Arc<dyn FileRepository>;
        let mock_storage = Arc::new(MockStorage::new()) as Arc<dyn VectorStorage>;
        let mock_embedding_service = Arc::new(MockEmbeddingService);
        let mut config = ApplicationConfig::from_env();
        config.indexing.use_persistent_queue = false; // Unit tests use in-memory queue
        let code_parser = CodeParser::default();

        // Create indexer with all required dependencies
        let mut indexer = Indexer::new(
            mock_embedding_service,
            mock_storage,
            mock_repo.clone(),
            code_parser,
            &config,
        );

        let file_v1 = FileContent {
            path: "src/main.rs".to_string(),
            content: "content v1".to_string(),
            hash: codetriever_meta_data::hash_content("content v1"),
        };

        indexer
            .index_file_content("test_repo:main", vec![file_v1])
            .await
            .unwrap();

        // Act - Index with different content
        let file_v2 = FileContent {
            path: "src/main.rs".to_string(),
            content: "content v2".to_string(),
            hash: codetriever_meta_data::hash_content("content v2"),
        };

        indexer
            .index_file_content("test_repo:main", vec![file_v2])
            .await
            .unwrap();

        // Assert - Generation should be incremented
        let files = mock_repo
            .get_indexed_files("test_repo", "main")
            .await
            .unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].generation, 2);
    }
}
