//! Database repository layer with optimized connection pool separation
//!
//! Uses separate connection pools for different operation types to prevent
//! resource contention and improve performance.

use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::Row;
use uuid::Uuid;

use crate::models::{
    ChunkMetadata, FileMetadata, FileState, IndexedFile, IndexingJob, JobStatus, ProjectBranch,
    RepositoryContext,
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
    pub async fn from_env() -> Result<Self> {
        let pools = PoolManager::from_env().await?;
        Ok(Self::new(pools))
    }
}

#[async_trait]
impl FileRepository for DbFileRepository {
    async fn ensure_project_branch(&self, ctx: &RepositoryContext) -> Result<ProjectBranch> {
        // Use write pool for INSERT/UPDATE operations
        let pool = self.pools.write_pool();

        let row = sqlx::query(
            r"
            INSERT INTO project_branches (repository_id, branch, repository_url)
            VALUES ($1, $2, $3)
            ON CONFLICT (repository_id, branch) 
            DO UPDATE SET repository_url = EXCLUDED.repository_url
            RETURNING 
                repository_id, 
                branch, 
                repository_url, 
                first_seen, 
                last_indexed
            ",
        )
        .bind(&ctx.repository_id)
        .bind(&ctx.branch)
        .bind(&ctx.repository_url)
        .fetch_one(pool)
        .await
        .context("Failed to ensure project branch")?;

        Ok(ProjectBranch {
            repository_id: row.get("repository_id"),
            branch: row.get("branch"),
            repository_url: row.get("repository_url"),
            first_seen: row.get("first_seen"),
            last_indexed: row.get("last_indexed"),
        })
    }

