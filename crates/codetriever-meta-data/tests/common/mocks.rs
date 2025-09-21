//! Comprehensive mock implementations for testing the repository layer
//!
//! This module provides fully-featured mocks that can simulate various
//! database behaviors including errors, delays, and state management.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::time::{sleep, Duration};
use uuid::Uuid;

use crate::models::{
    ChunkMetadata, FileMetadata, FileState, IndexedFile, IndexingJob, JobStatus, ProjectBranch,
    RepositoryContext,
};
use crate::traits::FileRepository;

/// Configuration for mock behavior
#[derive(Debug, Clone)]
pub struct MockConfig {
    /// Simulate network delay in milliseconds
    pub latency_ms: u64,
    /// Probability of random failures (0.0-1.0)
    pub failure_rate: f32,
    /// Enable state persistence across calls
    pub persist_state: bool,
    /// Simulate database constraints
    pub enforce_constraints: bool,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            latency_ms: 0,
            failure_rate: 0.0,
            persist_state: true,
            enforce_constraints: true,
        }
    }
}

/// Mock repository state
#[derive(Debug, Default)]
struct MockState {
    project_branches: HashMap<(String, String), ProjectBranch>,
    indexed_files: HashMap<(String, String, String), IndexedFile>,
    chunks: HashMap<(String, String, String), Vec<ChunkMetadata>>,
    jobs: HashMap<Uuid, IndexingJob>,
    next_generation: HashMap<(String, String, String), i64>,
}

/// Mock repository for testing
pub struct MockFileRepository {
    state: Arc<Mutex<MockState>>,
    config: MockConfig,
    call_count: Arc<Mutex<HashMap<String, usize>>>,
}

impl MockFileRepository {
    pub fn new(config: MockConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(MockState::default())),
            config,
            call_count: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(MockConfig::default())
    }

    /// Configure to fail on specific operations
    pub fn with_failure_rate(mut self, rate: f32) -> Self {
        self.config.failure_rate = rate;
        self
    }

    /// Configure to simulate latency
    pub fn with_latency(mut self, latency_ms: u64) -> Self {
        self.config.latency_ms = latency_ms;
        self
    }

    /// Get call count for a specific method
    pub fn get_call_count(&self, method: &str) -> usize {
        self.call_count
            .lock()
            .unwrap()
            .get(method)
            .copied()
            .unwrap_or(0)
    }

    /// Reset all call counts
    pub fn reset_call_counts(&self) {
        self.call_count.lock().unwrap().clear();
    }

    /// Clear all mock state
    pub fn clear_state(&self) {
        let mut state = self.state.lock().unwrap();
        state.project_branches.clear();
        state.indexed_files.clear();
        state.chunks.clear();
        state.jobs.clear();
        state.next_generation.clear();
    }

    /// Inject test data
    pub fn inject_indexed_file(&self, file: IndexedFile) {
        let mut state = self.state.lock().unwrap();
        let key = (
            file.repository_id.clone(),
            file.branch.clone(),
            file.file_path.clone(),
        );
        state.indexed_files.insert(key, file);
    }

    /// Inject test chunks
    pub fn inject_chunks(&self, repository_id: &str, branch: &str, file_path: &str, chunks: Vec<ChunkMetadata>) {
        let mut state = self.state.lock().unwrap();
        let key = (
            repository_id.to_string(),
            branch.to_string(),
            file_path.to_string(),
        );
        state.chunks.insert(key, chunks);
    }

    /// Track method calls
    fn track_call(&self, method: &str) {
        let mut counts = self.call_count.lock().unwrap();
        *counts.entry(method.to_string()).or_insert(0) += 1;
    }

    /// Simulate potential failure
    async fn maybe_fail(&self, operation: &str) -> Result<()> {
        if self.config.failure_rate > 0.0 {
            let should_fail = rand::random::<f32>() < self.config.failure_rate;
            if should_fail {
                return Err(anyhow::anyhow!("Mock failure in {}", operation));
            }
        }
        Ok(())
    }

    /// Simulate latency
    async fn simulate_latency(&self) {
        if self.config.latency_ms > 0 {
            sleep(Duration::from_millis(self.config.latency_ms)).await;
        }
    }
}

#[async_trait]
impl FileRepository for MockFileRepository {
    async fn ensure_project_branch(&self, ctx: &RepositoryContext) -> Result<ProjectBranch> {
        self.track_call("ensure_project_branch");
        self.simulate_latency().await;
        self.maybe_fail("ensure_project_branch").await?;

        let mut state = self.state.lock().unwrap();
        let key = (ctx.repository_id.clone(), ctx.branch.clone());
        
        let project = state
            .project_branches
            .entry(key)
            .or_insert_with(|| ProjectBranch {
                repository_id: ctx.repository_id.clone(),
                branch: ctx.branch.clone(),
                repository_url: ctx.repository_url.clone(),
                first_seen: chrono::Utc::now(),
                last_indexed: None,
            });
        
        Ok(project.clone())
    }

