//! Database repository layer with generation-based versioning

use anyhow::{Context, Result};
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::models::{
    ChunkMetadata, FileMetadata, FileState, IndexedFile, IndexingJob, JobStatus, ProjectBranch,
    RepositoryContext,
};
use crate::traits::FileRepository;

/// Repository for database operations
pub struct DbFileRepository {
    pool: PgPool,
}

impl DbFileRepository {
    /// Create new repository instance
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FileRepository for DbFileRepository {
    async fn ensure_project_branch(&self, ctx: &RepositoryContext) -> Result<ProjectBranch> {
        let row = sqlx::query(
            r#"
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
            "#,
        )
        .bind(&ctx.repository_id)
        .bind(&ctx.branch)
        .bind(&ctx.repository_url)
        .fetch_one(&self.pool)
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
        let existing = sqlx::query(
            r#"
            SELECT content_hash, generation
            FROM indexed_files
            WHERE repository_id = $1 AND branch = $2 AND file_path = $3
            "#,
        )
        .bind(repository_id)
        .bind(branch)
        .bind(file_path)
        .fetch_optional(&self.pool)
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
                    Ok(FileState::Updated {
                        old_generation: generation,
                        new_generation: generation + 1,
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
        let row = sqlx::query(
            r#"
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
            "#,
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
        .fetch_one(&self.pool)
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

        // Use transaction for batch insert
        let mut tx = self
            .pool
            .begin()
            .await
            .context("Failed to begin transaction")?;

        for chunk in chunks {
            sqlx::query(
                r#"
                INSERT INTO chunk_metadata (
                    chunk_id, repository_id, branch, file_path, chunk_index, generation,
                    start_line, end_line, kind, name, created_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                "#,
            )
            .bind(chunk.chunk_id)
            .bind(repository_id)
            .bind(branch)
            .bind(&chunk.file_path)
            .bind(chunk.chunk_index)
            .bind(chunk.generation)
            .bind(chunk.start_line)
            .bind(chunk.end_line)
            .bind(&chunk.kind)
            .bind(&chunk.name)
            .bind(chunk.created_at)
            .execute(&mut *tx)
            .await
            .context("Failed to insert chunk")?;
        }

        tx.commit().await.context("Failed to commit chunks")?;

        Ok(())
    }

    async fn replace_file_chunks(
        &self,
        repository_id: &str,
        branch: &str,
        file_path: &str,
        new_generation: i64,
    ) -> Result<Vec<Uuid>> {
        let rows = sqlx::query("SELECT * FROM replace_file_chunks($1, $2, $3, $4)")
            .bind(repository_id)
            .bind(branch)
            .bind(file_path)
            .bind(new_generation)
            .fetch_all(&self.pool)
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
        let job_id = Uuid::new_v4();

        let row = sqlx::query(
            r#"
            INSERT INTO indexing_jobs (
                job_id, repository_id, branch, status, 
                files_processed, chunks_created, commit_sha, started_at
            )
            VALUES ($1, $2, $3, $4, 0, 0, $5, NOW())
            RETURNING *
            "#,
        )
        .bind(job_id)
        .bind(repository_id)
        .bind(branch)
        .bind(JobStatus::Running.to_string())
        .bind(commit_sha)
        .fetch_one(&self.pool)
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
        sqlx::query(
            r#"
            UPDATE indexing_jobs
            SET files_processed = files_processed + $2,
                chunks_created = chunks_created + $3
            WHERE job_id = $1
            "#,
        )
        .bind(job_id)
        .bind(files_processed)
        .bind(chunks_created)
        .execute(&self.pool)
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
        sqlx::query(
            r#"
            UPDATE indexing_jobs
            SET status = $2,
                completed_at = NOW(),
                error_message = $3
            WHERE job_id = $1
            "#,
        )
        .bind(job_id)
        .bind(status.to_string())
        .bind(error)
        .execute(&self.pool)
        .await
        .context("Failed to complete job")?;

        // Update project last_indexed timestamp
        if status == JobStatus::Completed {
            sqlx::query(
                r#"
                UPDATE project_branches
                SET last_indexed = NOW()
                WHERE repository_id = (
                    SELECT repository_id FROM indexing_jobs WHERE job_id = $1
                )
                AND branch = (
                    SELECT branch FROM indexing_jobs WHERE job_id = $1
                )
                "#,
            )
            .bind(job_id)
            .execute(&self.pool)
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
        let rows = sqlx::query(
            r#"
            SELECT 
                chunk_id, repository_id, branch, file_path,
                chunk_index, generation, start_line, end_line,
                kind, name, created_at
            FROM chunk_metadata
            WHERE repository_id = $1 AND branch = $2 AND file_path = $3
            ORDER BY chunk_index
            "#,
        )
        .bind(repository_id)
        .bind(branch)
        .bind(file_path)
        .fetch_all(&self.pool)
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
        let rows = sqlx::query(
            r#"
            SELECT *
            FROM indexed_files
            WHERE repository_id = $1 AND branch = $2
            ORDER BY file_path
            "#,
        )
        .bind(repository_id)
        .bind(branch)
        .fetch_all(&self.pool)
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
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count
            FROM indexing_jobs
            WHERE repository_id = $1 
              AND branch = $2
              AND status IN ('pending', 'running')
            "#,
        )
        .bind(repository_id)
        .bind(branch)
        .fetch_one(&self.pool)
        .await
        .context("Failed to check running jobs")?;

        let count: i64 = row.get("count");
        Ok(count > 0)
    }
}
