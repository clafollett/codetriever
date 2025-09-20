//! Structured error handling for the data layer
//!
//! Provides comprehensive error types with full context for database operations,
//! connection pool management, and batch processing failures.

use std::fmt;
use thiserror::Error;
use uuid::Uuid;

/// Result type alias for database operations
pub type DatabaseResult<T> = std::result::Result<T, DatabaseError>;

/// Pool type for connection pool identification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionPoolType {
    /// Write pool for INSERT/UPDATE/DELETE operations
    Write,
    /// Read pool for SELECT queries
    Read,
    /// Analytics pool for heavy queries and aggregations
    Analytics,
}

impl fmt::Display for ConnectionPoolType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Write => write!(f, "write"),
            Self::Read => write!(f, "read"),
            Self::Analytics => write!(f, "analytics"),
        }
    }
}

/// Database operation type for error context
#[derive(Debug, Clone)]
pub enum DatabaseOperation {
    /// Project/branch operations
    EnsureProjectBranch {
        repository_id: String,
        branch: String,
    },
    GetProjectBranch {
        repository_id: String,
        branch: String,
    },
    GetProjectBranches {
        count: usize,
    },

    /// File state operations
    CheckFileState {
        repository_id: String,
        branch: String,
        file_path: String,
    },
    RecordFileIndexing {
        repository_id: String,
        branch: String,
        file_path: String,
    },
    GetFileMetadata {
        repository_id: String,
        branch: String,
        file_path: String,
    },
    GetFilesMetadata {
        file_count: usize,
    },
    GetIndexedFiles {
        repository_id: String,
        branch: String,
    },

    /// Chunk operations
    InsertChunks {
        repository_id: String,
        branch: String,
        chunk_count: usize,
    },
    ReplaceFileChunks {
        repository_id: String,
        branch: String,
        file_path: String,
        new_generation: i64,
    },
    GetFileChunks {
        repository_id: String,
        branch: String,
        file_path: String,
    },

    /// Job operations
    CreateIndexingJob {
        repository_id: String,
        branch: String,
    },
    UpdateJobProgress {
        job_id: Uuid,
    },
    CompleteJob {
        job_id: Uuid,
    },
    CheckRunningJobs {
        repository_id: String,
        branch: String,
    },

    /// Generic operations
    Query {
        description: String,
    },
    Transaction {
        description: String,
    },
    Migration {
        version: i64,
    },
}

impl fmt::Display for DatabaseOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnsureProjectBranch {
                repository_id,
                branch,
            } => write!(
                f,
                "ensure_project_branch(repo={repository_id}, branch={branch})"
            ),
            Self::GetProjectBranch {
                repository_id,
                branch,
            } => write!(
                f,
                "get_project_branch(repo={repository_id}, branch={branch})"
            ),
            Self::GetProjectBranches { count } => {
                write!(f, "get_project_branches(count={count})")
            }

            Self::CheckFileState {
                repository_id,
                branch,
                file_path,
            } => write!(
                f,
                "check_file_state(repo={repository_id}, branch={branch}, file={file_path})"
            ),
            Self::RecordFileIndexing {
                repository_id,
                branch,
                file_path,
            } => write!(
                f,
                "record_file_indexing(repo={repository_id}, branch={branch}, file={file_path})"
            ),
            Self::GetFileMetadata {
                repository_id,
                branch,
                file_path,
            } => write!(
                f,
                "get_file_metadata(repo={repository_id}, branch={branch}, file={file_path})"
            ),
            Self::GetFilesMetadata { file_count } => {
                write!(f, "get_files_metadata(count={file_count})")
            }
            Self::GetIndexedFiles {
                repository_id,
                branch,
            } => write!(
                f,
                "get_indexed_files(repo={repository_id}, branch={branch})"
            ),

            Self::InsertChunks {
                repository_id,
                branch,
                chunk_count,
            } => write!(
                f,
                "insert_chunks(repo={repository_id}, branch={branch}, count={chunk_count})"
            ),
            Self::ReplaceFileChunks {
                repository_id,
                branch,
                file_path,
                new_generation,
            } => write!(
                f,
                "replace_file_chunks(repo={repository_id}, branch={branch}, file={file_path}, gen={new_generation})"
            ),
            Self::GetFileChunks {
                repository_id,
                branch,
                file_path,
            } => write!(
                f,
                "get_file_chunks(repo={repository_id}, branch={branch}, file={file_path})"
            ),

            Self::CreateIndexingJob {
                repository_id,
                branch,
            } => write!(
                f,
                "create_indexing_job(repo={repository_id}, branch={branch})"
            ),
            Self::UpdateJobProgress { job_id } => {
                write!(f, "update_job_progress(job_id={job_id})")
            }
            Self::CompleteJob { job_id } => write!(f, "complete_job(job_id={job_id})"),
            Self::CheckRunningJobs {
                repository_id,
                branch,
            } => write!(
                f,
                "check_running_jobs(repo={repository_id}, branch={branch})"
            ),

            Self::Query { description } => write!(f, "query({description})"),
            Self::Transaction { description } => write!(f, "transaction({description})"),
            Self::Migration { version } => write!(f, "migration(v{version})"),
        }
    }
}