    async fn check_file_state(
        &self,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        content_hash: &str,
    ) -> Result<FileState> {
        self.track_call("check_file_state");
        self.simulate_latency().await;
        self.maybe_fail("check_file_state").await?;

        let state = self.state.lock().unwrap();
        let key = (
            repository_id.to_string(),
            branch.to_string(),
            file_path.to_string(),
        );
        
        if let Some(file) = state.indexed_files.get(&key) {
            if file.content_hash == content_hash {
                Ok(FileState::Unchanged)
            } else {
                Ok(FileState::Updated {
                    old_generation: file.generation,
                    new_generation: file.generation + 1,
                })
            }
        } else {
            Ok(FileState::New { generation: 1 })
        }
    }

    async fn record_file_indexing(
        &self,
        repository_id: &str,
        branch: &str,
        metadata: &FileMetadata,
    ) -> Result<IndexedFile> {
        self.track_call("record_file_indexing");
        self.simulate_latency().await;
        self.maybe_fail("record_file_indexing").await?;

        let mut state = self.state.lock().unwrap();
        let key = (
            repository_id.to_string(),
            branch.to_string(),
            metadata.path.clone(),
        );
        
        // Update generation tracking
        state.next_generation.insert(key.clone(), metadata.generation + 1);
        
        let indexed_file = IndexedFile {
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
            file_path: metadata.path.clone(),
            content_hash: metadata.content_hash.clone(),
            generation: metadata.generation,
            commit_sha: metadata.commit_sha.clone(),
            commit_message: metadata.commit_message.clone(),
            commit_date: metadata.commit_date,
            author: metadata.author.clone(),
            indexed_at: chrono::Utc::now(),
        };
        
        state.indexed_files.insert(key, indexed_file.clone());
        Ok(indexed_file)
    }

    async fn insert_chunks(
        &self,
        repository_id: &str,
        branch: &str,
        chunks: Vec<ChunkMetadata>,
    ) -> Result<()> {
        self.track_call("insert_chunks");
        self.simulate_latency().await;
        self.maybe_fail("insert_chunks").await?;

        if chunks.is_empty() {
            return Ok(());
        }

        let mut state = self.state.lock().unwrap();
        
        // Group chunks by file
        let mut chunks_by_file: HashMap<String, Vec<ChunkMetadata>> = HashMap::new();
        for chunk in chunks {
            chunks_by_file
                .entry(chunk.file_path.clone())
                .or_insert_with(Vec::new)
                .push(chunk);
        }
        
        // Store chunks
        for (file_path, file_chunks) in chunks_by_file {
            let key = (
                repository_id.to_string(),
                branch.to_string(),
                file_path,
            );
            state.chunks.insert(key, file_chunks);
        }
        
        Ok(())
    }

    async fn replace_file_chunks(
        &self,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        new_generation: i64,
    ) -> Result<Vec<Uuid>> {
        self.track_call("replace_file_chunks");
        self.simulate_latency().await;
        self.maybe_fail("replace_file_chunks").await?;

        let mut state = self.state.lock().unwrap();
        let key = (
            repository_id.to_string(),
            branch.to_string(),
            file_path.to_string(),
        );
        
        // Get old chunk IDs
        let old_ids = state
            .chunks
            .get(&key)
            .map(|chunks| {
                chunks
                    .iter()
                    .filter(|c| c.generation < new_generation)
                    .map(|c| c.chunk_id)
                    .collect()
            })
            .unwrap_or_else(Vec::new);
        
        // Remove old chunks
        if let Some(chunks) = state.chunks.get_mut(&key) {
            chunks.retain(|c| c.generation >= new_generation);
        }
        
        Ok(old_ids)
    }

    async fn create_indexing_job(
        &self,
        repository_id: &str,
        branch: &str,
        commit_sha: Option<&str>,
    ) -> Result<IndexingJob> {
        self.track_call("create_indexing_job");
        self.simulate_latency().await;
        self.maybe_fail("create_indexing_job").await?;

        let job = IndexingJob {
            job_id: Uuid::new_v4(),
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
            status: JobStatus::Running,
            files_total: None,
            files_processed: 0,
            chunks_created: 0,
            commit_sha: commit_sha.map(|s| s.to_string()),
            started_at: chrono::Utc::now(),
            completed_at: None,
            error_message: None,
        };
        
        let mut state = self.state.lock().unwrap();
        state.jobs.insert(job.job_id, job.clone());
        
        Ok(job)
    }

    async fn update_job_progress(
        &self,
        job_id: &Uuid,
        files_processed: i32,
        chunks_created: i32,
    ) -> Result<()> {
        self.track_call("update_job_progress");
        self.simulate_latency().await;
        self.maybe_fail("update_job_progress").await?;

        let mut state = self.state.lock().unwrap();
        
        if let Some(job) = state.jobs.get_mut(job_id) {
            job.files_processed += files_processed;
            job.chunks_created += chunks_created;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Job not found"))
        }
    }

