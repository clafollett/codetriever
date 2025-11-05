//! Mock implementation of `DbRepository` for testing

// Allow test-specific patterns in mock implementation
#![allow(clippy::unwrap_used)] // Mocks can panic on lock poisoning
#![allow(clippy::expect_used)] // Test code can use expect
#![allow(clippy::arithmetic_side_effects)] // Test counters can overflow
#![allow(clippy::significant_drop_tightening)] // Mock locks don't need optimization

use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::error::{DatabaseError, DatabaseOperation, DatabaseResult};

use crate::models::{
    ChunkMetadata, CommitContext, FileMetadata, FileState, IndexedFile, IndexingJob, JobStatus,
    ProjectBranch, RepositoryContext,
};
use crate::traits::FileRepository;

// Type aliases to simplify complex types
type ProjectBranchMap = Arc<Mutex<HashMap<(Uuid, String, String), ProjectBranch>>>;
type IndexedFileMap = Arc<Mutex<HashMap<(Uuid, String, String, String), IndexedFile>>>;
type ChunkList = Arc<Mutex<Vec<ChunkMetadata>>>;
type JobMap = Arc<Mutex<HashMap<Uuid, IndexingJob>>>;

/// Mock repository for testing
#[derive(Clone)]
pub struct MockFileRepository {
    pub project_branches: ProjectBranchMap,
    pub indexed_files: IndexedFileMap,
    pub chunks: ChunkList,
    pub jobs: JobMap,

    // Behavior controls for testing
    pub should_fail_next: Arc<Mutex<bool>>,
    pub error_message: Arc<Mutex<String>>,
}

impl Default for MockFileRepository {
    fn default() -> Self {
        Self {
            project_branches: Arc::new(Mutex::new(HashMap::new())),
            indexed_files: Arc::new(Mutex::new(HashMap::new())),
            chunks: Arc::new(Mutex::new(Vec::new())),
            jobs: Arc::new(Mutex::new(HashMap::new())),
            should_fail_next: Arc::new(Mutex::new(false)),
            error_message: Arc::new(Mutex::new("Mock error".to_string())),
        }
    }
}

impl MockFileRepository {
    /// Create a new mock repository
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure to fail on next operation
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned (should only happen if another thread panicked while holding the lock)
    pub fn fail_next(&self, message: &str) {
        *self.should_fail_next.lock().unwrap() = true;
        *self.error_message.lock().unwrap() = message.to_string();
    }

    /// Check if should fail and reset
    fn check_fail(&self) -> DatabaseResult<()> {
        let should_fail = *self.should_fail_next.lock().unwrap();
        if should_fail {
            *self.should_fail_next.lock().unwrap() = false;
            let message = self.error_message.lock().unwrap().clone();
            return Err(DatabaseError::UnexpectedState {
                operation: Box::new(DatabaseOperation::Query {
                    description: "mock operation".to_string(),
                }),
                message,
                correlation_id: None,
            });
        }
        Ok(())
    }
}

#[async_trait]
impl FileRepository for MockFileRepository {
    async fn create_tenant(&self, _name: &str) -> DatabaseResult<Uuid> {
        self.check_fail()?;
        // Mock just returns a new UUID (doesn't actually store tenants)
        Ok(Uuid::new_v4())
    }

    async fn ensure_project_branch(
        &self,
        ctx: &RepositoryContext,
    ) -> DatabaseResult<ProjectBranch> {
        self.check_fail()?;

        let key = (ctx.tenant_id, ctx.repository_id.clone(), ctx.branch.clone());
        let mut branches = self.project_branches.lock().unwrap();

        let branch = branches.entry(key).or_insert_with(|| ProjectBranch {
            tenant_id: ctx.tenant_id,
            repository_id: ctx.repository_id.clone(),
            branch: ctx.branch.clone(),
            repository_url: Some(ctx.repository_url.clone()),
            first_seen: Utc::now(),
            last_indexed: None,
        });

        Ok(branch.clone())
    }

