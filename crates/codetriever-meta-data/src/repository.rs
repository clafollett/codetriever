//! Database repository layer with optimized connection pool separation
//!
//! Uses separate connection pools for different operation types to prevent
//! resource contention and improve performance.

use async_trait::async_trait;
use sqlx::Row;
use uuid::Uuid;

use crate::error::{DatabaseError, DatabaseErrorExt, DatabaseOperation, DatabaseResult};
use crate::models::{
    ChunkMetadata, CommitContext, DequeuedFile, FileMetadata, FileState, IndexedFile, IndexingJob,
    JobStatus, ProjectBranch, RepositoryContext,
};
use crate::pool_manager::PoolManager;
use crate::traits::FileRepository;

/// Repository for database operations with optimized connection pools
pub struct DbFileRepository {
    pools: PoolManager,
}

impl DbFileRepository {
    /// Create new repository with optimized connection pools
    pub const fn new(pools: PoolManager) -> Self {
        Self { pools }
    }

    /// Create from environment with optimized pools
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `DATABASE_URL` environment variable is not set
    /// - Database connection fails
    /// - Pool manager creation fails (see `PoolManager::from_env` errors)
    pub async fn from_env() -> std::result::Result<Self, anyhow::Error> {
        let pools = PoolManager::from_env().await?;
        Ok(Self::new(pools))
    }

    /// Count total project branches across all repositories
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn count_project_branches(&self) -> DatabaseResult<i64> {
        let pool = self.pools.read_pool();
        let correlation_id = None;

        let operation = DatabaseOperation::CountProjectBranches;

        let row = sqlx::query("SELECT COUNT(*) as count FROM project_branches")
            .fetch_one(pool)
            .await
            .map_db_err(operation, correlation_id)?;

        Ok(row.get("count"))
    }

    /// Count total indexed files across all repositories and branches
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn count_indexed_files(&self) -> DatabaseResult<i64> {
        let pool = self.pools.read_pool();
        let correlation_id = None;

        let operation = DatabaseOperation::CountIndexedFiles;

        let row = sqlx::query("SELECT COUNT(*) as count FROM indexed_files")
            .fetch_one(pool)
            .await
            .map_db_err(operation, correlation_id)?;

        Ok(row.get("count"))
    }

    /// Count total chunks across all repositories and branches
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn count_chunks(&self) -> DatabaseResult<i64> {
        let pool = self.pools.read_pool();
        let correlation_id = None;

        let operation = DatabaseOperation::CountChunks;

        let row = sqlx::query("SELECT COUNT(*) as count FROM chunk_metadata")
            .fetch_one(pool)
            .await
            .map_db_err(operation, correlation_id)?;

        Ok(row.get("count"))
    }

    /// Get database size in megabytes (`PostgreSQL` only)
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn get_database_size_mb(&self) -> DatabaseResult<f64> {
        let pool = self.pools.read_pool();
        let correlation_id = None;

        let operation = DatabaseOperation::GetDatabaseSize;

        let row = sqlx::query("SELECT pg_database_size(current_database())::BIGINT as size_bytes")
            .fetch_one(pool)
            .await
            .map_db_err(operation, correlation_id)?;

        let size_bytes: i64 = row.get("size_bytes");
        // Convert bytes to megabytes and round to 2 decimal places
        #[allow(clippy::cast_precision_loss)]
        let size_mb = size_bytes as f64 / 1_048_576.0;
        Ok((size_mb * 100.0).round() / 100.0)
    }

    /// Get most recent indexed timestamp across all project branches
    ///
    /// Returns `None` if no branches have been indexed yet
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn get_last_indexed_timestamp(
        &self,
    ) -> DatabaseResult<Option<chrono::DateTime<chrono::Utc>>> {
        let pool = self.pools.read_pool();
        let correlation_id = None;

        let operation = DatabaseOperation::GetLastIndexedTimestamp;

        let row = sqlx::query("SELECT MAX(last_indexed) as last_indexed FROM project_branches")
            .fetch_one(pool)
            .await
            .map_db_err(operation, correlation_id)?;

        Ok(row.get("last_indexed"))
    }

    /// Get full file content by path
    ///
    /// # Errors
    ///
    /// Returns error if database query fails or file not found
    /// Get file content with metadata (`repository_id`, branch, content)
    ///
    /// Returns tuple of (`repository_id`, branch, content) if found, None otherwise
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn get_file_content(
        &self,
        repository_id: Option<&str>,
        branch: Option<&str>,
        file_path: &str,
    ) -> DatabaseResult<Option<(String, String, String)>> {
        let pool = self.pools.read_pool();
        let correlation_id = None;

        let operation = DatabaseOperation::GetFileMetadata {
            repository_id: repository_id.unwrap_or("unknown").to_string(),
            branch: branch.unwrap_or("unknown").to_string(),
            file_path: file_path.to_string(),
        };

        // Build conditional WHERE clause based on which parameters are provided
        let row = match (repository_id, branch) {
            (Some(repo), Some(br)) => {
                // Both provided - exact match
                sqlx::query(
                    r"
                    SELECT repository_id, branch, file_content
                    FROM indexed_files
                    WHERE repository_id = $1 AND branch = $2 AND file_path = $3
                    ",
                )
                .bind(repo)
                .bind(br)
                .bind(file_path)
                .fetch_optional(pool)
                .await
                .map_db_err(operation, correlation_id)?
            }
            (Some(repo), None) => {
                // Repository provided, branch not - find in any branch (prefer main/master)
                sqlx::query(
                    r"
                    SELECT repository_id, branch, file_content
                    FROM indexed_files
                    WHERE repository_id = $1 AND file_path = $2
                    ORDER BY
                        CASE
                            WHEN branch = 'main' THEN 1
                            WHEN branch = 'master' THEN 2
                            ELSE 3
                        END,
                        last_indexed DESC
                    LIMIT 1
                    ",
                )
                .bind(repo)
                .bind(file_path)
                .fetch_optional(pool)
                .await
                .map_db_err(operation, correlation_id)?
            }
            (None, Some(br)) => {
                // Branch provided, repository not - find most recently indexed repository
                sqlx::query(
                    r"
                    SELECT repository_id, branch, file_content
                    FROM indexed_files
                    WHERE branch = $1 AND file_path = $2
                    ORDER BY last_indexed DESC
                    LIMIT 1
                    ",
                )
                .bind(br)
                .bind(file_path)
                .fetch_optional(pool)
                .await
                .map_db_err(operation, correlation_id)?
            }
            (None, None) => {
                // Neither provided - find most recently indexed file across all repos/branches
                sqlx::query(
                    r"
                    SELECT repository_id, branch, file_content
                    FROM indexed_files
                    WHERE file_path = $1
                    ORDER BY last_indexed DESC
                    LIMIT 1
                    ",
                )
                .bind(file_path)
                .fetch_optional(pool)
                .await
                .map_db_err(operation, correlation_id)?
            }
        };

        Ok(row.map(|r| {
            (
                r.get("repository_id"),
                r.get("branch"),
                r.get("file_content"),
            )
        }))
    }