    async fn complete_job(
        &self,
        job_id: &Uuid,
        status: JobStatus,
        error: Option<String>,
    ) -> Result<()> {
        self.track_call("complete_job");
        self.simulate_latency().await;
        self.maybe_fail("complete_job").await?;

        let mut state = self.state.lock().unwrap();
        
        if let Some(job) = state.jobs.get_mut(job_id) {
            job.status = status;
            job.completed_at = Some(chrono::Utc::now());
            job.error_message = error;
            
            // Update last_indexed if completed successfully
            if status == JobStatus::Completed {
                let key = (job.repository_id.clone(), job.branch.clone());
                if let Some(project) = state.project_branches.get_mut(&key) {
                    project.last_indexed = Some(chrono::Utc::now());
                }
            }
            
            Ok(())
        } else {
            Err(anyhow::anyhow!("Job not found"))
        }
    }

    async fn get_file_chunks(
        &self,
        repository_id: &str,
        branch: &str,
        file_path: &str,
    ) -> Result<Vec<ChunkMetadata>> {
        self.track_call("get_file_chunks");
        self.simulate_latency().await;
        self.maybe_fail("get_file_chunks").await?;

        let state = self.state.lock().unwrap();
        let key = (
            repository_id.to_string(),
            branch.to_string(),
            file_path.to_string(),
        );
        
        Ok(state.chunks.get(&key).cloned().unwrap_or_default())
    }

    async fn get_indexed_files(
        &self,
        repository_id: &str,
        branch: &str,
    ) -> Result<Vec<IndexedFile>> {
        self.track_call("get_indexed_files");
        self.simulate_latency().await;
        self.maybe_fail("get_indexed_files").await?;

        let state = self.state.lock().unwrap();
        
        let files: Vec<IndexedFile> = state
            .indexed_files
            .iter()
            .filter(|((r, b, _), _)| r == repository_id && b == branch)
            .map(|(_, file)| file.clone())
            .collect();
        
        Ok(files)
    }

    async fn has_running_jobs(&self, repository_id: &str, branch: &str) -> Result<bool> {
        self.track_call("has_running_jobs");
        self.simulate_latency().await;
        self.maybe_fail("has_running_jobs").await?;

        let state = self.state.lock().unwrap();
        
        let has_running = state.jobs.values().any(|job| {
            job.repository_id == repository_id
                && job.branch == branch
                && matches!(job.status, JobStatus::Running | JobStatus::Pending)
        });
        
        Ok(has_running)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_basic_operations() {
        let mock = MockFileRepository::with_defaults();
        
        // Test project branch creation
        let ctx = RepositoryContext {
            repository_id: "test_repo".to_string(),
            branch: "main".to_string(),
            repository_url: "https://github.com/test/repo".to_string(),
        };
        
        let project = mock.ensure_project_branch(&ctx).await.unwrap();
        assert_eq!(project.repository_id, "test_repo");
        assert_eq!(project.branch, "main");
        
        // Test file state checking
        let state = mock
            .check_file_state("test_repo", "main", "test.rs", "hash123")
            .await
            .unwrap();
        assert!(matches!(state, FileState::New { generation: 1 }));
        
        // Verify call tracking
        assert_eq!(mock.get_call_count("ensure_project_branch"), 1);
        assert_eq!(mock.get_call_count("check_file_state"), 1);
    }

    #[tokio::test]
    async fn test_mock_with_failures() {
        let mock = MockFileRepository::new(MockConfig {
            failure_rate: 1.0, // Always fail
            ..Default::default()
        });
        
        let ctx = RepositoryContext {
            repository_id: "test_repo".to_string(),
            branch: "main".to_string(),
            repository_url: "https://github.com/test/repo".to_string(),
        };
        
        let result = mock.ensure_project_branch(&ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_state_persistence() {
        let mock = MockFileRepository::with_defaults();
        
        // Inject test data
        let file = IndexedFile {
            repository_id: "test_repo".to_string(),
            branch: "main".to_string(),
            file_path: "test.rs".to_string(),
            content_hash: "hash123".to_string(),
            generation: 1,
            commit_sha: Some("abc123".to_string()),
            commit_message: Some("Initial commit".to_string()),
            commit_date: None,
            author: Some("Test Author".to_string()),
            indexed_at: chrono::Utc::now(),
        };
        
        mock.inject_indexed_file(file.clone());
        
        // Verify file state reflects injected data
        let state = mock
            .check_file_state("test_repo", "main", "test.rs", "hash123")
            .await
            .unwrap();
        assert!(matches!(state, FileState::Unchanged));
        
        // Check with different hash
        let state = mock
            .check_file_state("test_repo", "main", "test.rs", "hash456")
            .await
            .unwrap();
        assert!(matches!(state, FileState::Updated { old_generation: 1, new_generation: 2 }));
    }
}