/// Comprehensive database error with full context
#[derive(Debug, Error)]
pub enum DatabaseError {
    /// Connection pool exhausted
    #[error(
        "Connection pool exhausted for {pool_type} pool (max={max_connections}, timeout={timeout_secs}s, correlation_id={correlation_id:?})"
    )]
    ConnectionPoolExhausted {
        pool_type: ConnectionPoolType,
        max_connections: u32,
        timeout_secs: u64,
        correlation_id: Option<String>,
    },

    /// Connection pool creation failed
    #[error(
        "Failed to create {pool_type} connection pool: {message} (correlation_id={correlation_id:?})"
    )]
    PoolCreationFailed {
        pool_type: ConnectionPoolType,
        message: String,
        correlation_id: Option<String>,
        #[source]
        source: sqlx::Error,
    },

    /// Database connection error
    #[error(
        "Database connection failed for {pool_type} pool: {message} (correlation_id={correlation_id:?})"
    )]
    ConnectionFailed {
        pool_type: ConnectionPoolType,
        message: String,
        correlation_id: Option<String>,
        #[source]
        source: sqlx::Error,
    },

    /// Query timeout
    #[error(
        "Query timeout for operation '{operation}' (timeout={timeout_secs}s, correlation_id={correlation_id:?})"
    )]
    QueryTimeout {
        operation: Box<DatabaseOperation>,
        timeout_secs: u64,
        correlation_id: Option<String>,
        #[source]
        source: sqlx::Error,
    },

    /// Query execution error
    #[error(
        "Query failed for operation '{operation}': {message} (correlation_id={correlation_id:?})"
    )]
    QueryFailed {
        operation: Box<DatabaseOperation>,
        message: String,
        correlation_id: Option<String>,
        #[source]
        source: sqlx::Error,
    },

    /// Constraint violation
    #[error(
        "Database constraint violation in table '{table}': {constraint} (operation='{operation}', correlation_id={correlation_id:?})"
    )]
    ConstraintViolation {
        table: String,
        constraint: String,
        operation: Box<DatabaseOperation>,
        correlation_id: Option<String>,
        #[source]
        source: sqlx::Error,
    },

    /// Batch operation partial failure
    #[error(
        "Batch operation '{operation}' partially failed: {successful_count}/{total_count} succeeded (correlation_id={correlation_id:?})"
    )]
    BatchOperationFailed {
        operation: Box<DatabaseOperation>,
        total_count: usize,
        successful_count: usize,
        failed_items: Vec<String>,
        correlation_id: Option<String>,
        #[source]
        source: Option<Box<DatabaseError>>,
    },

    /// Transaction rollback
    #[error(
        "Transaction rolled back for operation '{operation}': {reason} (correlation_id={correlation_id:?})"
    )]
    TransactionRollback {
        operation: Box<DatabaseOperation>,
        reason: String,
        correlation_id: Option<String>,
        #[source]
        source: Option<sqlx::Error>,
    },

    /// Data integrity error
    #[error(
        "Data integrity error: {message} (operation='{operation}', correlation_id={correlation_id:?})"
    )]
    DataIntegrityError {
        operation: Box<DatabaseOperation>,
        message: String,
        correlation_id: Option<String>,
    },

    /// Migration error
    #[error(
        "Database migration failed at version {version}: {message} (correlation_id={correlation_id:?})"
    )]
    MigrationFailed {
        version: i64,
        message: String,
        correlation_id: Option<String>,
        #[source]
        source: sqlx::migrate::MigrateError,
    },

    /// Configuration error
    #[error("Database configuration error: {message}")]
    ConfigurationError { message: String },

    /// Unexpected database state
    #[error(
        "Unexpected database state for operation '{operation}': {message} (correlation_id={correlation_id:?})"
    )]
    UnexpectedState {
        operation: Box<DatabaseOperation>,
        message: String,
        correlation_id: Option<String>,
    },
}

