//! Background worker for processing indexing jobs
//!
//! This module provides a background worker that processes indexing jobs from the
//! persistent PostgreSQL queue. It's designed to be easily extractable into a
//! separate daemon binary when needed for production deployments.
//!
//! # Architecture
//!
//! The worker continuously polls the database for queued jobs and processes them
//! using the existing parser and embedding worker infrastructure. Jobs are processed
//! one at a time per worker instance, with concurrent file processing within each job.
//!
//! # Future Extraction
//!
//! This module is designed with zero web framework dependencies, making it easy to
//! extract into a standalone `codetriever-worker` binary:
//!
//! ```rust,ignore
//! // Future bins/codetriever-worker/main.rs
//! use codetriever_indexing::worker::BackgroundWorker;
//!
//! #[tokio::main]
//! async fn main() {
//!     let worker = BackgroundWorker::new(...);
//!     worker.run().await;  // Same code, different binary!
//! }
//! ```

use crate::{IndexerError, IndexerResult};
use codetriever_config::ApplicationConfig;
use codetriever_embeddings::EmbeddingService;
use codetriever_meta_data::models::JobStatus;
use codetriever_meta_data::traits::FileRepository;
use codetriever_parsing::CodeParser;
use codetriever_vector_data::VectorStorage;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};

/// Result of encoding detection and conversion
struct EncodingResult {
    content: String,
    encoding_name: String,
}

/// Detect file encoding and convert to UTF-8
///
/// Returns None for binary files that can't be represented as text
fn detect_and_convert_to_utf8(content: &str) -> Option<EncodingResult> {
    let bytes = content.as_bytes();

    // Check for NULL bytes (binary files)
    if bytes.contains(&0) {
        tracing::debug!("File contains NULL bytes - skipping as binary");
        return None;
    }

    // Fast path: Check if already valid UTF-8
    if std::str::from_utf8(bytes).is_ok() {
        return Some(EncodingResult {
            content: content.to_string(),
            encoding_name: "UTF-8".to_string(),
        });
    }

    // Try auto-detection with encoding_rs
    let (encoding, _bom_length) =
        encoding_rs::Encoding::for_bom(bytes).unwrap_or((encoding_rs::UTF_8, 0));

    let (decoded, actual_encoding, malformed) = encoding.decode(bytes);

    // If decoding had errors, it's likely binary - skip it
    if malformed {
        tracing::debug!(
            "File appears to be binary (encoding errors detected with {})",
            actual_encoding.name()
        );
        return None;
    }

    // Successfully decoded
    Some(EncodingResult {
        content: decoded.into_owned(),
        encoding_name: actual_encoding.name().to_string(),
    })
}

/// Configuration for background worker
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// How often to poll for new jobs (milliseconds)
    pub poll_interval_ms: u64,
    /// Number of concurrent parser workers per job
    pub parser_concurrency: usize,
    /// Number of concurrent embedding workers per job
    pub embedder_concurrency: usize,
    /// Batch size for embeddings
    pub embedding_batch_size: usize,
    /// Chunk queue capacity
    pub chunk_queue_capacity: usize,
}

impl WorkerConfig {
    /// Create worker config from application config
    pub fn from_app_config(config: &ApplicationConfig) -> Self {
        Self {
            poll_interval_ms: 1000, // Poll every 1 second
            parser_concurrency: config.indexing.concurrency_limit,
            embedder_concurrency: config.embedding.performance.pool_size,
            embedding_batch_size: config.embedding.performance.indexer_batch_size,
            chunk_queue_capacity: config.indexing.chunk_queue_capacity,
        }
    }
}

/// Background worker for processing indexing jobs
///
/// This worker continuously polls the PostgreSQL queue for jobs and processes them.
/// Designed to run as a background thread in the API binary, with zero web framework
/// dependencies for easy extraction to a separate daemon later.
pub struct BackgroundWorker {
    repository: Arc<dyn FileRepository>,
    embedding_service: Arc<dyn EmbeddingService>,
    vector_storage: Arc<dyn VectorStorage>,
    code_parser: Arc<CodeParser>,
    config: WorkerConfig,
    shutdown_signal: Arc<AtomicBool>,
}

