//! Database repository trait for dependency injection and testing

use async_trait::async_trait;
use uuid::Uuid;

use crate::error::Result;
use crate::models::{
    ChunkMetadata, FileMetadata, FileState, IndexedFile, IndexingJob, JobStatus, ProjectBranch,
    RepositoryContext,
};

/// Database repository trait for all database operations
#[async_trait]
pub trait FileRepository: Send + Sync {
    /// Get or create a project/branch combination
    async fn ensure_project_branch(&self, ctx: &RepositoryContext) -> Result<ProjectBranch>;

    /// Check file state for re-indexing decision
    async fn check_file_state(
        &self,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        content_hash: &str,
    ) -> Result<FileState>;

    /// Record file indexing with metadata
    async fn record_file_indexing(
        &self,
        repository_id: &str,
        branch: &str,
        metadata: &FileMetadata,
    ) -> Result<IndexedFile>;

    /// Insert chunk metadata for a file
    async fn insert_chunks(
        &self,
        repository_id: &str,
        branch: &str,
        chunks: Vec<ChunkMetadata>,
    ) -> Result<()>;

    /// Atomically replace chunks for a file (returns deleted chunk IDs)
    async fn replace_file_chunks(
        &self,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        new_generation: i64,
    ) -> Result<Vec<Uuid>>;

    /// Create new indexing job
    async fn create_indexing_job(
        &self,
        repository_id: &str,
        branch: &str,
        commit_sha: Option<&str>,
    ) -> Result<IndexingJob>;

    /// Update indexing job progress
    async fn update_job_progress(
        &self,
        job_id: &Uuid,
        files_processed: i32,
        chunks_created: i32,
    ) -> Result<()>;

    /// Complete indexing job
    async fn complete_job(
        &self,
        job_id: &Uuid,
        status: JobStatus,
        error: Option<String>,
    ) -> Result<()>;

    /// Get chunks for a specific file
    async fn get_file_chunks(
        &self,
        repository_id: &str,
        branch: &str,
        file_path: &str,
    ) -> Result<Vec<ChunkMetadata>>;

    /// Get all indexed files for a project/branch
    async fn get_indexed_files(
        &self,
        repository_id: &str,
        branch: &str,
    ) -> Result<Vec<IndexedFile>>;

    /// Check if any jobs are running for a project/branch
    async fn has_running_jobs(&self, repository_id: &str, branch: &str) -> Result<bool>;

    /// Get file metadata for a specific file path
    async fn get_file_metadata(
        &self,
        repository_id: &str,
        branch: &str,
        file_path: &str,
    ) -> Result<Option<IndexedFile>>;

    /// Get file metadata for multiple file paths (batch query for search results)
    async fn get_files_metadata(&self, file_paths: &[&str]) -> Result<Vec<IndexedFile>>;

    /// Get project branch metadata for repository/branch combination
    async fn get_project_branch(
        &self,
        repository_id: &str,
        branch: &str,
    ) -> Result<Option<ProjectBranch>>;

    /// Get multiple project branches in a single query (batch operation)
    async fn get_project_branches(
        &self,
        repo_branches: &[(String, String)], // (repository_id, branch) pairs
    ) -> Result<Vec<ProjectBranch>>;
}
