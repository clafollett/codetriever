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
    ChunkMetadata, FileMetadata, FileState, IndexedFile, IndexingJob, JobStatus, ProjectBranch,
    RepositoryContext,
};
use crate::traits::FileRepository;

// Type aliases to simplify complex types
type ProjectBranchMap = Arc<Mutex<HashMap<(String, String), ProjectBranch>>>;
type IndexedFileMap = Arc<Mutex<HashMap<(String, String, String), IndexedFile>>>;
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
                operation: DatabaseOperation::Query {
                    description: "mock operation".to_string(),
                },
                message,
                correlation_id: None,
            });
        }
        Ok(())
    }
}

#[async_trait]
impl FileRepository for MockFileRepository {
    async fn ensure_project_branch(
        &self,
        ctx: &RepositoryContext,
    ) -> DatabaseResult<ProjectBranch> {
        self.check_fail()?;

        let key = (ctx.repository_id.clone(), ctx.branch.clone());
        let mut branches = self.project_branches.lock().unwrap();

        let branch = branches.entry(key).or_insert_with(|| ProjectBranch {
            repository_id: ctx.repository_id.clone(),
            branch: ctx.branch.clone(),
            repository_url: ctx.repository_url.clone(),
            first_seen: Utc::now(),
            last_indexed: None,
        });

        Ok(branch.clone())
    }

    async fn check_file_state(
        &self,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        content_hash: &str,
    ) -> DatabaseResult<FileState> {
        self.check_fail()?;

        let key = (
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
        repository_id: &str,
        branch: &str,
        metadata: &FileMetadata,
    ) -> DatabaseResult<IndexedFile> {
        self.check_fail()?;

        let key = (
            repository_id.to_string(),
            branch.to_string(),
            metadata.path.clone(),
        );
        let file = IndexedFile {
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
            file_path: metadata.path.clone(),
            content_hash: metadata.content_hash.clone(),
            generation: metadata.generation,
            commit_sha: metadata.commit_sha.clone(),
            commit_message: metadata.commit_message.clone(),
            commit_date: metadata.commit_date,
            author: metadata.author.clone(),
            indexed_at: Utc::now(),
        };

        self.indexed_files.lock().unwrap().insert(key, file.clone());
        Ok(file)
    }

    async fn insert_chunks(
        &self,
        repository_id: &str,
        branch: &str,
        chunks: Vec<ChunkMetadata>,
    ) -> DatabaseResult<()> {
        self.check_fail()?;

        let mut stored_chunks = self.chunks.lock().unwrap();
        for mut chunk in chunks {
            chunk.repository_id = repository_id.to_string();
            chunk.branch = branch.to_string();
            stored_chunks.push(chunk);
        }
        Ok(())
    }

    async fn replace_file_chunks(
        &self,
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
                c.repository_id == repository_id
                    && c.branch == branch
                    && c.file_path == file_path
                    && c.generation < new_generation
            })
            .map(|c| c.chunk_id)
            .collect();

        chunks.retain(|c| {
            !(c.repository_id == repository_id
                && c.branch == branch
                && c.file_path == file_path
                && c.generation < new_generation)
        });