    /// Enqueue a file for indexing (persistent queue)
    ///
    /// # Errors
    ///
    /// Returns error if database insert fails
    pub async fn enqueue_file(
        &self,
        job_id: &uuid::Uuid,
        tenant_id: &uuid::Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        file_content: &str,
        content_hash: &str,
    ) -> DatabaseResult<()> {
        let pool = self.pools.write_pool();
        let correlation_id = None;

        let operation = DatabaseOperation::Query {
            description: "enqueue_file".to_string(),
        };

        sqlx::query(
            r"
            INSERT INTO indexing_job_file_queue
            (job_id, tenant_id, repository_id, branch, file_path, file_content, content_hash, status, priority)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'queued', 0)
            ",
        )
        .bind(job_id)
        .bind(tenant_id)
        .bind(repository_id)
        .bind(branch)
        .bind(file_path)
        .bind(file_content)
        .bind(content_hash)
        .execute(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        Ok(())
    }

    /// Dequeue next file from global queue (atomic with FOR UPDATE SKIP LOCKED)
    ///
    /// Pulls the next file from ANY tenant's jobs in FIFO order (`created_at`).
    /// This enables fair scheduling and maximum concurrency across all jobs/tenants.
    ///
    /// Returns file with `tenant_id` in payload for downstream isolation.
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn dequeue_file(&self) -> DatabaseResult<Option<DequeuedFile>> {
        let pool = self.pools.write_pool();
        let correlation_id = None;

        let operation = DatabaseOperation::Query {
            description: "dequeue_file_global".to_string(),
        };

        let row = sqlx::query(
            r"
            WITH updated AS (
                UPDATE indexing_job_file_queue
                SET status = 'processing', started_at = NOW()
                WHERE id = (
                    SELECT id FROM indexing_job_file_queue
                    WHERE status = 'queued'
                    ORDER BY priority DESC, created_at ASC
                    LIMIT 1
                    FOR UPDATE SKIP LOCKED
                )
                RETURNING tenant_id, job_id, file_path, file_content, content_hash
            )
            SELECT
                u.tenant_id, u.job_id, u.file_path, u.file_content, u.content_hash,
                j.vector_namespace
            FROM updated u
            JOIN indexing_jobs j ON j.job_id = u.job_id
            ",
        )
        .fetch_optional(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        Ok(row.map(|r| DequeuedFile {
            tenant_id: r.get("tenant_id"),
            job_id: r.get("job_id"),
            file_path: r.get("file_path"),
            file_content: r.get("file_content"),
            content_hash: r.get("content_hash"),
            vector_namespace: r.get("vector_namespace"),
        }))
    }

    /// Get queue depth for a job (count of queued + processing files)
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn get_queue_depth(&self, job_id: &uuid::Uuid) -> DatabaseResult<i64> {
        let pool = self.pools.read_pool();
        let correlation_id = None;

        let operation = DatabaseOperation::Query {
            description: "get_queue_depth".to_string(),
        };

        let row = sqlx::query(
            r"
            SELECT COUNT(*) as count
            FROM indexing_job_file_queue
            WHERE job_id = $1 AND status IN ('queued', 'processing')
            ",
        )
        .bind(job_id)
        .fetch_one(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        Ok(row.get("count"))
    }

    /// Atomically increment job progress after processing a file
    ///
    /// # Errors
    ///
    /// Atomically increment files processed count
    ///
    /// Called by parser workers after successfully processing a file.
    ///
    /// # Errors
    ///
    /// Returns error if database update fails.
    pub async fn increment_files_processed(
        &self,
        job_id: &uuid::Uuid,
        delta: i32,
    ) -> DatabaseResult<()> {
        let pool = self.pools.write_pool();
        let correlation_id = None;

        let operation = DatabaseOperation::Query {
            description: "increment_files_processed".to_string(),
        };

        sqlx::query(
            r"
            UPDATE indexing_jobs
            SET files_processed = files_processed + $2
            WHERE job_id = $1
            ",
        )
        .bind(job_id)
        .bind(delta)
        .execute(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        Ok(())
    }

    /// Atomically increment chunks created count
    ///
    /// Called by embedder workers after successfully storing chunks.
    ///
    /// # Errors
    ///
    /// Returns error if database update fails.
    pub async fn increment_chunks_created(
        &self,
        job_id: &uuid::Uuid,
        delta: i32,
    ) -> DatabaseResult<()> {
        let pool = self.pools.write_pool();
        let correlation_id = None;

        let operation = DatabaseOperation::Query {
            description: "increment_chunks_created".to_string(),
        };

        sqlx::query(
            r"
            UPDATE indexing_jobs
            SET chunks_created = chunks_created + $2
            WHERE job_id = $1
            ",
        )
        .bind(job_id)
        .bind(delta)
        .execute(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        Ok(())
    }

    /// Mark a file as completed in the queue after successful processing
    ///
    /// # Errors
    ///
    /// Returns error if database update fails
    pub async fn mark_file_completed(
        &self,
        job_id: &uuid::Uuid,
        file_path: &str,
    ) -> DatabaseResult<()> {
        let pool = self.pools.write_pool();
        let correlation_id = None;

        let operation = DatabaseOperation::Query {
            description: "mark_file_completed".to_string(),
        };

        sqlx::query(
            r"
            UPDATE indexing_job_file_queue
            SET status = 'completed', completed_at = NOW()
            WHERE job_id = $1 AND file_path = $2 AND status = 'processing'
            ",
        )
        .bind(job_id)
        .bind(file_path)
        .execute(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        Ok(())
    }

    /// Check if job is complete (no more queued or processing files)
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn check_job_complete(&self, job_id: &uuid::Uuid) -> DatabaseResult<bool> {
        let depth = self.get_queue_depth(job_id).await?;
        Ok(depth == 0)
    }

    /// Get indexing job by ID
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn get_indexing_job(&self, job_id: &Uuid) -> DatabaseResult<Option<IndexingJob>> {
        let pool = self.pools.read_pool();
        let correlation_id = None;

        let operation = DatabaseOperation::GetFileMetadata {
            repository_id: "unknown".to_string(),
            branch: "unknown".to_string(),
            file_path: format!("job:{job_id}"),
        };

        let row = sqlx::query(
            r"
            SELECT job_id, tenant_id, repository_id, branch, status, files_total, files_processed,
                   chunks_created, repository_url, commit_sha, commit_message, commit_date, author,
                   vector_namespace, correlation_id, started_at, completed_at, error_message
            FROM indexing_jobs
            WHERE job_id = $1
            ",
        )
        .bind(job_id)
        .fetch_optional(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        Ok(row.map(|r| {
            let status_str: String = r.get("status");
            IndexingJob {
                job_id: r.get("job_id"),
                tenant_id: r.get("tenant_id"),
                repository_id: r.get("repository_id"),
                branch: r.get("branch"),
                status: status_str.parse().unwrap_or(JobStatus::Failed),
                files_total: r.get("files_total"),
                files_processed: r.get("files_processed"),
                chunks_created: r.get("chunks_created"),
                repository_url: r.get("repository_url"),
                commit_sha: r.get("commit_sha"),
                commit_message: r.get("commit_message"),
                commit_date: r.get("commit_date"),
                author: r.get("author"),
                vector_namespace: r.get("vector_namespace"),
                correlation_id: r.get("correlation_id"),
                started_at: r.get("started_at"),
                completed_at: r.get("completed_at"),
                error_message: r.get("error_message"),
            }
        }))
    }

    /// List indexing jobs, optionally filtered by repository
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn list_indexing_jobs(
        &self,
        tenant_id: Option<&Uuid>,
        repository_id: Option<&str>,
    ) -> DatabaseResult<Vec<IndexingJob>> {
        let pool = self.pools.read_pool();
        let correlation_id = None;

        let operation = DatabaseOperation::GetFileMetadata {
            repository_id: repository_id.unwrap_or("all").to_string(),
            branch: "unknown".to_string(),
            file_path: "jobs".to_string(),
        };

        let rows = match (tenant_id, repository_id) {
            (Some(tid), Some(repo)) => sqlx::query(
                r"
                    SELECT job_id, tenant_id, repository_id, branch, status, files_total, files_processed,
                           chunks_created, repository_url, commit_sha, commit_message, commit_date, author,
                           vector_namespace, correlation_id, started_at, completed_at, error_message
                    FROM indexing_jobs
                    WHERE tenant_id = $1 AND repository_id = $2
                    ORDER BY started_at DESC
                    LIMIT 100
                    ",
            )
            .bind(tid)
            .bind(repo)
            .fetch_all(pool)
            .await
            .map_db_err(operation, correlation_id)?,
            (Some(tid), None) => sqlx::query(
                r"
                    SELECT job_id, tenant_id, repository_id, branch, status, files_total, files_processed,
                           chunks_created, repository_url, commit_sha, commit_message, commit_date, author,
                           vector_namespace, correlation_id, started_at, completed_at, error_message
                    FROM indexing_jobs
                    WHERE tenant_id = $1
                    ORDER BY started_at DESC
                    LIMIT 100
                    ",
            )
            .bind(tid)
            .fetch_all(pool)
            .await
            .map_db_err(operation, correlation_id)?,
            (None, _) => sqlx::query(
                r"
                    SELECT job_id, tenant_id, repository_id, branch, status, files_total, files_processed,
                           chunks_created, repository_url, commit_sha, commit_message, commit_date, author,
                           vector_namespace, correlation_id, started_at, completed_at, error_message
                    FROM indexing_jobs
                    ORDER BY started_at DESC
                    LIMIT 100
                    ",
            )
            .fetch_all(pool)
            .await
            .map_db_err(operation, correlation_id)?,
        };

        Ok(rows
            .into_iter()
            .map(|r| {
                let status_str: String = r.get("status");
                IndexingJob {
                    job_id: r.get("job_id"),
                    tenant_id: r.get("tenant_id"),
                    repository_id: r.get("repository_id"),
                    branch: r.get("branch"),
                    status: status_str.parse().unwrap_or(JobStatus::Failed),
                    files_total: r.get("files_total"),
                    files_processed: r.get("files_processed"),
                    chunks_created: r.get("chunks_created"),
                    repository_url: r.get("repository_url"),
                    commit_sha: r.get("commit_sha"),
                    commit_message: r.get("commit_message"),
                    commit_date: r.get("commit_date"),
                    author: r.get("author"),
                    vector_namespace: r.get("vector_namespace"),
                    correlation_id: r.get("correlation_id"),
                    started_at: r.get("started_at"),
                    completed_at: r.get("completed_at"),
                    error_message: r.get("error_message"),
                }
            })
            .collect())
    }
}

#[async_trait]
impl FileRepository for DbFileRepository {
    async fn create_tenant(&self, name: &str) -> DatabaseResult<Uuid> {
        let pool = self.pools.write_pool();
        let correlation_id = None; // TODO: Wire through from upper layers

        let operation = DatabaseOperation::Query {
            description: "create_tenant".to_string(),
        };

        let tenant_id: Uuid =
            sqlx::query_scalar("INSERT INTO tenants (name) VALUES ($1) RETURNING tenant_id")
                .bind(name)
                .fetch_one(pool)
                .await
                .map_db_err(operation, correlation_id)?;

        Ok(tenant_id)
    }

    async fn ensure_project_branch(
        &self,
        ctx: &RepositoryContext,
    ) -> DatabaseResult<ProjectBranch> {
        // Use write pool for INSERT/UPDATE operations
        let pool = self.pools.write_pool();
        let correlation_id = None; // Will be passed from upper layers in future

        let operation = DatabaseOperation::EnsureProjectBranch {
            repository_id: ctx.repository_id.clone(),
            branch: ctx.branch.clone(),
        };

        let row = sqlx::query(
            r"
            INSERT INTO project_branches (tenant_id, repository_id, branch, repository_url)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (tenant_id, repository_id, branch)
            DO UPDATE SET repository_url = EXCLUDED.repository_url
            RETURNING
                tenant_id,
                repository_id,
                branch,
                repository_url,
                first_seen,
                last_indexed
            ",
        )
        .bind(ctx.tenant_id)
        .bind(&ctx.repository_id)
        .bind(&ctx.branch)
        .bind(&ctx.repository_url)
        .fetch_one(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        Ok(ProjectBranch {
            tenant_id: row.get("tenant_id"),
            repository_id: row.get("repository_id"),
            branch: row.get("branch"),
            repository_url: row.get("repository_url"),
            first_seen: row.get("first_seen"),
            last_indexed: row.get("last_indexed"),
        })
    }

    #[tracing::instrument(skip(self), fields(elapsed_ms))]
    async fn check_file_state(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        content_hash: &str,
    ) -> DatabaseResult<FileState> {
        let start = std::time::Instant::now();

        // Use read pool for SELECT operations
        let pool = self.pools.read_pool();
        let correlation_id = None; // Will be passed from upper layers in future

        let operation = DatabaseOperation::CheckFileState {
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
            file_path: file_path.to_string(),
        };

        let existing = sqlx::query(
            r"
            SELECT content_hash, generation
            FROM indexed_files
            WHERE tenant_id = $1 AND repository_id = $2 AND branch = $3 AND file_path = $4
            ",
        )
        .bind(tenant_id)
        .bind(repository_id)
        .bind(branch)
        .bind(file_path)
        .fetch_optional(pool)
        .await
        .map_db_err(operation.clone(), correlation_id)?;

        let result = match existing {
            None => {
                // New file, start at generation 1
                Ok(FileState::New { generation: 1 })
            }
            Some(row) => {
                let existing_hash: String = row.get("content_hash");
                if existing_hash == content_hash {
                    // Content unchanged
                    Ok(FileState::Unchanged)
                } else {
                    // Content changed, increment generation
                    let generation: i64 = row.get("generation");
                    // Use checked_add for generation tracking - overflow indicates data corruption
                    let new_generation = generation.checked_add(1).ok_or_else(|| {
                        DatabaseError::DataIntegrityError {
                            operation: Box::new(operation.clone()),
                            message: "Generation counter overflow - indicates data corruption"
                                .to_string(),
                            correlation_id: None,
                        }
                    })?;
                    Ok(FileState::Updated {
                        old_generation: generation,
                        new_generation,
                    })
                }
            }
        };

        #[allow(clippy::cast_possible_truncation)]
        let elapsed = start.elapsed().as_millis() as u64;
        tracing::Span::current().record("elapsed_ms", elapsed);

        result
    }

    #[tracing::instrument(skip(self, metadata), fields(elapsed_ms))]
    async fn record_file_indexing(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        metadata: &FileMetadata,
    ) -> DatabaseResult<IndexedFile> {
        let start = std::time::Instant::now();

        // Use write pool for INSERT operations
        let pool = self.pools.write_pool();
        let correlation_id = None; // Will be passed from upper layers in future

        let operation = DatabaseOperation::RecordFileIndexing {
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
            file_path: metadata.path.clone(),
        };

        let row = sqlx::query(
            r"
            INSERT INTO indexed_files (
                tenant_id, repository_id, branch, file_path, file_content, content_hash, encoding, size_bytes, generation,
                commit_sha, commit_message, commit_date, author, indexed_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NOW())
            ON CONFLICT (tenant_id, repository_id, branch, file_path)
            DO UPDATE SET
                file_content = EXCLUDED.file_content,
                content_hash = EXCLUDED.content_hash,
                encoding = EXCLUDED.encoding,
                size_bytes = EXCLUDED.size_bytes,
                generation = EXCLUDED.generation,
                commit_sha = EXCLUDED.commit_sha,
                commit_message = EXCLUDED.commit_message,
                commit_date = EXCLUDED.commit_date,
                author = EXCLUDED.author,
                indexed_at = NOW()
            RETURNING *
            ",
        )
        .bind(tenant_id)
        .bind(repository_id)
        .bind(branch)
        .bind(&metadata.path)
        .bind(&metadata.content)
        .bind(&metadata.content_hash)
        .bind(&metadata.encoding)
        .bind(metadata.size_bytes)
        .bind(metadata.generation)
        .bind(&metadata.commit_sha)
        .bind(&metadata.commit_message)
        .bind(metadata.commit_date)
        .bind(&metadata.author)
        .fetch_one(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        let result = Ok(IndexedFile {
            tenant_id: row.get("tenant_id"),
            repository_id: row.get("repository_id"),
            branch: row.get("branch"),
            file_path: row.get("file_path"),
            file_content: row.get("file_content"),
            content_hash: row.get("content_hash"),
            encoding: row.get("encoding"),
            size_bytes: row.get("size_bytes"),
            generation: row.get("generation"),
            commit_sha: row.get("commit_sha"),
            commit_message: row.get("commit_message"),
            commit_date: row.get("commit_date"),
            author: row.get("author"),
            indexed_at: row.get("indexed_at"),
        });

        #[allow(clippy::cast_possible_truncation)]
        let elapsed = start.elapsed().as_millis() as u64;
        tracing::Span::current().record("elapsed_ms", elapsed);

        result
    }

    #[tracing::instrument(skip(self, chunks), fields(chunk_count = chunks.len(), elapsed_ms))]
    async fn insert_chunks(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        chunks: Vec<ChunkMetadata>,
    ) -> DatabaseResult<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        let start = std::time::Instant::now();

        let pool = self.pools.write_pool();
        let correlation_id = None; // Will be passed from upper layers in future
        let chunk_count = chunks.len();

        let operation = DatabaseOperation::InsertChunks {
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
            chunk_count,
        };

        // Use UNNEST for bulk insert - drastically faster than loop
        // Pre-allocate with exact capacity to avoid reallocations
        let len = chunks.len();
        let mut chunk_ids = Vec::with_capacity(len);
        let mut file_paths = Vec::with_capacity(len);
        let mut chunk_indices = Vec::with_capacity(len);
        let mut generations = Vec::with_capacity(len);
        let mut start_lines = Vec::with_capacity(len);
        let mut end_lines = Vec::with_capacity(len);
        let mut byte_starts = Vec::with_capacity(len);
        let mut byte_ends = Vec::with_capacity(len);
        let mut kinds = Vec::with_capacity(len);
        let mut names = Vec::with_capacity(len);

        // Single-pass iteration with zero-copy for references where possible
        for chunk in chunks {
            chunk_ids.push(chunk.chunk_id);
            file_paths.push(chunk.file_path.clone()); // Still need to clone, but only once per chunk
            chunk_indices.push(chunk.chunk_index);
            generations.push(chunk.generation);
            start_lines.push(chunk.start_line);
            end_lines.push(chunk.end_line);
            byte_starts.push(chunk.byte_start);
            byte_ends.push(chunk.byte_end);
            kinds.push(chunk.kind.clone()); // Still need to clone Option<String>
            names.push(chunk.name.clone()); // Still need to clone Option<String>
        }

        sqlx::query(
            r"
            INSERT INTO chunk_metadata (
                chunk_id, tenant_id, repository_id, branch, file_path, chunk_index, generation,
                start_line, end_line, byte_start, byte_end, kind, name, created_at
            )
            SELECT
                unnest($1::uuid[]),
                $2,
                $3,
                $4,
                unnest($5::text[]),
                unnest($6::int[]),
                unnest($7::bigint[]),
                unnest($8::int[]),
                unnest($9::int[]),
                unnest($10::bigint[]),
                unnest($11::bigint[]),
                unnest($12::text[]),
                unnest($13::text[]),
                NOW()
            ON CONFLICT (chunk_id) DO NOTHING
            ",
        )
        .bind(&chunk_ids)
        .bind(tenant_id)
        .bind(repository_id)
        .bind(branch)
        .bind(&file_paths)
        .bind(&chunk_indices)
        .bind(&generations)
        .bind(&start_lines)
        .bind(&end_lines)
        .bind(&byte_starts)
        .bind(&byte_ends)
        .bind(&kinds)
        .bind(&names)
        .execute(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        let result = Ok(());

        #[allow(clippy::cast_possible_truncation)]
        let elapsed = start.elapsed().as_millis() as u64;
        tracing::Span::current().record("elapsed_ms", elapsed);

        result
    }

    async fn replace_file_chunks(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        new_generation: i64,
    ) -> DatabaseResult<Vec<Uuid>> {
        // Use analytics pool for operations that might affect many rows
        let pool = self.pools.analytics_pool();
        let correlation_id = None; // Will be passed from upper layers in future

        let operation = DatabaseOperation::ReplaceFileChunks {
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
            file_path: file_path.to_string(),
            new_generation,
        };

        let rows = sqlx::query("SELECT * FROM replace_file_chunks($1, $2, $3, $4, $5)")
            .bind(tenant_id)
            .bind(repository_id)
            .bind(branch)
            .bind(file_path)
            .bind(new_generation)
            .fetch_all(pool)
            .await
            .map_db_err(operation, correlation_id)?;

        let deleted_ids = rows
            .into_iter()
            .map(|row| row.get::<Uuid, _>("deleted_chunk_id"))
            .collect();

        Ok(deleted_ids)
    }

    async fn create_indexing_job(
        &self,
        vector_namespace: &str,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        commit_context: &CommitContext,
        correlation_id: Uuid,
    ) -> DatabaseResult<IndexingJob> {
        // Use write pool for INSERT
        let pool = self.pools.write_pool();
        let job_id = Uuid::new_v4();

        let operation = DatabaseOperation::CreateIndexingJob {
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
        };

        let row = sqlx::query(
            r"
            INSERT INTO indexing_jobs (
                job_id, tenant_id, repository_id, branch, status,
                files_processed, chunks_created,
                repository_url, commit_sha, commit_message, commit_date, author,
                vector_namespace,
                correlation_id,
                started_at
            )
            VALUES ($1, $2, $3, $4, $5, 0, 0, $6, $7, $8, $9, $10, $11, $12, NOW())
            RETURNING *
            ",
        )
        .bind(job_id)
        .bind(tenant_id)
        .bind(repository_id)
        .bind(branch)
        .bind(JobStatus::Running.to_string())
        .bind(&commit_context.repository_url)
        .bind(&commit_context.commit_sha)
        .bind(&commit_context.commit_message)
        .bind(commit_context.commit_date)
        .bind(&commit_context.author)
        .bind(vector_namespace)
        .bind(correlation_id)
        .fetch_one(pool)
        .await
        .map_db_err(operation, Some(correlation_id.to_string()))?;

        Ok(IndexingJob {
            job_id: row.get("job_id"),
            tenant_id: row.get("tenant_id"),
            repository_id: row.get("repository_id"),
            branch: row.get("branch"),
            status: JobStatus::from(row.get::<String, _>("status")),
            files_total: row.get("files_total"),
            files_processed: row.get("files_processed"),
            chunks_created: row.get("chunks_created"),
            repository_url: row.get("repository_url"),
            commit_sha: row.get("commit_sha"),
            commit_message: row.get("commit_message"),
            commit_date: row.get("commit_date"),
            author: row.get("author"),
            vector_namespace: row.get("vector_namespace"),
            correlation_id: row.get("correlation_id"),
            started_at: row.get("started_at"),
            completed_at: row.get("completed_at"),
            error_message: row.get("error_message"),
        })
    }

    async fn update_job_progress(
        &self,
        job_id: &Uuid,
        files_processed: i32,
        chunks_created: i32,
    ) -> DatabaseResult<()> {
        // Use write pool for UPDATE
        let pool = self.pools.write_pool();
        let correlation_id = None; // Will be passed from upper layers in future

        let operation = DatabaseOperation::UpdateJobProgress { job_id: *job_id };

        sqlx::query(
            r"
            UPDATE indexing_jobs
            SET files_processed = files_processed + $2,
                chunks_created = chunks_created + $3
            WHERE job_id = $1
            ",
        )
        .bind(job_id)
        .bind(files_processed)
        .bind(chunks_created)
        .execute(pool)
        .await
        .map_db_err(operation.clone(), correlation_id)?;

        Ok(())
    }

    async fn complete_job(
        &self,
        job_id: &Uuid,
        status: JobStatus,
        error: Option<String>,
    ) -> DatabaseResult<()> {
        // Use write pool for UPDATE operations
        let pool = self.pools.write_pool();
        let correlation_id = None; // Will be passed from upper layers in future

        let operation = DatabaseOperation::CompleteJob { job_id: *job_id };

        sqlx::query(
            r"
            UPDATE indexing_jobs
            SET status = $2,
                completed_at = NOW(),
                error_message = $3
            WHERE job_id = $1
            ",
        )
        .bind(job_id)
        .bind(status.to_string())
        .bind(error)
        .execute(pool)
        .await
        .map_db_err(operation.clone(), correlation_id.clone())?;

        // Update project last_indexed timestamp
        if status == JobStatus::Completed {
            sqlx::query(
                r"
                UPDATE project_branches
                SET last_indexed = NOW()
                WHERE repository_id = (
                    SELECT repository_id FROM indexing_jobs WHERE job_id = $1
                )
                AND branch = (
                    SELECT branch FROM indexing_jobs WHERE job_id = $1
                )
                ",
            )
            .bind(job_id)
            .execute(pool)
            .await
            .map_db_err(operation, correlation_id.clone())?;
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
        // Use read pool for SELECT operations
        let pool = self.pools.read_pool();
        let correlation_id = None; // Will be passed from upper layers in future

        let operation = DatabaseOperation::GetFileChunks {
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
            file_path: file_path.to_string(),
        };

        let rows = sqlx::query(
            r"
            SELECT
                chunk_id, tenant_id, repository_id, branch, file_path,
                chunk_index, generation, start_line, end_line,
                byte_start, byte_end, kind, name, created_at
            FROM chunk_metadata
            WHERE tenant_id = $1 AND repository_id = $2 AND branch = $3 AND file_path = $4
            ORDER BY chunk_index
            ",
        )
        .bind(tenant_id)
        .bind(repository_id)
        .bind(branch)
        .bind(file_path)
        .fetch_all(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        let chunks = rows
            .into_iter()
            .map(|row| ChunkMetadata {
                chunk_id: row.get("chunk_id"),
                tenant_id: row.get("tenant_id"),
                repository_id: row.get("repository_id"),
                branch: row.get("branch"),
                file_path: row.get("file_path"),
                chunk_index: row.get("chunk_index"),
                generation: row.get("generation"),
                start_line: row.get("start_line"),
                end_line: row.get("end_line"),
                byte_start: row.get("byte_start"),
                byte_end: row.get("byte_end"),
                kind: row.get("kind"),
                name: row.get("name"),
                created_at: row.get("created_at"),
            })
            .collect();

        Ok(chunks)
    }

    async fn get_indexed_files(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
    ) -> DatabaseResult<Vec<IndexedFile>> {
        // Use read pool for SELECT operations
        let pool = self.pools.read_pool();
        let correlation_id = None; // Will be passed from upper layers in future

        let operation = DatabaseOperation::GetIndexedFiles {
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
        };

        let rows = sqlx::query(
            r"
            SELECT *
            FROM indexed_files
            WHERE tenant_id = $1 AND repository_id = $2 AND branch = $3
            ORDER BY file_path
            ",
        )
        .bind(tenant_id)
        .bind(repository_id)
        .bind(branch)
        .fetch_all(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        let files = rows
            .into_iter()
            .map(|row| IndexedFile {
                tenant_id: row.get("tenant_id"),
                repository_id: row.get("repository_id"),
                branch: row.get("branch"),
                file_path: row.get("file_path"),
                file_content: row.get("file_content"),
                content_hash: row.get("content_hash"),
                encoding: row.get("encoding"),
                size_bytes: row.get("size_bytes"),
                generation: row.get("generation"),
                commit_sha: row.get("commit_sha"),
                commit_message: row.get("commit_message"),
                commit_date: row.get("commit_date"),
                author: row.get("author"),
                indexed_at: row.get("indexed_at"),
            })
            .collect();

        Ok(files)
    }

    async fn has_running_jobs(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
    ) -> DatabaseResult<bool> {
        // Use read pool for quick SELECT operations
        let pool = self.pools.read_pool();
        let correlation_id = None; // Will be passed from upper layers in future

        let operation = DatabaseOperation::CheckRunningJobs {
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
        };

        let row = sqlx::query(
            r"
            SELECT COUNT(*) as count
            FROM indexing_jobs
            WHERE tenant_id = $1
              AND repository_id = $2
              AND branch = $3
              AND status IN ('pending', 'running')
            ",
        )
        .bind(tenant_id)
        .bind(repository_id)
        .bind(branch)
        .fetch_one(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        let count: i64 = row.get("count");
        Ok(count > 0)
    }

    async fn get_file_metadata(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
    ) -> DatabaseResult<Option<IndexedFile>> {
        // Use read pool for SELECT operations
        let pool = self.pools.read_pool();
        let correlation_id = None; // Will be passed from upper layers in future

        let operation = DatabaseOperation::GetFileMetadata {
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
            file_path: file_path.to_string(),
        };

        let row = sqlx::query(
            r"
            SELECT *
            FROM indexed_files
            WHERE tenant_id = $1 AND repository_id = $2 AND branch = $3 AND file_path = $4
            ",
        )
        .bind(tenant_id)
        .bind(repository_id)
        .bind(branch)
        .bind(file_path)
        .fetch_optional(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        row.map_or(Ok(None), |row| {
            Ok(Some(IndexedFile {
                tenant_id: row.get("tenant_id"),
                repository_id: row.get("repository_id"),
                branch: row.get("branch"),
                file_path: row.get("file_path"),
                file_content: row.get("file_content"),
                content_hash: row.get("content_hash"),
                encoding: row.get("encoding"),
                size_bytes: row.get("size_bytes"),
                generation: row.get("generation"),
                commit_sha: row.get("commit_sha"),
                commit_message: row.get("commit_message"),
                commit_date: row.get("commit_date"),
                author: row.get("author"),
                indexed_at: row.get("indexed_at"),
            }))
        })
    }

    #[tracing::instrument(skip(self), fields(file_count = file_paths.len()))]
    async fn get_files_metadata(
        &self,
        tenant_id: &Uuid,
        file_paths: &[&str],
    ) -> DatabaseResult<Vec<IndexedFile>> {
        if file_paths.is_empty() {
            return Ok(vec![]);
        }

        // Use read pool for SELECT operations
        let pool = self.pools.read_pool();
        let correlation_id = None; // Will be passed from upper layers in future

        let operation = DatabaseOperation::GetFilesMetadata {
            file_count: file_paths.len(),
        };

        // Convert to Vec<String> for sqlx binding
        let file_paths_vec: Vec<String> = file_paths.iter().map(|&s| s.to_string()).collect();

        let rows = sqlx::query(
            r"
            SELECT if.*
            FROM indexed_files if
            WHERE if.tenant_id = $1 AND if.file_path = ANY($2)
            ORDER BY if.file_path
            ",
        )
        .bind(tenant_id)
        .bind(&file_paths_vec)
        .fetch_all(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        let files = rows
            .into_iter()
            .map(|row| IndexedFile {
                tenant_id: row.get("tenant_id"),
                repository_id: row.get("repository_id"),
                branch: row.get("branch"),
                file_path: row.get("file_path"),
                file_content: row.get("file_content"),
                content_hash: row.get("content_hash"),
                encoding: row.get("encoding"),
                size_bytes: row.get("size_bytes"),
                generation: row.get("generation"),
                commit_sha: row.get("commit_sha"),
                commit_message: row.get("commit_message"),
                commit_date: row.get("commit_date"),
                author: row.get("author"),
                indexed_at: row.get("indexed_at"),
            })
            .collect();

        Ok(files)
    }

    async fn get_project_branch(
        &self,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
    ) -> DatabaseResult<Option<ProjectBranch>> {
        // Use read pool for SELECT operations
        let pool = self.pools.read_pool();
        let correlation_id = None; // Will be passed from upper layers in future

        let operation = DatabaseOperation::GetProjectBranch {
            repository_id: repository_id.to_string(),
            branch: branch.to_string(),
        };

        let row = sqlx::query(
            r"
            SELECT *
            FROM project_branches
            WHERE tenant_id = $1 AND repository_id = $2 AND branch = $3
            ",
        )
        .bind(tenant_id)
        .bind(repository_id)
        .bind(branch)
        .fetch_optional(pool)
        .await
        .map_db_err(operation, correlation_id)?;

        row.map_or(Ok(None), |row| {
            Ok(Some(ProjectBranch {
                tenant_id: row.get("tenant_id"),
                repository_id: row.get("repository_id"),
                branch: row.get("branch"),
                repository_url: row.get("repository_url"),
                first_seen: row.get("first_seen"),
                last_indexed: row.get("last_indexed"),
            }))
        })
    }

    #[tracing::instrument(skip(self), fields(repo_branch_count = repo_branches.len()))]
    async fn get_project_branches(
        &self,
        tenant_id: &Uuid,
        repo_branches: &[(String, String)],
    ) -> DatabaseResult<Vec<ProjectBranch>> {
        if repo_branches.is_empty() {
            return Ok(Vec::new());
        }

        let pool = self.pools.read_pool();
        let correlation_id = None; // Will be passed from upper layers in future

        let operation = DatabaseOperation::GetProjectBranches {
            count: repo_branches.len(),
        };

        // Build parameterized query - manually construct to avoid sqlx separator issues
        let mut query_builder = sqlx::QueryBuilder::new(
            "SELECT tenant_id, repository_id, branch, repository_url, first_seen, last_indexed
             FROM project_branches
             WHERE tenant_id = ",
        );
        query_builder.push_bind(tenant_id);
        query_builder.push(" AND (repository_id, branch) IN (");

        // Manually build tuple list with proper separation
        for (idx, (repo_id, branch)) in repo_branches.iter().enumerate() {
            if idx > 0 {
                query_builder.push(", ");
            }
            query_builder.push("(");
            query_builder.push_bind(repo_id);
            query_builder.push(", ");
            query_builder.push_bind(branch);
            query_builder.push(")");
        }
        query_builder.push(")");

        let rows = query_builder
            .build()
            .fetch_all(pool)
            .await
            .map_db_err(operation, correlation_id)?;

        let branches = rows
            .into_iter()
            .map(|row| ProjectBranch {
                tenant_id: row.get("tenant_id"),
                repository_id: row.get("repository_id"),
                branch: row.get("branch"),
                repository_url: row.get("repository_url"),
                first_seen: row.get("first_seen"),
                last_indexed: row.get("last_indexed"),
            })
            .collect();

        Ok(branches)
    }

    async fn enqueue_file(
        &self,
        job_id: &Uuid,
        tenant_id: &Uuid,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        file_content: &str,
        content_hash: &str,
    ) -> DatabaseResult<()> {
        self.enqueue_file(
            job_id,
            tenant_id,
            repository_id,
            branch,
            file_path,
            file_content,
            content_hash,
        )
        .await
    }

    async fn dequeue_file(&self) -> DatabaseResult<Option<DequeuedFile>> {
        self.dequeue_file().await
    }

    async fn get_queue_depth(&self, job_id: &Uuid) -> DatabaseResult<i64> {
        self.get_queue_depth(job_id).await
    }

    async fn increment_files_processed(&self, job_id: &Uuid, delta: i32) -> DatabaseResult<()> {
        self.increment_files_processed(job_id, delta).await
    }

    async fn increment_chunks_created(&self, job_id: &Uuid, delta: i32) -> DatabaseResult<()> {
        self.increment_chunks_created(job_id, delta).await
    }

    async fn mark_file_completed(&self, job_id: &Uuid, file_path: &str) -> DatabaseResult<()> {
        self.mark_file_completed(job_id, file_path).await
    }

    async fn check_job_complete(&self, job_id: &Uuid) -> DatabaseResult<bool> {
        self.check_job_complete(job_id).await
    }

    async fn get_indexing_job(&self, job_id: &Uuid) -> DatabaseResult<Option<IndexingJob>> {
        self.get_indexing_job(job_id).await
    }

    async fn list_indexing_jobs(
        &self,
        tenant_id: Option<&Uuid>,
        repository_id: Option<&str>,
    ) -> DatabaseResult<Vec<IndexingJob>> {
        self.list_indexing_jobs(tenant_id, repository_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test tenant for all unit tests
    const TEST_TENANT: Uuid = Uuid::nil();

    #[tokio::test]
    #[allow(clippy::expect_used)] // Tests can use expect
    async fn test_enqueue_file() {
        let job_id = Uuid::new_v4();
        let mock_repo = crate::mock::MockFileRepository::new();

        let result = mock_repo
            .enqueue_file(
                &job_id,
                &TEST_TENANT,
                "test_repo",
                "main",
                "src/test.rs",
                "fn test() {}",
                "hash123",
            )
            .await;

        assert!(result.is_ok(), "Should enqueue file successfully");
    }

    #[tokio::test]
    #[allow(clippy::expect_used)] // Tests can use expect
    async fn test_dequeue_file_empty_queue() {
        let mock_repo = crate::mock::MockFileRepository::new();

        // Test global queue dequeue (no job_id filter)
        let result = mock_repo.dequeue_file().await.expect("Should not error");

        assert!(result.is_none(), "Empty queue should return None");
    }

    #[tokio::test]
    #[allow(clippy::expect_used)] // Tests can use expect
    async fn test_get_queue_depth() {
        let job_id = Uuid::new_v4();
        let mock_repo = crate::mock::MockFileRepository::new();

        let depth = mock_repo
            .get_queue_depth(&job_id)
            .await
            .expect("Should not error");

        assert_eq!(depth, 0, "Empty queue should have depth 0");
    }
}