    async fn check_file_state(
        &self,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        content_hash: &str,
    ) -> Result<FileState> {
        // Use read pool for SELECT operations
        let pool = self.pools.read_pool();

        let existing = sqlx::query(
            r"
            SELECT content_hash, generation
            FROM indexed_files
            WHERE repository_id = $1 AND branch = $2 AND file_path = $3
            ",
        )
        .bind(repository_id)
        .bind(branch)
        .bind(file_path)
        .fetch_optional(pool)
        .await
        .context("Failed to check file state")?;

        match existing {
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
                    let new_generation = generation
                        .checked_add(1)
                        .context("Generation counter overflow - indicates data corruption")?;
                    Ok(FileState::Updated {
                        old_generation: generation,
                        new_generation,
                    })
                }
            }
        }
    }

    async fn record_file_indexing(
        &self,
        repository_id: &str,
        branch: &str,
        metadata: &FileMetadata,
    ) -> Result<IndexedFile> {
        // Use write pool for INSERT operations
        let pool = self.pools.write_pool();

        let row = sqlx::query(
            r"
            INSERT INTO indexed_files (
                repository_id, branch, file_path, content_hash, generation,
                commit_sha, commit_message, commit_date, author, indexed_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
            ON CONFLICT (repository_id, branch, file_path)
            DO UPDATE SET
                content_hash = EXCLUDED.content_hash,
                generation = EXCLUDED.generation,
                commit_sha = EXCLUDED.commit_sha,
                commit_message = EXCLUDED.commit_message,
                commit_date = EXCLUDED.commit_date,
                author = EXCLUDED.author,
                indexed_at = NOW()
            RETURNING *
            ",
        )
        .bind(repository_id)
        .bind(branch)
        .bind(&metadata.path)
        .bind(&metadata.content_hash)
        .bind(metadata.generation)
        .bind(&metadata.commit_sha)
        .bind(&metadata.commit_message)
        .bind(metadata.commit_date)
        .bind(&metadata.author)
        .fetch_one(pool)
        .await
        .context("Failed to record file indexing")?;

        Ok(IndexedFile {
            repository_id: row.get("repository_id"),
            branch: row.get("branch"),
            file_path: row.get("file_path"),
            content_hash: row.get("content_hash"),
            generation: row.get("generation"),
            commit_sha: row.get("commit_sha"),
            commit_message: row.get("commit_message"),
            commit_date: row.get("commit_date"),
            author: row.get("author"),
            indexed_at: row.get("indexed_at"),
        })
    }

    async fn insert_chunks(
        &self,
        repository_id: &str,
        branch: &str,
        chunks: Vec<ChunkMetadata>,
    ) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        let pool = self.pools.write_pool();

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
                chunk_id, repository_id, branch, file_path, chunk_index, generation,
                start_line, end_line, byte_start, byte_end, kind, name, created_at
            )
            SELECT 
                unnest($1::uuid[]),
                $2,
                $3,
                unnest($4::text[]),
                unnest($5::int[]),
                unnest($6::bigint[]),
                unnest($7::int[]),
                unnest($8::int[]),
                unnest($9::bigint[]),
                unnest($10::bigint[]),
                unnest($11::text[]),
                unnest($12::text[]),
                NOW()
            ON CONFLICT (chunk_id) DO NOTHING
            ",
        )
        .bind(&chunk_ids)
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
        .context("Failed to batch insert chunks")?;

        Ok(())
    }

    async fn replace_file_chunks(
        &self,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        new_generation: i64,
    ) -> Result<Vec<Uuid>> {
        // Use analytics pool for operations that might affect many rows
        let pool = self.pools.analytics_pool();

        let rows = sqlx::query("SELECT * FROM replace_file_chunks($1, $2, $3, $4)")
            .bind(repository_id)
            .bind(branch)
            .bind(file_path)
            .bind(new_generation)
            .fetch_all(pool)
            .await
            .context("Failed to replace file chunks")?;

        let deleted_ids = rows
            .into_iter()
            .map(|row| row.get::<Uuid, _>("deleted_chunk_id"))
            .collect();

        Ok(deleted_ids)
    }

    async fn create_indexing_job(
        &self,
        repository_id: &str,
        branch: &str,
        commit_sha: Option<&str>,
    ) -> Result<IndexingJob> {
        // Use write pool for INSERT
        let pool = self.pools.write_pool();
        let job_id = Uuid::new_v4();

        let row = sqlx::query(
            r"
            INSERT INTO indexing_jobs (
                job_id, repository_id, branch, status, 
                files_processed, chunks_created, commit_sha, started_at
            )
            VALUES ($1, $2, $3, $4, 0, 0, $5, NOW())
            RETURNING *
            ",
        )
        .bind(job_id)
        .bind(repository_id)
        .bind(branch)
        .bind(JobStatus::Running.to_string())
        .bind(commit_sha)
        .fetch_one(pool)
        .await
        .context("Failed to create indexing job")?;

        Ok(IndexingJob {
            job_id: row.get("job_id"),
            repository_id: row.get("repository_id"),
            branch: row.get("branch"),
            status: JobStatus::from(row.get::<String, _>("status")),
            files_total: row.get("files_total"),
            files_processed: row.get("files_processed"),
            chunks_created: row.get("chunks_created"),
            commit_sha: row.get("commit_sha"),
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
    ) -> Result<()> {
        // Use write pool for UPDATE
        let pool = self.pools.write_pool();

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
        .context("Failed to update job progress")?;

        Ok(())
    }

    async fn complete_job(
        &self,
        job_id: &Uuid,
        status: JobStatus,
        error: Option<String>,
    ) -> Result<()> {
        // Use write pool for UPDATE operations
        let pool = self.pools.write_pool();

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
        .context("Failed to complete job")?;

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
            .context("Failed to update last_indexed")?;
        }

        Ok(())
    }

    async fn get_file_chunks(
        &self,
        repository_id: &str,
        branch: &str,
        file_path: &str,
    ) -> Result<Vec<ChunkMetadata>> {
        // Use read pool for SELECT operations
        let pool = self.pools.read_pool();

        let rows = sqlx::query(
            r"
            SELECT 
                chunk_id, repository_id, branch, file_path,
                chunk_index, generation, start_line, end_line,
                byte_start, byte_end, kind, name, created_at
            FROM chunk_metadata
            WHERE repository_id = $1 AND branch = $2 AND file_path = $3
            ORDER BY chunk_index
            ",
        )
        .bind(repository_id)
        .bind(branch)
        .bind(file_path)
        .fetch_all(pool)
        .await
        .context("Failed to get file chunks")?;

        let chunks = rows
            .into_iter()
            .map(|row| ChunkMetadata {
                chunk_id: row.get("chunk_id"),
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
        repository_id: &str,
        branch: &str,
    ) -> Result<Vec<IndexedFile>> {
        // Use read pool for SELECT operations
        let pool = self.pools.read_pool();

        let rows = sqlx::query(
            r"
            SELECT *
            FROM indexed_files
            WHERE repository_id = $1 AND branch = $2
            ORDER BY file_path
            ",
        )
        .bind(repository_id)
        .bind(branch)
        .fetch_all(pool)
        .await
        .context("Failed to get indexed files")?;

        let files = rows
            .into_iter()
            .map(|row| IndexedFile {
                repository_id: row.get("repository_id"),
                branch: row.get("branch"),
                file_path: row.get("file_path"),
                content_hash: row.get("content_hash"),
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

    async fn has_running_jobs(&self, repository_id: &str, branch: &str) -> Result<bool> {
        // Use read pool for quick SELECT operations
        let pool = self.pools.read_pool();

        let row = sqlx::query(
            r"
            SELECT COUNT(*) as count
            FROM indexing_jobs
            WHERE repository_id = $1 
              AND branch = $2
              AND status IN ('pending', 'running')
            ",
        )
        .bind(repository_id)
        .bind(branch)
        .fetch_one(pool)
        .await
        .context("Failed to check running jobs")?;

        let count: i64 = row.get("count");
        Ok(count > 0)
    }
}