    async fn check_file_state(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        content_hash: &str,
    ) -> DatabaseResult<FileState> {
        self.check_fail()?;

        let key = (
            *tenant_id,
            repository_id.to_string(),
            branch.to_string(),
            file_path.to_string(),
        );
        let files = self.indexed_files.lock().unwrap();

        match files.get(&key) {
            None => Ok(FileState::New { generation: 1 }),
            Some(file) if file.content_hash == content_hash => Ok(FileState::Unchanged),
            Some(file) => Ok(FileState::Updated {
                old_generation: file.generation,
                new_generation: file.generation + 1,
            }),
        }
    }

    async fn record_file_indexing(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        metadata: &FileMetadata,
    ) -> DatabaseResult<IndexedFile> {
        self.check_fail()?;

        let key = (
            *tenant_id,
            repository_id.to_string(),
            branch.to_string(),
            metadata.path.clone(),
        );
        let file = IndexedFile {
            tenant_id: *tenant_id,
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
            file_path: metadata.path.clone(),
            file_content: metadata.content.clone(),
            content_hash: metadata.content_hash.clone(),
            encoding: metadata.encoding.clone(),
            size_bytes: metadata.size_bytes,
            generation: metadata.generation,
            commit_sha: Some(metadata.commit_sha.clone()),
            commit_message: Some(metadata.commit_message.clone()),
            commit_date: Some(metadata.commit_date),
            author: Some(metadata.author.clone()),
            indexed_at: Utc::now(),
        };

        self.indexed_files.lock().unwrap().insert(key, file.clone());
        Ok(file)
    }

    async fn insert_chunks(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        chunks: Vec<ChunkMetadata>,
    ) -> DatabaseResult<()> {
        self.check_fail()?;

        let mut stored_chunks = self.chunks.lock().unwrap();
        for mut chunk in chunks {
            chunk.tenant_id = *tenant_id;
            chunk.repository_id = repository_id.to_string();
            chunk.branch = branch.to_string();
            stored_chunks.push(chunk);
        }
        Ok(())
    }

    async fn replace_file_chunks(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        new_generation: i64,
    ) -> DatabaseResult<Vec<Uuid>> {
        self.check_fail()?;

        let mut chunks = self.chunks.lock().unwrap();
        let deleted_ids: Vec<Uuid> = chunks
            .iter()
            .filter(|c| {
                c.tenant_id == *tenant_id
                    && c.repository_id == repository_id
                    && c.branch == branch
                    && c.file_path == file_path
                    && c.generation < new_generation
            })
            .map(|c| c.chunk_id)
            .collect();

        chunks.retain(|c| {
            !(c.tenant_id == *tenant_id
                && c.repository_id == repository_id
                && c.branch == branch
                && c.file_path == file_path
                && c.generation < new_generation)
        });

        Ok(deleted_ids)
    }

    async fn create_indexing_job(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        commit_context: &CommitContext,
    ) -> DatabaseResult<IndexingJob> {
        self.check_fail()?;

        let job_id = Uuid::new_v4();
        let job = IndexingJob {
            job_id,
            tenant_id: *tenant_id,
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
            status: JobStatus::Running,
            files_total: None,
            files_processed: 0,
            chunks_created: 0,
            repository_url: commit_context.repository_url.clone(),
            commit_sha: commit_context.commit_sha.clone(),
            commit_message: commit_context.commit_message.clone(),
            commit_date: commit_context.commit_date,
            author: commit_context.author.clone(),
            started_at: Utc::now(),
            completed_at: None,
            error_message: None,
        };

        self.jobs.lock().unwrap().insert(job_id, job.clone());
        Ok(job)
    }

    async fn update_job_progress(
        &self,
        job_id: &Uuid,
        files_processed: i32,
        chunks_created: i32,
    ) -> DatabaseResult<()> {
        self.check_fail()?;

        let mut jobs = self.jobs.lock().unwrap();
        if let Some(job) = jobs.get_mut(job_id) {
            job.files_processed += files_processed;
            job.chunks_created += chunks_created;
        }
        Ok(())
    }