impl DatabaseError {
    /// Create a new query failed error from `sqlx::Error`
    pub fn query_failed(
        operation: DatabaseOperation,
        source: sqlx::Error,
        correlation_id: Option<String>,
    ) -> Self {
        let message = source.to_string();

        // Check for specific error types
        if let Some(db_err) = source.as_database_error() {
            // Check for constraint violations
            if let Some(constraint) = db_err.constraint() {
                let table = Self::extract_table_from_constraint(constraint)
                    .unwrap_or_else(|| "unknown".to_string());

                return Self::ConstraintViolation {
                    table,
                    constraint: constraint.to_string(),
                    operation: Box::new(operation),
                    correlation_id,
                    source,
                };
            }
        }

        // Check for timeout errors
        if message.contains("timeout") || message.contains("timed out") {
            return Self::QueryTimeout {
                operation: Box::new(operation),
                timeout_secs: 30, // Default timeout, should be passed from config
                correlation_id,
                source,
            };
        }

        // Generic query failure
        Self::QueryFailed {
            operation: Box::new(operation),
            message,
            correlation_id,
            source,
        }
    }

    /// Create a connection failed error
    pub fn connection_failed(
        pool_type: ConnectionPoolType,
        source: sqlx::Error,
        correlation_id: Option<String>,
    ) -> Self {
        Self::ConnectionFailed {
            pool_type,
            message: source.to_string(),
            correlation_id,
            source,
        }
    }

    /// Create a pool exhausted error
    pub const fn pool_exhausted(
        pool_type: ConnectionPoolType,
        max_connections: u32,
        timeout_secs: u64,
        correlation_id: Option<String>,
    ) -> Self {
        Self::ConnectionPoolExhausted {
            pool_type,
            max_connections,
            timeout_secs,
            correlation_id,
        }
    }