impl BackgroundWorker {
    /// Create a new background worker
    pub fn new(
        repository: Arc<dyn FileRepository>,
        embedding_service: Arc<dyn EmbeddingService>,
        vector_storage: Arc<dyn VectorStorage>,
        code_parser: Arc<CodeParser>,
        config: WorkerConfig,
    ) -> Self {
        Self {
            repository,
            embedding_service,
            vector_storage,
            code_parser,
            config,
            shutdown_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get a handle for graceful shutdown
    pub fn shutdown_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown_signal)
    }

    /// Main worker loop - continuously processes files from global queue
    ///
    /// This method runs indefinitely until shutdown is signaled. It pulls files
    /// from the global FIFO queue (across ALL jobs) and processes them, providing
    /// maximum concurrency and fair scheduling.
    ///
    /// # Graceful Shutdown
    ///
    /// Set the shutdown signal to trigger graceful shutdown. The worker will
    /// finish its current file before exiting.
    pub async fn run(&self) {
        info!("🚀 Background indexing worker started (file-level FIFO)");

        loop {
            // Check for shutdown signal
            if self.shutdown_signal.load(Ordering::Relaxed) {
                info!("📛 Shutdown signal received, worker stopping gracefully");
                break;
            }

            // Dequeue next file from global queue (industry-standard pattern!)
            match self.repository.dequeue_file().await {
                Ok(Some(dequeued)) => {
                    info!(
                        job_id = %dequeued.job_id,
                        file = %dequeued.file_path,
                        "📥 Processing file from global queue"
                    );

                    match self.process_file(&dequeued).await {
                        Ok(chunks_created) => {
                            info!(
                                job_id = %dequeued.job_id,
                                file = %dequeued.file_path,
                                chunks = chunks_created,
                                "✅ File processed successfully"
                            );

                            // Mark file as completed in queue
                            if let Err(e) = self
                                .repository
                                .mark_file_completed(&dequeued.job_id, &dequeued.file_path)
                                .await
                            {
                                error!(
                                    job_id = %dequeued.job_id,
                                    error = %e,
                                    "Failed to mark file completed"
                                );
                            }

                            // Increment job progress
                            if let Err(e) = self
                                .repository
                                .increment_job_progress(&dequeued.job_id, 1, chunks_created as i32)
                                .await
                            {
                                error!(
                                    job_id = %dequeued.job_id,
                                    error = %e,
                                    "Failed to update job progress"
                                );
                            }

                            // Check if job is complete (now that file is marked completed)
                            match self.repository.check_job_complete(&dequeued.job_id).await {
                                Ok(true) => {
                                    info!(job_id = %dequeued.job_id, "🎉 Job completed!");
                                    if let Err(e) = self
                                        .repository
                                        .complete_job(&dequeued.job_id, JobStatus::Completed, None)
                                        .await
                                    {
                                        error!(job_id = %dequeued.job_id, error = %e, "Failed to mark job complete");
                                    }
                                }
                                Ok(false) => {
                                    // Job still has more files
                                }
                                Err(e) => {
                                    error!(job_id = %dequeued.job_id, error = %e, "Failed to check job completion");
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                job_id = %dequeued.job_id,
                                file = %dequeued.file_path,
                                error = %e,
                                "❌ File processing failed"
                            );
                            // Don't fail the entire job - just this file
                            // Continue processing other files
                        }
                    }
                }
                Ok(None) => {
                    // No files available - sleep and retry
                    sleep(Duration::from_millis(self.config.poll_interval_ms)).await;
                }
                Err(e) => {
                    error!(error = %e, "Failed to dequeue file");
                    sleep(Duration::from_millis(self.config.poll_interval_ms * 5)).await; // Back off on errors
                }
            }
        }

        info!("🛑 Background indexing worker stopped");
    }

    /// Process a single file (parse + embed + store)
    ///
    /// Implements the complete indexing pipeline for a single file:
    /// 1. Encoding detection and UTF-8 conversion
    /// 2. File state checking (skip unchanged files)
    /// 3. Parsing into chunks
    /// 4. Embedding generation
    /// 5. Storage in Qdrant + PostgreSQL
    ///
    /// Returns the number of chunks created for progress tracking.
    async fn process_file(
        &self,
        dequeued: &codetriever_meta_data::DequeuedFile,
    ) -> IndexerResult<usize> {
        use codetriever_common::CorrelationId;
        use codetriever_parsing::get_language_from_extension;

        // Get job details to know repository_id and branch
        let job = self
            .repository
            .get_indexing_job(&dequeued.job_id)
            .await?
            .ok_or_else(|| IndexerError::Other(format!("Job not found: {}", dequeued.job_id)))?;

        // 1. Detect encoding and convert to UTF-8 (skip binary files)
        let encoding_result = match detect_and_convert_to_utf8(&dequeued.file_content) {
            Some(result) => result,
            None => {
                tracing::warn!("Skipping binary file {}", dequeued.file_path);
                return Ok(0); // No chunks for binary files
            }
        };

        let utf8_content = encoding_result.content;
        let detected_encoding = encoding_result.encoding_name;

        // 2. Check file state in database (skip unchanged files)
        let content_hash = codetriever_meta_data::hash_content(&utf8_content);
        let state = self
            .repository
            .check_file_state(
                &job.tenant_id,
                &job.repository_id,
                &job.branch,
                &dequeued.file_path,
                &content_hash,
            )
            .await?;

        let generation = match state {
            codetriever_meta_data::models::FileState::Unchanged => {
                tracing::debug!("Skipping unchanged file {}", dequeued.file_path);
                return Ok(0); // No chunks needed
            }
            codetriever_meta_data::models::FileState::New { generation }
            | codetriever_meta_data::models::FileState::Updated {
                new_generation: generation,
                ..
            } => {
                // Record file indexing with detected encoding
                #[allow(clippy::cast_possible_wrap)]
                let size_bytes = dequeued.file_content.len() as i64;
                let metadata = codetriever_meta_data::models::FileMetadata {
                    path: dequeued.file_path.clone(),
                    content: utf8_content.clone(),
                    content_hash: content_hash.clone(),
                    encoding: detected_encoding.clone(),
                    size_bytes,
                    generation,
                    commit_sha: job.commit_sha.clone(),
                    commit_message: job.commit_message.clone(),
                    commit_date: job.commit_date,
                    author: job.author.clone(),
                };
                self.repository
                    .record_file_indexing(
                        &job.tenant_id,
                        &job.repository_id,
                        &job.branch,
                        &metadata,
                    )
                    .await?;

                // For updated files, delete old chunks from both Qdrant and PostgreSQL
                if matches!(
                    state,
                    codetriever_meta_data::models::FileState::Updated { .. }
                ) {
                    let deleted_ids = self
                        .repository
                        .replace_file_chunks(
                            &job.tenant_id,
                            &job.repository_id,
                            &job.branch,
                            &dequeued.file_path,
                            generation,
                        )
                        .await?;
                    tracing::debug!(
                        "Deleted {} old chunks for {}",
                        deleted_ids.len(),
                        dequeued.file_path
                    );
                }

                generation
            }
        };

        // 3. Parse file into chunks
        let ext = dequeued.file_path.rsplit('.').next().unwrap_or("");
        let language = get_language_from_extension(ext).unwrap_or(ext);
        let parsing_chunks =
            self.code_parser
                .parse(&utf8_content, language, &dequeued.file_path)?;

        if parsing_chunks.is_empty() {
            tracing::warn!("File {} produced zero chunks", dequeued.file_path);
            return Ok(0);
        }

        let chunk_count = parsing_chunks.len();
        tracing::info!("Parsed {} chunks from {}", chunk_count, dequeued.file_path);

        // 4. Generate embeddings for all chunks
        let texts: Vec<&str> = parsing_chunks.iter().map(|c| c.content.as_str()).collect();
        let embeddings = self.embedding_service.generate_embeddings(texts).await?;

        // 5. Convert to vector data chunks and add embeddings
        let vector_chunks: Vec<codetriever_vector_data::CodeChunk> = parsing_chunks
            .iter()
            .zip(embeddings.iter())
            .map(
                |(parsing_chunk, embedding)| codetriever_vector_data::CodeChunk {
                    file_path: parsing_chunk.file_path.clone(),
                    content: parsing_chunk.content.clone(),
                    start_line: parsing_chunk.start_line,
                    end_line: parsing_chunk.end_line,
                    byte_start: parsing_chunk.byte_start,
                    byte_end: parsing_chunk.byte_end,
                    kind: parsing_chunk.kind.clone(),
                    language: parsing_chunk.language.clone(),
                    name: parsing_chunk.name.clone(),
                    token_count: parsing_chunk.token_count,
                    embedding: Some(embedding.clone()),
                },
            )
            .collect();

        // 6. Store chunks in Qdrant with full metadata context
        // Build storage context with available metadata
        // Note: commit_sha comes from job level (one commit per indexing job)
        // For per-file commit tracking, we'd need to enhance the queue schema
        let storage_context = codetriever_vector_data::ChunkStorageContext {
            tenant_id: job.tenant_id,
            repository_id: job.repository_id.clone(),
            branch: job.branch.clone(),
            generation,
            repository_url: Some(job.repository_url.clone()),
            commit_sha: Some(job.commit_sha.clone()),
            commit_message: Some(job.commit_message.clone()),
            commit_date: Some(job.commit_date),
            author: Some(job.author.clone()),
        };

        let correlation_id = CorrelationId::new();
        let chunk_ids = self
            .vector_storage
            .store_chunks(&storage_context, &vector_chunks, &correlation_id)
            .await?;

        // 7. Store chunk metadata in PostgreSQL
        let chunk_metadata: Vec<codetriever_meta_data::models::ChunkMetadata> = parsing_chunks
            .iter()
            .zip(chunk_ids.iter())
            .enumerate()
            .map(
                |(idx, (parsing_chunk, chunk_id))| codetriever_meta_data::models::ChunkMetadata {
                    chunk_id: *chunk_id,
                    tenant_id: job.tenant_id,
                    repository_id: job.repository_id.clone(),
                    branch: job.branch.clone(),
                    file_path: parsing_chunk.file_path.clone(),
                    chunk_index: idx as i32,
                    generation,
                    start_line: parsing_chunk.start_line as i32,
                    end_line: parsing_chunk.end_line as i32,
                    byte_start: parsing_chunk.byte_start as i64,
                    byte_end: parsing_chunk.byte_end as i64,
                    kind: parsing_chunk.kind.clone(),
                    name: parsing_chunk.name.clone(),
                    created_at: chrono::Utc::now(),
                },
            )
            .collect();

        self.repository
            .insert_chunks(
                &job.tenant_id,
                &job.repository_id,
                &job.branch,
                chunk_metadata,
            )
            .await?;

        tracing::info!("Stored {} chunks for {}", chunk_count, dequeued.file_path);

        Ok(chunk_count)
    }

    /// Process one file from the queue (for testing)
    ///
    /// Pulls a single file from the global queue, processes it, and returns.
    /// Useful for integration tests that need to control processing step-by-step.
    ///
    /// Returns the job_id of the processed file, or None if queue is empty.
    pub async fn process_one_job(&self) -> IndexerResult<Option<uuid::Uuid>> {
        match self.repository.dequeue_file().await? {
            Some(dequeued) => {
                let job_id = dequeued.job_id;

                // Process the file
                let chunks_created = self.process_file(&dequeued).await?;

                // Update progress
                self.repository
                    .increment_job_progress(&job_id, 1, chunks_created as i32)
                    .await?;

                // Check if job complete
                if self.repository.check_job_complete(&job_id).await? {
                    self.repository
                        .complete_job(&job_id, JobStatus::Completed, None)
                        .await?;
                }

                Ok(Some(job_id))
            }
            None => Ok(None),
        }
    }
}