    async fn complete_job(
        &self,
        job_id: &Uuid,
        status: JobStatus,
        error: Option<String>,
    ) -> DatabaseResult<()> {
        self.check_fail()?;

        let mut jobs = self.jobs.lock().unwrap();
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = status;
            job.completed_at = Some(Utc::now());
            job.error_message = error;
        }
        Ok(())
    }

    async fn get_file_chunks(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
    ) -> DatabaseResult<Vec<ChunkMetadata>> {
        self.check_fail()?;

        let chunks = self.chunks.lock().unwrap();
        Ok(chunks
            .iter()
            .filter(|c| {
                c.tenant_id == *tenant_id
                    && c.repository_id == repository_id
                    && c.branch == branch
                    && c.file_path == file_path
            })
            .cloned()
            .collect())
    }

    async fn get_indexed_files(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
    ) -> DatabaseResult<Vec<IndexedFile>> {
        self.check_fail()?;

        let files = self.indexed_files.lock().unwrap();
        Ok(files
            .values()
            .filter(|f| {
                f.tenant_id == *tenant_id && f.repository_id == repository_id && f.branch == branch
            })
            .cloned()
            .collect())
    }

    async fn has_running_jobs(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
    ) -> DatabaseResult<bool> {
        self.check_fail()?;

        let jobs = self.jobs.lock().unwrap();
        Ok(jobs.values().any(|j| {
            j.tenant_id == *tenant_id
                && j.repository_id == repository_id
                && j.branch == branch
                && matches!(j.status, JobStatus::Running | JobStatus::Pending)
        }))
    }

    async fn get_file_metadata(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
    ) -> DatabaseResult<Option<IndexedFile>> {
        self.check_fail()?;

        let key = (
            *tenant_id,
            repository_id.to_string(),
            branch.to_string(),
            file_path.to_string(),
        );
        let files = self.indexed_files.lock().unwrap();
        Ok(files.get(&key).cloned())
    }

    async fn get_files_metadata(
        &self,
        tenant_id: &Uuid,
        file_paths: &[&str],
    ) -> DatabaseResult<Vec<IndexedFile>> {
        self.check_fail()?;

        let files = self.indexed_files.lock().unwrap();
        let mut results = Vec::new();

        for &file_path in file_paths {
            // Search across all repository/branch combinations for this tenant + file path
            for ((tid, _, _, stored_path), file) in files.iter() {
                if tid == tenant_id && stored_path == file_path {
                    results.push(file.clone());
                }
            }
        }

        Ok(results)
    }

    async fn get_project_branch(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
    ) -> DatabaseResult<Option<ProjectBranch>> {
        self.check_fail()?;

        let key = (*tenant_id, repository_id.to_string(), branch.to_string());
        let branches = self.project_branches.lock().unwrap();
        Ok(branches.get(&key).cloned())
    }

    async fn get_project_branches(
        &self,
        tenant_id: &Uuid,
        repo_branches: &[(String, String)],
    ) -> DatabaseResult<Vec<ProjectBranch>> {
        self.check_fail()?;

        let branches = self.project_branches.lock().unwrap();
        let results = repo_branches
            .iter()
            .filter_map(|(repo_id, branch)| {
                let key = (*tenant_id, repo_id.clone(), branch.clone());
                branches.get(&key).cloned()
            })
            .collect();
        Ok(results)
    }

    async fn enqueue_file(
        &self,
        _job_id: &Uuid,
        _tenant_id: &Uuid,
        _repository_id: &str,
        _branch: &str,
        _file_path: &str,
        _file_content: &str,
        _content_hash: &str,
    ) -> DatabaseResult<()> {
        self.check_fail()?;
        // Mock doesn't implement queue - just succeed
        Ok(())
    }

    async fn dequeue_file(&self) -> DatabaseResult<Option<crate::models::DequeuedFile>> {
        self.check_fail()?;
        // Mock returns None (empty queue)
        Ok(None)
    }

    async fn get_queue_depth(&self, _job_id: &Uuid) -> DatabaseResult<i64> {
        self.check_fail()?;
        Ok(0)
    }

    async fn increment_job_progress(
        &self,
        _job_id: &Uuid,
        _files_delta: i32,
        _chunks_delta: i32,
    ) -> DatabaseResult<()> {
        self.check_fail()?;
        Ok(())
    }

    async fn check_job_complete(&self, _job_id: &Uuid) -> DatabaseResult<bool> {
        self.check_fail()?;
        Ok(true) // Mock always returns complete
    }

    async fn get_indexing_job(&self, job_id: &Uuid) -> DatabaseResult<Option<IndexingJob>> {
        self.check_fail()?;
        Ok(self.jobs.lock().unwrap().get(job_id).cloned())
    }

    async fn list_indexing_jobs(
        &self,
        _tenant_id: Option<&Uuid>,
        _repository_id: Option<&str>,
    ) -> DatabaseResult<Vec<IndexingJob>> {
        self.check_fail()?;
        Ok(vec![])
    }

    async fn mark_file_completed(
        &self,
        _job_id: &uuid::Uuid,
        _file_path: &str,
    ) -> DatabaseResult<()> {
        self.check_fail()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test tenant for all unit tests
    const TEST_TENANT: Uuid = Uuid::nil();

    /// Helper to build test `RepositoryContext` with defaults
    fn test_repo_context(repo_id: &str, branch: &str) -> RepositoryContext {
        RepositoryContext {
            tenant_id: TEST_TENANT,
            repository_id: repo_id.to_string(),
            repository_url: format!("https://github.com/test/{repo_id}"),
            branch: branch.to_string(),
            commit_sha: "abc123def456".to_string(),
            commit_message: "Test commit".to_string(),
            commit_date: Utc::now(),
            author: "Test Author <test@example.com>".to_string(),
            is_dirty: false,
            root_path: std::path::PathBuf::from("/tmp"),
        }
    }

    #[tokio::test]
    async fn test_mock_project_branch() {
        let mock = MockFileRepository::new();
        let ctx = test_repo_context("github.com/user/repo", "main");

        // First call creates
        let branch1 = mock.ensure_project_branch(&ctx).await.unwrap();
        assert_eq!(branch1.repository_id, "github.com/user/repo");
        assert_eq!(branch1.branch, "main");

        // Second call returns existing
        let branch2 = mock.ensure_project_branch(&ctx).await.unwrap();
        assert_eq!(branch1.repository_id, branch2.repository_id);
    }

    #[tokio::test]
    async fn test_mock_file_state() {
        let mock = MockFileRepository::new();

        // New file
        let state = mock
            .check_file_state(&TEST_TENANT, "repo", "main", "file.rs", "hash1")
            .await
            .unwrap();
        assert!(matches!(state, FileState::New { generation: 1 }));

        // Record the file
        let metadata = FileMetadata {
            path: "file.rs".to_string(),
            content: "fn main() {}".to_string(),
            content_hash: "hash1".to_string(),
            encoding: "UTF-8".to_string(),
            size_bytes: 12,
            generation: 1,
            commit_sha: "abc123".to_string(),
            commit_message: "Initial commit".to_string(),
            commit_date: Utc::now(),
            author: "Test Author".to_string(),
        };
        mock.record_file_indexing(&TEST_TENANT, "repo", "main", &metadata)
            .await
            .unwrap();

        // Same hash = unchanged
        let state = mock
            .check_file_state(&TEST_TENANT, "repo", "main", "file.rs", "hash1")
            .await
            .unwrap();
        assert!(matches!(state, FileState::Unchanged));

        // Different hash = updated
        let state = mock
            .check_file_state(&TEST_TENANT, "repo", "main", "file.rs", "hash2")
            .await
            .unwrap();
        assert!(matches!(
            state,
            FileState::Updated {
                old_generation: 1,
                new_generation: 2
            }
        ));
    }

    #[tokio::test]
    async fn test_mock_error_handling() {
        let mock = MockFileRepository::new();
        mock.fail_next("Expected test error");

        let result = mock.has_running_jobs(&TEST_TENANT, "repo", "main").await;
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Expected test error"));

        // Should work after error is consumed
        let result = mock.has_running_jobs(&TEST_TENANT, "repo", "main").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_chunks() {
        let mock = MockFileRepository::new();

        // Generate deterministic UUIDs for testing using byte ranges
        let chunk_id1 = crate::generate_chunk_id("repo", "main", "file.rs", 1, 0, 100);
        let chunk_id2 = crate::generate_chunk_id("repo", "main", "file.rs", 1, 100, 200);

        let chunks = vec![
            ChunkMetadata {
                chunk_id: chunk_id1,
                tenant_id: TEST_TENANT,
                repository_id: "repo".to_string(),
                branch: "main".to_string(),
                file_path: "file.rs".to_string(),
                chunk_index: 0,
                generation: 1,
                start_line: 1,
                end_line: 10,
                byte_start: 0,
                byte_end: 100,
                kind: None,
                name: None,
                created_at: Utc::now(),
            },
            ChunkMetadata {
                chunk_id: chunk_id2,
                tenant_id: TEST_TENANT,
                repository_id: "repo".to_string(),
                branch: "main".to_string(),
                file_path: "file.rs".to_string(),
                chunk_index: 1,
                generation: 1,
                start_line: 11,
                end_line: 20,
                byte_start: 100,
                byte_end: 200,
                kind: None,
                name: None,
                created_at: Utc::now(),
            },
        ];

        mock.insert_chunks(&TEST_TENANT, "repo", "main", chunks)
            .await
            .unwrap();

        let retrieved = mock
            .get_file_chunks(&TEST_TENANT, "repo", "main", "file.rs")
            .await
            .unwrap();
        assert_eq!(retrieved.len(), 2);

        // Replace with new generation
        let deleted = mock
            .replace_file_chunks(&TEST_TENANT, "repo", "main", "file.rs", 2)
            .await
            .unwrap();
        assert_eq!(deleted.len(), 2);
        assert!(deleted.contains(&chunk_id1));
        assert!(deleted.contains(&chunk_id2));

        let remaining = mock
            .get_file_chunks(&TEST_TENANT, "repo", "main", "file.rs")
            .await
            .unwrap();
        assert_eq!(remaining.len(), 0);
    }
}

/// Mock `DataClient` for API testing
#[derive(Clone)]
pub struct MockDataClient {
    repository: MockFileRepository,
}

impl MockDataClient {
    #[must_use]
    pub fn new() -> Self {
        Self {
            repository: MockFileRepository::new(),
        }
    }

    /// Count project branches in mock data
    ///
    /// # Errors
    ///
    /// Never returns error for mock implementation
    ///
    /// # Panics
    ///
    /// Panics if mutex is poisoned (test code only)
    pub fn count_project_branches(&self) -> DatabaseResult<i64> {
        #[allow(clippy::cast_possible_wrap)]
        Ok(self.repository.project_branches.lock().unwrap().len() as i64)
    }

    /// Count indexed files in mock data
    ///
    /// # Errors
    ///
    /// Never returns error for mock implementation
    ///
    /// # Panics
    ///
    /// Panics if mutex is poisoned (test code only)
    pub fn count_indexed_files(&self) -> DatabaseResult<i64> {
        #[allow(clippy::cast_possible_wrap)]
        Ok(self.repository.indexed_files.lock().unwrap().len() as i64)
    }

    /// Count chunks in mock data
    ///
    /// # Errors
    ///
    /// Never returns error for mock implementation
    ///
    /// # Panics
    ///
    /// Panics if mutex is poisoned (test code only)
    pub fn count_chunks(&self) -> DatabaseResult<i64> {
        #[allow(clippy::cast_possible_wrap)]
        Ok(self.repository.chunks.lock().unwrap().len() as i64)
    }

    /// Get mock database size (always returns 1.0 MB for testing)
    ///
    /// # Errors
    ///
    /// Never returns error for mock implementation
    pub const fn get_database_size_mb(&self) -> DatabaseResult<f64> {
        // Mock implementation returns fixed size
        Ok(1.0)
    }

    /// Get last indexed timestamp from mock data
    ///
    /// Returns most recent `last_indexed` across all project branches
    ///
    /// # Errors
    ///
    /// Never returns error for mock implementation
    ///
    /// # Panics
    ///
    /// Panics if mutex is poisoned (test code only)
    #[allow(clippy::type_complexity)] // Return type mirrors production code signature
    pub fn get_last_indexed_timestamp(
        &self,
    ) -> DatabaseResult<Option<chrono::DateTime<chrono::Utc>>> {
        let branches = self.repository.project_branches.lock().unwrap();
        let last_indexed = branches.values().filter_map(|b| b.last_indexed).max();
        Ok(last_indexed)
    }
}

impl Default for MockDataClient {
    fn default() -> Self {
        Self::new()
    }
}