        Ok(deleted_ids)
    }

    async fn create_indexing_job(
        &self,
        repository_id: &str,
        branch: &str,
        commit_sha: Option<&str>,
    ) -> DatabaseResult<IndexingJob> {
        self.check_fail()?;

        let job_id = Uuid::new_v4();
        let job = IndexingJob {
            job_id,
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
            status: JobStatus::Running,
            files_total: None,
            files_processed: 0,
            chunks_created: 0,
            commit_sha: commit_sha.map(std::string::ToString::to_string),
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
        repository_id: &str,
        branch: &str,
        file_path: &str,
    ) -> DatabaseResult<Vec<ChunkMetadata>> {
        self.check_fail()?;

        let chunks = self.chunks.lock().unwrap();
        Ok(chunks
            .iter()
            .filter(|c| {
                c.repository_id == repository_id && c.branch == branch && c.file_path == file_path
            })
            .cloned()
            .collect())
    }

    async fn get_indexed_files(
        &self,
        repository_id: &str,
        branch: &str,
    ) -> DatabaseResult<Vec<IndexedFile>> {
        self.check_fail()?;

        let files = self.indexed_files.lock().unwrap();
        Ok(files
            .values()
            .filter(|f| f.repository_id == repository_id && f.branch == branch)
            .cloned()
            .collect())
    }

    async fn has_running_jobs(&self, repository_id: &str, branch: &str) -> DatabaseResult<bool> {
        self.check_fail()?;

        let jobs = self.jobs.lock().unwrap();
        Ok(jobs.values().any(|j| {
            j.repository_id == repository_id
                && j.branch == branch
                && matches!(j.status, JobStatus::Running | JobStatus::Pending)
        }))
    }

    async fn get_file_metadata(
        &self,
        repository_id: &str,
        branch: &str,
        file_path: &str,
    ) -> DatabaseResult<Option<IndexedFile>> {
        self.check_fail()?;

        let key = (
            repository_id.to_string(),
            branch.to_string(),
            file_path.to_string(),
        );
        let files = self.indexed_files.lock().unwrap();
        Ok(files.get(&key).cloned())
    }

    async fn get_files_metadata(&self, file_paths: &[&str]) -> DatabaseResult<Vec<IndexedFile>> {
        self.check_fail()?;

        let files = self.indexed_files.lock().unwrap();
        let mut results = Vec::new();

        for &file_path in file_paths {
            // Search across all repository/branch combinations for this file path
            for ((_, _, stored_path), file) in files.iter() {
                if stored_path == file_path {
                    results.push(file.clone());
                }
            }
        }

        Ok(results)
    }

    async fn get_project_branch(
        &self,
        repository_id: &str,
        branch: &str,
    ) -> DatabaseResult<Option<ProjectBranch>> {
        self.check_fail()?;

        let key = (repository_id.to_string(), branch.to_string());
        let branches = self.project_branches.lock().unwrap();
        Ok(branches.get(&key).cloned())
    }

    async fn get_project_branches(
        &self,
        repo_branches: &[(String, String)],
    ) -> DatabaseResult<Vec<ProjectBranch>> {
        self.check_fail()?;

        let branches = self.project_branches.lock().unwrap();
        let results = repo_branches
            .iter()
            .filter_map(|(repo_id, branch)| {
                let key = (repo_id.clone(), branch.clone());
                branches.get(&key).cloned()
            })
            .collect();
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_project_branch() {
        let mock = MockFileRepository::new();
        let ctx = RepositoryContext {
            repository_id: "github.com/user/repo".to_string(),
            repository_url: Some("https://github.com/user/repo".to_string()),
            branch: "main".to_string(),
            commit_sha: None,
            commit_message: None,
            commit_date: None,
            author: None,
            is_dirty: false,
            root_path: std::path::PathBuf::from("/tmp"),
        };

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
            .check_file_state("repo", "main", "file.rs", "hash1")
            .await
            .unwrap();
        assert!(matches!(state, FileState::New { generation: 1 }));

        // Record the file
        let metadata = FileMetadata {
            path: "file.rs".to_string(),
            content_hash: "hash1".to_string(),
            generation: 1,
            commit_sha: None,
            commit_message: None,
            commit_date: None,
            author: None,
        };
        mock.record_file_indexing("repo", "main", &metadata)
            .await
            .unwrap();

        // Same hash = unchanged
        let state = mock
            .check_file_state("repo", "main", "file.rs", "hash1")
            .await
            .unwrap();
        assert!(matches!(state, FileState::Unchanged));

        // Different hash = updated
        let state = mock
            .check_file_state("repo", "main", "file.rs", "hash2")
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

        let result = mock.has_running_jobs("repo", "main").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Expected test error");

        // Should work after error is consumed
        let result = mock.has_running_jobs("repo", "main").await;
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

        mock.insert_chunks("repo", "main", chunks).await.unwrap();

        let retrieved = mock
            .get_file_chunks("repo", "main", "file.rs")
            .await
            .unwrap();
        assert_eq!(retrieved.len(), 2);

        // Replace with new generation
        let deleted = mock
            .replace_file_chunks("repo", "main", "file.rs", 2)
            .await
            .unwrap();
        assert_eq!(deleted.len(), 2);
        assert!(deleted.contains(&chunk_id1));
        assert!(deleted.contains(&chunk_id2));

        let remaining = mock
            .get_file_chunks("repo", "main", "file.rs")
            .await
            .unwrap();
        assert_eq!(remaining.len(), 0);
    }
}
