//! Database repository trait for dependency injection and testing

use async_trait::async_trait;
use uuid::Uuid;

use crate::error::DatabaseResult;
use crate::models::{
    ChunkMetadata, CommitContext, DequeuedFile, FileMetadata, FileState, IndexedFile, IndexingJob,
    JobStatus, ProjectBranch, RepositoryContext,
};

/// Database repository trait for all database operations
#[async_trait]
pub trait FileRepository: Send + Sync {
    /// Create a new tenant
    ///
    /// Returns the created `tenant_id`
    async fn create_tenant(&self, name: &str) -> DatabaseResult<Uuid>;

    /// Get or create a project/branch combination
    async fn ensure_project_branch(&self, ctx: &RepositoryContext)
    -> DatabaseResult<ProjectBranch>;

    /// Check file state for re-indexing decision
    async fn check_file_state(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        content_hash: &str,
    ) -> DatabaseResult<FileState>;

    /// Record file indexing with metadata
    async fn record_file_indexing(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        metadata: &FileMetadata,
    ) -> DatabaseResult<IndexedFile>;

    /// Insert chunk metadata for a file
    async fn insert_chunks(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        chunks: Vec<ChunkMetadata>,
    ) -> DatabaseResult<()>;

    /// Atomically replace chunks for a file (returns deleted chunk IDs)
    async fn replace_file_chunks(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        new_generation: i64,
    ) -> DatabaseResult<Vec<Uuid>>;

    /// Create new indexing job with commit context
    async fn create_indexing_job(
        &self,
        vector_namespace: &str,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        commit_context: &CommitContext,
    ) -> DatabaseResult<IndexingJob>;

    /// Update indexing job progress
    async fn update_job_progress(
        &self,
        job_id: &Uuid,
        files_processed: i32,
        chunks_created: i32,
    ) -> DatabaseResult<()>;

    /// Complete indexing job
    async fn complete_job(
        &self,
        job_id: &Uuid,
        status: JobStatus,
        error: Option<String>,
    ) -> DatabaseResult<()>;

    /// Get indexing job by ID
    async fn get_indexing_job(&self, job_id: &Uuid) -> DatabaseResult<Option<IndexingJob>>;

    /// List indexing jobs, optionally filtered by tenant and/or repository
    async fn list_indexing_jobs(
        &self,
        tenant_id: Option<&Uuid>,
        repository_id: Option<&str>,
    ) -> DatabaseResult<Vec<IndexingJob>>;

    /// Get chunks for a specific file
    async fn get_file_chunks(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
    ) -> DatabaseResult<Vec<ChunkMetadata>>;

    /// Get all indexed files for a project/branch
    async fn get_indexed_files(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
    ) -> DatabaseResult<Vec<IndexedFile>>;

    /// Check if any jobs are running for a project/branch
    async fn has_running_jobs(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
    ) -> DatabaseResult<bool>;

    /// Get file metadata for a specific file path
    async fn get_file_metadata(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
    ) -> DatabaseResult<Option<IndexedFile>>;

    /// Get file metadata for multiple file paths (batch query for search results)
    async fn get_files_metadata(
        &self,
        tenant_id: &Uuid,
        file_paths: &[&str],
    ) -> DatabaseResult<Vec<IndexedFile>>;

    /// Get project branch metadata for repository/branch combination
    async fn get_project_branch(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
    ) -> DatabaseResult<Option<ProjectBranch>>;

    /// Get multiple project branches in a single query (batch operation)
    async fn get_project_branches(
        &self,
        tenant_id: &Uuid,
        repo_branches: &[(String, String)], // (repository_id, branch) pairs
    ) -> DatabaseResult<Vec<ProjectBranch>>;

    /// Enqueue a file for persistent indexing (`PostgreSQL` queue)
    async fn enqueue_file(
        &self,
        job_id: &Uuid,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        file_content: &str,
        content_hash: &str,
    ) -> DatabaseResult<()>;

    /// Dequeue next file from global queue (atomic with FOR UPDATE SKIP LOCKED)
    ///
    /// Pulls next file from ANY tenant's jobs in FIFO order (global queue).
    /// File contains `tenant_id` for downstream isolation.
    /// Returns None if no files available in queue.
    async fn dequeue_file(&self) -> DatabaseResult<Option<DequeuedFile>>;

    /// Get queue depth for a job
    async fn get_queue_depth(&self, job_id: &Uuid) -> DatabaseResult<i64>;

    /// Atomically increment job progress after processing a file
    async fn increment_job_progress(
        &self,
        job_id: &Uuid,
        files_delta: i32,
        chunks_delta: i32,
    ) -> DatabaseResult<()>;

    /// Mark a file as completed in the queue
    async fn mark_file_completed(&self, job_id: &Uuid, file_path: &str) -> DatabaseResult<()>;

    /// Check if job is complete (no more queued or processing files)
    async fn check_job_complete(&self, job_id: &Uuid) -> DatabaseResult<bool>;
}
