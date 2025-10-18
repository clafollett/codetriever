use crate::{IndexerResult, indexing::service::FileContent};
use codetriever_common::CorrelationId;
use codetriever_config::ApplicationConfig;
use codetriever_embeddings::EmbeddingService;
use codetriever_parsing::CodeChunk as ParsingCodeChunk;
use codetriever_parsing::{CodeParser, get_language_from_extension};
use codetriever_vector_data::{CodeChunk, VectorStorage};
use std::sync::Arc;

// Type alias for the repository trait object
type RepositoryRef = Arc<dyn codetriever_meta_data::traits::FileRepository>;

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
    code_parser: CodeParser,
    config: ApplicationConfig,
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
            code_parser,
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

        let mut all_chunks = Vec::new();
        let mut files_indexed = 0;

        // Track file metadata for database recording
        struct FileMetadata {
            file_path: String,
            generation: i64,
        }
        let mut file_metadata_map: Vec<FileMetadata> = Vec::new();

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

        // TODO: Issue #35 - Replace in-memory batching with persistent PostgreSQL job queue
        // Current approach: Process files in batches to limit memory usage (stopgap)
        // Future: PostgreSQL-backed job queue for async, resilient, scalable indexing

        // Process files in batches using CONFIG to prevent memory explosion
        // Batch size controlled by concurrency_limit (e.g., 4 files at a time)
        let file_batch_size = self.config.indexing.concurrency_limit;
        tracing::info!(
            "Processing {} files in batches of {} (controlled by CODETRIEVER_INDEXING_CONCURRENCY_LIMIT)",
            files.len(),
            file_batch_size
        );

        for (batch_num, file_batch) in files.chunks(file_batch_size).enumerate() {
            tracing::debug!(
                "Processing file batch {}/{} ({} files)",
                batch_num + 1,
                files.len().div_ceil(file_batch_size),
                file_batch.len()
            );

            for file in file_batch {
                tracing::debug!("Processing file: {}", file.path);

                // Check file state (repository is always available as required dependency)
                let content_hash = codetriever_meta_data::hash_content(&file.content);
                let state = self
                    .repository
                    .check_file_state(&repository_id, &branch, &file.path, &content_hash)
                    .await?;

                let current_generation = match state {
                    codetriever_meta_data::models::FileState::Unchanged => {
                        tracing::debug!("  Skipping unchanged file");
                        continue; // Skip unchanged files
                    }
                    codetriever_meta_data::models::FileState::New { generation }
                    | codetriever_meta_data::models::FileState::Updated {
                        new_generation: generation,
                        ..
                    } => {
                        let current_generation = generation;

                        // Record file indexing in database
                        let metadata = codetriever_meta_data::models::FileMetadata {
                            path: file.path.clone(),
                            content_hash: content_hash.clone(),
                            generation,
                            commit_sha: None,
                            commit_message: None,
                            commit_date: None,
                            author: None,
                        };

                        self.repository
                            .record_file_indexing(&repository_id, &branch, &metadata)
                            .await?;

                        // For updated files, delete old chunks from both database and Qdrant
                        if matches!(
                            state,
                            codetriever_meta_data::models::FileState::Updated { .. }
                        ) {
                            let deleted_ids = self
                                .repository
                                .replace_file_chunks(
                                    &repository_id,
                                    &branch,
                                    &file.path,
                                    generation,
                                )
                                .await?;
                            tracing::debug!(
                                "  Deleted {} old chunks from database",
                                deleted_ids.len()
                            );

                            // Delete from Qdrant (storage is always available)
                            self.storage.delete_chunks(&deleted_ids).await?;
                            tracing::debug!(
                                "  Deleted {} old chunks from Qdrant",
                                deleted_ids.len()
                            );
                        }

                        current_generation
                    }
                };

                // Get language from file extension
                let ext = file.path.rsplit('.').next().unwrap_or("");
                let language = get_language_from_extension(ext).unwrap_or(ext);

                // Parse the content into chunks
                tracing::info!(
                    "📝 Parsing file: {} ({} bytes)",
                    file.path,
                    file.content.len()
                );
                let chunks = self
                    .code_parser
                    .parse(&file.content, language, &file.path)?;

                if !chunks.is_empty() {
                    files_indexed += 1;
                    tracing::info!("✅ Parsed {} chunks from {}", chunks.len(), file.path);

                    // Track file metadata for this file
                    file_metadata_map.push(FileMetadata {
                        file_path: file.path.clone(),
                        generation: current_generation,
                    });

                    all_chunks.extend(chunks.into_iter().map(convert_chunk));
                } else {
                    tracing::warn!("⚠️  File {} produced ZERO chunks!", file.path);
                }
            }

            // Generate embeddings for this batch's chunks using CONFIG batch size
            // This prevents accumulating ALL files' chunks in RAM
            if !all_chunks.is_empty() {
                let chunk_batch_size = self.config.embedding.performance.indexer_batch_size;
                tracing::debug!(
                    "Generating embeddings for {} chunks in batches of {} (CODETRIEVER_EMBEDDING_INDEXER_BATCH_SIZE)",
                    all_chunks.len(),
                    chunk_batch_size
                );

                // Process chunks in batches
                for chunk_batch in all_chunks.chunks_mut(chunk_batch_size) {
                    let texts: Vec<&str> = chunk_batch.iter().map(|c| c.content.as_str()).collect();
                    let embeddings = self.embedding_service.generate_embeddings(texts).await?;

                    // Apply embeddings immediately
                    for (chunk, embedding) in chunk_batch.iter_mut().zip(embeddings.into_iter()) {
                        chunk.embedding = Some(embedding);
                    }
                }

                tracing::debug!("✅ Batch {} embeddings generated", all_chunks.len());
            }
        }

        let chunks_created = all_chunks.len();
        tracing::info!("📊 Total: {files_indexed} files indexed, {chunks_created} chunks created");

        // Store all chunks with their embeddings (already applied in batching loop above)
        if !all_chunks.is_empty() {
            tracing::info!("💾 Storing {} chunks with embeddings...", all_chunks.len());

            // Store chunks with embeddings (storage and repository are always available)
            tracing::debug!("Storing {} chunks in vector database...", all_chunks.len());

            // Store chunks per file with deterministic IDs
            for file_info in &file_metadata_map {
                let file_chunks: Vec<&CodeChunk> = all_chunks
                    .iter()
                    .filter(|c| c.file_path == file_info.file_path)
                    .collect();

                if !file_chunks.is_empty() {
                    // Collect chunks with embeddings
                    let mut chunks_with_embeddings = Vec::new();
                    for chunk in file_chunks {
                        chunks_with_embeddings.push(chunk.clone());
                    }

                    let correlation_id = CorrelationId::new();
                    let chunk_ids = self
                        .storage
                        .store_chunks(
                            &repository_id,
                            &branch,
                            &chunks_with_embeddings,
                            file_info.generation,
                            &correlation_id,
                        )
                        .await?;

                    // Record chunk IDs in database
                    let chunk_metadata: Vec<codetriever_meta_data::models::ChunkMetadata> =
                        chunk_ids
                            .iter()
                            .enumerate()
                            .zip(&chunks_with_embeddings)
                            .map(|((idx, id), chunk)| {
                                codetriever_meta_data::models::ChunkMetadata {
                                    chunk_id: *id,
                                    repository_id: repository_id.clone(),
                                    branch: branch.clone(),
                                    file_path: chunk.file_path.clone(),
                                    chunk_index: idx as i32,
                                    generation: file_info.generation,
                                    start_line: chunk.start_line as i32,
                                    end_line: chunk.end_line as i32,
                                    byte_start: chunk.byte_start as i64,
                                    byte_end: chunk.byte_end as i64,
                                    kind: chunk.kind.clone(),
                                    name: chunk.name.clone(),
                                    created_at: chrono::Utc::now(),
                                }
                            })
                            .collect();

                    self.repository
                        .insert_chunks(&repository_id, &branch, chunk_metadata)
                        .await?;
                }
            }

            tracing::debug!("Successfully stored chunks");
        }

        Ok(IndexResult {
            files_indexed,
            chunks_created,
            chunks_stored: chunks_created, // All created chunks are stored
        })
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
        let config = ApplicationConfig::from_env();
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
            content_hash: content_hash.clone(),
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
        let config = ApplicationConfig::from_env();
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
