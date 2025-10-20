//! Domain models for database entities

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a repository/branch combination
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectBranch {
    pub repository_id: String,
    pub branch: String,
    pub repository_url: Option<String>,
    pub first_seen: DateTime<Utc>,
    pub last_indexed: Option<DateTime<Utc>>,
}

/// Represents an indexed file in the database
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IndexedFile {
    pub repository_id: String,
    pub branch: String,
    pub file_path: String,
    pub file_content: String,
    pub content_hash: String,
    pub encoding: String,
    pub size_bytes: i64,
    pub generation: i64,

    // Git metadata
    pub commit_sha: Option<String>,
    pub commit_message: Option<String>,
    pub commit_date: Option<DateTime<Utc>>,
    pub author: Option<String>,

    // Timestamps
    pub indexed_at: DateTime<Utc>,
}

/// Metadata about a chunk stored in the vector database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub chunk_id: Uuid,
    pub repository_id: String,
    pub branch: String,
    pub file_path: String,
    pub chunk_index: i32,
    pub generation: i64,

    // Semantic info
    pub start_line: i32,
    pub end_line: i32,
    // Byte range info
    pub byte_start: i64,
    pub byte_end: i64,

    pub kind: Option<String>,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Represents a background indexing job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingJob {
    pub job_id: Uuid,
    pub repository_id: String,
    pub branch: String,
    pub status: JobStatus,
    pub files_total: Option<i32>,
    pub files_processed: i32,
    pub chunks_created: i32,
    pub commit_sha: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

/// Status of an indexing job
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl std::str::FromStr for JobStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("Invalid job status: {s}")),
        }
    }
}

impl From<String> for JobStatus {
    fn from(s: String) -> Self {
        s.as_str().parse().unwrap_or(Self::Pending)
    }
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        };
        write!(f, "{status}")
    }
}

/// Represents a file in the indexing job queue (persistent queue)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IndexingJobFile {
    pub id: Uuid,
    pub job_id: Uuid,
    pub repository_id: String,
    pub branch: String,
    pub file_path: String,
    pub file_content: String,
    pub content_hash: String,
    pub status: String,
    pub priority: i32,
    pub retry_count: i32,
    pub error_message: Option<String>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// State of a file when checking for re-indexing
#[derive(Debug, Clone)]
pub enum FileState {
    /// File content hasn't changed, skip indexing
    Unchanged,
    /// File is being indexed for the first time
    New { generation: i64 },
    /// File content has changed and needs re-indexing
    Updated {
        old_generation: i64,
        new_generation: i64,
    },
}

/// Repository context from Git
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryContext {
    pub repository_id: String,
    pub repository_url: Option<String>,
    pub branch: String,
    pub commit_sha: Option<String>,
    pub commit_message: Option<String>,
    pub commit_date: Option<DateTime<Utc>>,
    pub author: Option<String>,
    pub is_dirty: bool,
    pub root_path: std::path::PathBuf,
}

/// File metadata for indexing
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FileMetadata {
    pub path: String,
    pub content: String, // Full file content (converted to UTF-8)
    pub content_hash: String,
    pub encoding: String, // Original encoding detected ("UTF-8", "UTF-16LE", etc.)
    pub size_bytes: i64,  // Original file size (before conversion)
    pub generation: i64,
    pub commit_sha: Option<String>,
    pub commit_message: Option<String>,
    pub commit_date: Option<DateTime<Utc>>,
    pub author: Option<String>,
}

/// Statistics about a project's index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingStats {
    pub total_files: i64,
    pub total_chunks: i64,
    pub total_size_bytes: i64,
    pub last_indexed: Option<DateTime<Utc>>,
    pub unique_commits: i64,
}