    /// Add correlation ID to existing error
    #[must_use]
    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        match &mut self {
            Self::ConnectionPoolExhausted {
                correlation_id: id, ..
            }
            | Self::PoolCreationFailed {
                correlation_id: id, ..
            }
            | Self::ConnectionFailed {
                correlation_id: id, ..
            }
            | Self::QueryTimeout {
                correlation_id: id, ..
            }
            | Self::QueryFailed {
                correlation_id: id, ..
            }
            | Self::ConstraintViolation {
                correlation_id: id, ..
            }
            | Self::BatchOperationFailed {
                correlation_id: id, ..
            }
            | Self::TransactionRollback {
                correlation_id: id, ..
            }
            | Self::DataIntegrityError {
                correlation_id: id, ..
            }
            | Self::MigrationFailed {
                correlation_id: id, ..
            }
            | Self::UnexpectedState {
                correlation_id: id, ..
            } => {
                *id = Some(correlation_id);
            }
            Self::ConfigurationError { .. } => {
                // Configuration errors don't have correlation IDs
            }
        }
        self
    }

    /// Get the correlation ID if present
    pub fn correlation_id(&self) -> Option<&str> {
        match self {
            Self::ConnectionPoolExhausted { correlation_id, .. }
            | Self::PoolCreationFailed { correlation_id, .. }
            | Self::ConnectionFailed { correlation_id, .. }
            | Self::QueryTimeout { correlation_id, .. }
            | Self::QueryFailed { correlation_id, .. }
            | Self::ConstraintViolation { correlation_id, .. }
            | Self::BatchOperationFailed { correlation_id, .. }
            | Self::TransactionRollback { correlation_id, .. }
            | Self::DataIntegrityError { correlation_id, .. }
            | Self::MigrationFailed { correlation_id, .. }
            | Self::UnexpectedState { correlation_id, .. } => correlation_id.as_deref(),
            Self::ConfigurationError { .. } => None,
        }
    }

    /// Extract table name from constraint name (assumes format: `table_constraint`)
    fn extract_table_from_constraint(constraint: &str) -> Option<String> {
        constraint.split('_').next().map(String::from)
    }
}

/// Extension trait for converting sqlx errors with context
#[allow(clippy::result_large_err)]
pub trait DatabaseErrorExt<T> {
    /// Convert to `DatabaseError` with operation context
    ///
    /// # Errors
    /// Returns `DatabaseError` with operation context and correlation ID
    fn map_db_err(
        self,
        operation: DatabaseOperation,
        correlation_id: Option<String>,
    ) -> DatabaseResult<T>;

    /// Convert to `DatabaseError` with operation context and custom error mapping
    ///
    /// # Errors
    /// Returns `DatabaseError` from custom mapping function
    fn map_db_err_with<F>(
        self,
        operation: DatabaseOperation,
        correlation_id: Option<String>,
        f: F,
    ) -> DatabaseResult<T>
    where
        F: FnOnce(sqlx::Error) -> DatabaseError;
}

impl<T> DatabaseErrorExt<T> for std::result::Result<T, sqlx::Error> {
    fn map_db_err(
        self,
        operation: DatabaseOperation,
        correlation_id: Option<String>,
    ) -> DatabaseResult<T> {
        self.map_err(|e| DatabaseError::query_failed(operation, e, correlation_id))
    }

    #[allow(clippy::missing_errors_doc)]
    fn map_db_err_with<F>(
        self,
        _operation: DatabaseOperation,
        correlation_id: Option<String>,
        f: F,
    ) -> DatabaseResult<T>
    where
        F: FnOnce(sqlx::Error) -> DatabaseError,
    {
        self.map_err(|e| {
            let err = f(e);
            // Ensure correlation ID is set
            if let Some(id) = correlation_id {
                if err.correlation_id().is_none() {
                    err.with_correlation_id(id)
                } else {
                    err
                }
            } else {
                err
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_type_display() {
        assert_eq!(ConnectionPoolType::Write.to_string(), "write");
        assert_eq!(ConnectionPoolType::Read.to_string(), "read");
        assert_eq!(ConnectionPoolType::Analytics.to_string(), "analytics");
    }

    #[test]
    fn test_database_operation_display() {
        let op = DatabaseOperation::InsertChunks {
            repository_id: "repo123".to_string(),
            branch: "main".to_string(),
            chunk_count: 42,
        };
        assert_eq!(
            op.to_string(),
            "insert_chunks(repo=repo123, branch=main, count=42)"
        );
    }

    #[test]
    fn test_correlation_id_propagation() {
        let error = DatabaseError::pool_exhausted(ConnectionPoolType::Write, 10, 30, None);

        let error_with_id = error.with_correlation_id("test-123".to_string());
        assert_eq!(error_with_id.correlation_id(), Some("test-123"));
    }
}
