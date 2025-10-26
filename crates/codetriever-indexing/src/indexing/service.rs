//! Indexer service trait for dependency injection and testing

use super::IndexResult;
use async_trait::async_trait;
use codetriever_meta_data::models::IndexingJob;
use uuid::Uuid;

/// Trait for indexing operations to enable dependency injection and testing
#[async_trait]
pub trait IndexerService: Send + Sync {
    /// Index file content directly without filesystem access (synchronous, blocking)
    ///
    /// This method processes files synchronously and blocks until completion.
    /// Use for tests and CLI tools. For API endpoints, use `start_indexing_job()` instead.
    async fn index_file_content(
        &mut self,
        project_id: &str,
        files: Vec<FileContent>,
    ) -> crate::IndexerResult<IndexResult>;

    /// Start an asynchronous indexing job (non-blocking, returns immediately)
    ///
    /// Enqueues files to the persistent queue and returns a job ID.
    /// The job is processed asynchronously by background workers.
    /// Use this for API endpoints to avoid blocking HTTP requests.
    ///
    /// # Returns
    /// - `Uuid`: Job ID for tracking progress via `get_job_status()`
    async fn start_indexing_job(
        &mut self,
        project_id: &str,
        files: Vec<FileContent>,
    ) -> crate::IndexerResult<Uuid>;

    /// Get the current status of an indexing job
    ///
    /// # Returns
    /// - `IndexingJob`: Job metadata including status, progress, and any errors
    async fn get_job_status(&mut self, job_id: &Uuid) -> crate::IndexerResult<Option<IndexingJob>>;

    /// List all indexing jobs, optionally filtered by project
    async fn list_jobs(
        &mut self,
        project_id: Option<&str>,
    ) -> crate::IndexerResult<Vec<IndexingJob>>;

    /// Drop the collection from storage
    async fn drop_collection(&mut self) -> crate::IndexerResult<bool>;
}

/// Represents a file with its content for indexing
#[derive(Debug, Clone)]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub hash: String,
}

/// Test utilities for mocking IndexerService
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils {
    use super::*;

    /// Mock implementation of IndexerService for testing
    pub struct MockIndexerService {
        pub files_to_return: usize,
        pub chunks_to_return: usize,
        pub should_error: bool,
    }

    impl MockIndexerService {
        pub fn new(files: usize, chunks: usize) -> Self {
            Self {
                files_to_return: files,
                chunks_to_return: chunks,
                should_error: false,
            }
        }

        pub fn with_error() -> Self {
            Self {
                files_to_return: 0,
                chunks_to_return: 0,
                should_error: true,
            }
        }
    }

    #[async_trait]
    impl IndexerService for MockIndexerService {
        async fn index_file_content(
            &mut self,
            _project_id: &str,
            _files: Vec<FileContent>,
        ) -> crate::IndexerResult<IndexResult> {
            if self.should_error {
                Err(crate::IndexerError::Io {
                    message: "Mock error".to_string(),
                    source: None,
                })
            } else {
                Ok(IndexResult {
                    files_indexed: self.files_to_return,
                    chunks_created: self.chunks_to_return,
                    chunks_stored: 0,
                })
            }
        }

        async fn start_indexing_job(
            &mut self,
            _project_id: &str,
            _files: Vec<FileContent>,
        ) -> crate::IndexerResult<Uuid> {
            // Mock returns a test job ID
            Ok(Uuid::new_v4())
        }

        async fn get_job_status(
            &mut self,
            _job_id: &Uuid,
        ) -> crate::IndexerResult<Option<IndexingJob>> {
            // Mock returns None (job not found)
            Ok(None)
        }

        async fn list_jobs(
            &mut self,
            _project_id: Option<&str>,
        ) -> crate::IndexerResult<Vec<IndexingJob>> {
            // Mock returns empty list
            Ok(vec![])
        }

        async fn drop_collection(&mut self) -> crate::IndexerResult<bool> {
            if self.should_error {
                Err(crate::IndexerError::Io {
                    message: "Mock error".to_string(),
                    source: None,
                })
            } else {
                Ok(true)
            }
        }
    }
}
