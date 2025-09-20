//! Indexer service trait for dependency injection and testing

use super::IndexResult;
use async_trait::async_trait;

/// Trait for indexing operations to enable dependency injection and testing
#[async_trait]
pub trait IndexerService: Send + Sync {
    /// Index a directory and return the result
    async fn index_directory(
        &mut self,
        path: &std::path::Path,
        recursive: bool,
    ) -> crate::IndexerResult<IndexResult>;

    /// Index file content directly without filesystem access
    async fn index_file_content(
        &mut self,
        project_id: &str,
        files: Vec<FileContent>,
    ) -> crate::IndexerResult<IndexResult>;

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

/// Real implementation of IndexerService using the actual Indexer
pub struct ApiIndexerService {
    indexer: super::Indexer,
}

impl Default for ApiIndexerService {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiIndexerService {
    /// Create a new IndexerServiceImpl with default configuration
    pub fn new() -> Self {
        Self {
            indexer: super::Indexer::new(),
        }
    }

    /// Create a new IndexerServiceImpl with specific configuration
    pub fn with_config(config: &crate::config::Config) -> Self {
        Self {
            indexer: super::Indexer::with_config(config),
        }
    }
}

#[async_trait]
impl IndexerService for ApiIndexerService {
    async fn index_directory(
        &mut self,
        path: &std::path::Path,
        recursive: bool,
    ) -> crate::IndexerResult<IndexResult> {
        self.indexer.index_directory(path, recursive).await
    }

    async fn index_file_content(
        &mut self,
        project_id: &str,
        files: Vec<FileContent>,
    ) -> crate::IndexerResult<IndexResult> {
        self.indexer.index_file_content(project_id, files).await
    }

    async fn drop_collection(&mut self) -> crate::IndexerResult<bool> {
        self.indexer.drop_collection().await
    }
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
        async fn index_directory(
            &mut self,
            _path: &std::path::Path,
            _recursive: bool,
        ) -> crate::IndexerResult<IndexResult> {
            if self.should_error {
                Err(crate::IndexerError::Io("Mock error".to_string()))
            } else {
                Ok(IndexResult {
                    files_indexed: self.files_to_return,
                    chunks_created: self.chunks_to_return,
                    chunks_stored: 0,
                })
            }
        }

        async fn index_file_content(
            &mut self,
            _project_id: &str,
            _files: Vec<FileContent>,
        ) -> crate::IndexerResult<IndexResult> {
            if self.should_error {
                Err(crate::IndexerError::Io("Mock error".to_string()))
            } else {
                Ok(IndexResult {
                    files_indexed: self.files_to_return,
                    chunks_created: self.chunks_to_return,
                    chunks_stored: 0,
                })
            }
        }

        async fn drop_collection(&mut self) -> crate::IndexerResult<bool> {
            if self.should_error {
                Err(crate::IndexerError::Io("Mock error".to_string()))
            } else {
                Ok(true)
            }
        }
    }
}
