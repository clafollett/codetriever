use crate::IndexerResult;
use codetriever_embeddings::EmbeddingService;
use codetriever_parsing::CodeChunk as ParsingCodeChunk;
use codetriever_vector_data::{CodeChunk, VectorStorage};
use std::sync::Arc;

// Type alias for the repository trait object
type RepositoryRef = Arc<dyn codetriever_meta_data::traits::FileRepository>;

/// Convert from parsing CodeChunk to vector data CodeChunk
#[allow(dead_code)]
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
        vector_namespace: &str,
        tenant_id: uuid::Uuid,
        project_id: &str,
        files: Vec<ServiceFileContent>,
        commit_context: &codetriever_meta_data::models::CommitContext,
        correlation_id: &codetriever_common::CorrelationId,
    ) -> crate::IndexerResult<uuid::Uuid> {
        // Parse project_id as "repository_id:branch"
        let (repository_id, branch) = project_id.split_once(':').unwrap_or((project_id, "main"));

        // Ensure project branch exists (required for FK constraint)
        let ctx = codetriever_meta_data::models::RepositoryContext {
            tenant_id,
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
            repository_url: commit_context.repository_url.clone(),
            commit_sha: commit_context.commit_sha.clone(),
            commit_message: commit_context.commit_message.clone(),
            commit_date: commit_context.commit_date,
            author: commit_context.author.clone(),
            is_dirty: false,
            root_path: std::path::PathBuf::from("."),
        };
        self.repository.ensure_project_branch(&ctx).await?;

        // Create job in database with full commit context
        let job = self
            .repository
            .create_indexing_job(
                vector_namespace,
                &tenant_id,
                repository_id,
                branch,
                commit_context,
                correlation_id.to_uuid(),
            )
            .await?;

        // Enqueue all files to persistent queue (skip binary files)
        let mut enqueued_count = 0;
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
            enqueued_count += 1;
        }

        tracing::info!(
            correlation_id = %correlation_id,
            job_id = %job.job_id,
            files_enqueued = enqueued_count,
            "Files enqueued to job"
        );

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
        let correlation_id = codetriever_common::CorrelationId::new();

        // Create indexer with required dependencies
        let indexer = Indexer::new(mock_embedding_service, mock_storage, mock_repo.clone());

        // Act - Start an indexing job
        let content = r#"fn main() { println!("Hello, world!"); }"#;
        let file_content = FileContent {
            path: "src/main.rs".to_string(),
            content: content.to_string(),
            hash: codetriever_meta_data::hash_content(content),
        };

        let commit_context = codetriever_meta_data::models::CommitContext {
            repository_url: "https://github.com/test/repo".to_string(),
            commit_sha: "abc123".to_string(),
            commit_message: "Test commit".to_string(),
            commit_date: chrono::Utc::now(),
            author: "Test <test@test.com>".to_string(),
        };

        let job_id = indexer
            .start_indexing_job(
                "test_namespace",
                TEST_TENANT,
                "test_repo:main",
                vec![file_content],
                &commit_context,
                &correlation_id,
            )
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
