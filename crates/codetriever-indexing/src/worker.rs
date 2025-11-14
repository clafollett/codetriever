//! Background worker for processing indexing jobs
//!
//! This module provides a background worker that processes indexing jobs from the
//! persistent PostgreSQL queue using a two-level worker architecture for maximum
//! parallelism and throughput.
//!
//! # Architecture
//!
//! The worker uses a two-level pipeline:
//! 1. **Parser Workers** (N concurrent): Pull files from PostgreSQL → parse → push chunks to in-memory queue
//! 2. **Embedder Workers** (M concurrent): Pull chunks from in-memory queue → embed → store
//!
//! This architecture keeps ALL embedding models busy in parallel, achieving optimal GPU utilization.
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

use crate::queues::ChunkWithMetadata;
use crate::{IndexerError, IndexerResult};
use codetriever_config::ApplicationConfig;
use codetriever_embeddings::EmbeddingService;
use codetriever_meta_data::models::JobStatus;
use codetriever_meta_data::traits::FileRepository;
use codetriever_meta_data::{ChunkQueue, PostgresChunkQueue};
use codetriever_parsing::CodeParser;
use codetriever_vector_data::VectorStorage;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info};

/// Type alias for storage cache - maps vector namespace to cached QdrantStorage instances
type StorageCache = dashmap::DashMap<String, Arc<codetriever_vector_data::QdrantStorage>>;

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
/// This worker continuously polls the PostgreSQL queue for jobs and processes them
/// using a two-level architecture:
/// - Parser workers: Pull files from PostgreSQL → parse → push chunks to in-memory queue
/// - Embedder workers: Pull chunks from in-memory queue → embed → store in Qdrant/PostgreSQL
///
/// Designed to run as a background thread in the API binary, with zero web framework
/// dependencies for easy extraction to a separate daemon later.
pub struct BackgroundWorker {
    repository: Arc<dyn FileRepository>,
    embedding_service: Arc<dyn EmbeddingService>,
    qdrant_url: String,
    storage_cache: StorageCache,
    code_parser: Arc<CodeParser>,
    config: WorkerConfig,
    shutdown_signal: Arc<AtomicBool>,
    chunk_queue: Arc<PostgresChunkQueue>,
}

impl BackgroundWorker {
    /// Create a new background worker with two-level architecture
    pub fn new(
        repository: Arc<dyn FileRepository>,
        embedding_service: Arc<dyn EmbeddingService>,
        qdrant_url: String,
        code_parser: Arc<CodeParser>,
        config: WorkerConfig,
        chunk_queue: Arc<PostgresChunkQueue>,
    ) -> Self {
        Self {
            repository,
            embedding_service,
            qdrant_url,
            storage_cache: dashmap::DashMap::new(),
            code_parser,
            config,
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            chunk_queue,
        }
    }

    /// Get a handle for graceful shutdown
    pub fn shutdown_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown_signal)
    }

    /// Get or create a cached QdrantStorage instance for the given vector namespace
    ///
    /// Uses DashMap for lock-free concurrent access to the storage cache.
    /// Creates new storage on cache miss and reuses existing instances.
    async fn get_or_create_storage(
        &self,
        vector_namespace: &str,
    ) -> IndexerResult<Arc<codetriever_vector_data::QdrantStorage>> {
        // Check cache first (lock-free read)
        if let Some(storage) = self.storage_cache.get(vector_namespace) {
            tracing::debug!("✅ Cache hit for namespace: {}", vector_namespace);
            return Ok(storage.clone());
        }

        // Cache miss - create new storage
        tracing::info!(
            "📦 Creating new storage for namespace: {}",
            vector_namespace
        );
        let start = std::time::Instant::now();
        let storage = codetriever_vector_data::QdrantStorage::new(
            self.qdrant_url.clone(),
            vector_namespace.to_string(),
        )
        .await
        .map_err(|e| IndexerError::Other(format!("Failed to create storage: {e}")))?;
        tracing::info!("✅ Storage created in {}ms", start.elapsed().as_millis());

        let storage = Arc::new(storage);

        // Insert into cache
        self.storage_cache
            .insert(vector_namespace.to_string(), storage.clone());

        Ok(storage)
    }

    /// Main worker loop - spawns parser and embedder workers in two-level architecture
    ///
    /// This method runs indefinitely until shutdown is signaled. It spawns:
    /// - N parser workers that pull files from PostgreSQL and push chunks to in-memory queue
    /// - M embedder workers that pull chunks from queue, embed them, and store results
    ///
    /// # Graceful Shutdown
    ///
    /// Set the shutdown signal to trigger graceful shutdown. The worker will
    /// close queues, wait for all workers to finish, then exit.
    pub async fn run(&self) {
        info!(
            "🚀 Background indexing worker started (parsers: {}, embedders: {})",
            self.config.parser_concurrency, self.config.embedder_concurrency
        );

        // Shared counters for progress tracking
        let files_processed = Arc::new(AtomicUsize::new(0));
        let chunks_created = Arc::new(AtomicUsize::new(0));

        let mut join_set = tokio::task::JoinSet::new();

        // Spawn parser workers (pull from PostgreSQL → parse → push chunks to queue)
        for worker_id in 0..self.config.parser_concurrency {
            let repository = Arc::clone(&self.repository);
            let code_parser = Arc::clone(&self.code_parser);
            let chunk_queue = Arc::clone(&self.chunk_queue);
            let shutdown = Arc::clone(&self.shutdown_signal);
            let files_processed = Arc::clone(&files_processed);
            let chunks_created = Arc::clone(&chunks_created);
            let poll_interval = self.config.poll_interval_ms;

            join_set.spawn(async move {
                parser_worker(
                    worker_id,
                    repository,
                    code_parser,
                    chunk_queue,
                    shutdown,
                    files_processed,
                    chunks_created,
                    poll_interval,
                )
                .await
            });
        }

        // Spawn embedder workers (pull chunks from queue → embed → store)
        for worker_id in 0..self.config.embedder_concurrency {
            let embedding_service = Arc::clone(&self.embedding_service);
            let chunk_queue = Arc::clone(&self.chunk_queue);
            let shutdown = Arc::clone(&self.shutdown_signal);
            let batch_size = self.config.embedding_batch_size;
            let storage_cache = self.storage_cache.clone();
            let qdrant_url = self.qdrant_url.clone();
            let repository = Arc::clone(&self.repository);

            join_set.spawn(async move {
                embedder_worker(
                    worker_id,
                    embedding_service,
                    chunk_queue,
                    storage_cache,
                    qdrant_url,
                    repository,
                    shutdown,
                    batch_size,
                )
                .await
            });
        }

        // Wait for shutdown signal
        loop {
            if self.shutdown_signal.load(Ordering::Relaxed) {
                info!("📛 Shutdown signal received, waiting for workers to finish");
                // NOTE: PostgreSQL queue doesn't need explicit close() - workers will exit naturally
                break;
            }
            sleep(Duration::from_millis(self.config.poll_interval_ms)).await;
        }

        // Wait for all workers to complete
        info!("⏳ Waiting for {} workers to complete", join_set.len());
        while let Some(result) = join_set.join_next().await {
            if let Err(e) = result {
                error!(error = %e, "Worker task panicked");
            }
        }

        info!(
            "🛑 Background indexing worker stopped gracefully (processed {} files, {} chunks)",
            files_processed.load(Ordering::Relaxed),
            chunks_created.load(Ordering::Relaxed)
        );
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

                // Get job details
                let job = self
                    .repository
                    .get_indexing_job(&job_id)
                    .await?
                    .ok_or_else(|| IndexerError::Other(format!("Job not found: {job_id}")))?;

                // Parse file
                let file_path = dequeued.file_path.clone();
                let parsing_result =
                    parse_file(&dequeued, &job, &self.code_parser, &self.repository).await?;

                if let Some((chunks, generation)) = parsing_result {
                    let chunk_count = chunks.len();

                    // Create ChunkWithMetadata entries
                    let chunks_with_metadata: Vec<ChunkWithMetadata> = chunks
                        .into_iter()
                        .enumerate()
                        .map(|(idx, chunk)| ChunkWithMetadata {
                            chunk,
                            generation,
                            file_chunk_index: idx,
                            job_id,
                            correlation_id: job.correlation_id,
                            tenant_id: job.tenant_id,
                            repository_id: job.repository_id.clone(),
                            branch: job.branch.clone(),
                            vector_namespace: dequeued.vector_namespace.clone(),
                            repository_url: job.repository_url.clone(),
                            commit_sha: job.commit_sha.clone(),
                            commit_message: job.commit_message.clone(),
                            commit_date: job.commit_date,
                            author: job.author.clone(),
                        })
                        .collect();

                    // Embed and store (synchronously for testing)
                    let storage = self
                        .get_or_create_storage(&dequeued.vector_namespace)
                        .await?;
                    embed_and_store_chunks(
                        &chunks_with_metadata,
                        &self.embedding_service,
                        &storage,
                        &self.repository,
                    )
                    .await?;

                    // Update progress
                    self.repository
                        .mark_file_completed(&job_id, &file_path)
                        .await?;
                    self.repository
                        .increment_files_processed(&job_id, 1)
                        .await?;
                    self.repository
                        .increment_chunks_created(&job_id, chunk_count as i32)
                        .await?;

                    // Check if job complete
                    if self.repository.check_job_complete(&job_id).await? {
                        self.repository
                            .complete_job(&job_id, JobStatus::Completed, None)
                            .await?;
                    }
                }

                Ok(Some(job_id))
            }
            None => Ok(None),
        }
    }
}

/// Parser worker: pulls files from PostgreSQL, parses them, pushes chunks to queue
#[allow(clippy::too_many_arguments)]
async fn parser_worker(
    worker_id: usize,
    repository: Arc<dyn FileRepository>,
    code_parser: Arc<CodeParser>,
    chunk_queue: Arc<PostgresChunkQueue>,
    shutdown: Arc<AtomicBool>,
    files_processed: Arc<AtomicUsize>,
    chunks_created: Arc<AtomicUsize>,
    poll_interval_ms: u64,
) -> IndexerResult<()> {
    tracing::debug!("Parser worker {worker_id} starting");

    loop {
        // Check shutdown
        if shutdown.load(Ordering::Relaxed) {
            tracing::debug!("Parser worker {worker_id}: shutdown signal received");
            break;
        }

        // Try to dequeue a file
        match repository.dequeue_file().await {
            Ok(Some(dequeued)) => {
                let job_id = dequeued.job_id;
                let file_path = dequeued.file_path.clone();

                tracing::debug!(
                    "Parser worker {worker_id}: processing file {file_path} from job {job_id}"
                );

                // Get job details
                let job = match repository.get_indexing_job(&job_id).await? {
                    Some(job) => job,
                    None => {
                        error!("Job {job_id} not found");
                        continue;
                    }
                };

                // Parse file
                match parse_file(&dequeued, &job, &code_parser, &repository).await {
                    Ok(Some((chunks, generation))) => {
                        let chunk_count = chunks.len();
                        chunks_created.fetch_add(chunk_count, Ordering::Relaxed);

                        tracing::info!(
                            "Parser worker {worker_id}: parsed {chunk_count} chunks from {file_path}"
                        );

                        // Create ChunkWithMetadata entries and serialize to JSONB for PostgreSQL
                        let chunks_with_metadata: Vec<serde_json::Value> = chunks
                            .into_iter()
                            .enumerate()
                            .map(|(idx, chunk)| {
                                let chunk_meta = ChunkWithMetadata {
                                    chunk,
                                    generation,
                                    file_chunk_index: idx,
                                    job_id,
                                    correlation_id: job.correlation_id,
                                    tenant_id: job.tenant_id,
                                    repository_id: job.repository_id.clone(),
                                    branch: job.branch.clone(),
                                    vector_namespace: dequeued.vector_namespace.clone(),
                                    repository_url: job.repository_url.clone(),
                                    commit_sha: job.commit_sha.clone(),
                                    commit_message: job.commit_message.clone(),
                                    commit_date: job.commit_date,
                                    author: job.author.clone(),
                                };
                                serde_json::to_value(chunk_meta).expect("Failed to serialize chunk")
                            })
                            .collect();

                        // Enqueue to PostgreSQL (persistent, crash-recoverable!)
                        if let Err(e) = chunk_queue
                            .enqueue_chunks(job_id, chunks_with_metadata)
                            .await
                        {
                            error!("Parser worker {worker_id}: failed to enqueue chunks: {e}");
                            continue;
                        }

                        // Mark file as completed
                        if let Err(e) = repository.mark_file_completed(&job_id, &file_path).await {
                            error!("Failed to mark file completed: {e}");
                        }

                        // Increment files processed counter
                        if let Err(e) = repository.increment_files_processed(&job_id, 1).await {
                            error!("Failed to increment files processed: {e}");
                        }

                        files_processed.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(None) => {
                        // File was skipped (binary, unchanged, etc.)
                        // Still need to mark it as completed and check job completion

                        // Mark file as completed
                        if let Err(e) = repository.mark_file_completed(&job_id, &file_path).await {
                            error!("Failed to mark skipped file completed: {e}");
                        }

                        // Increment files processed counter in DB
                        if let Err(e) = repository.increment_files_processed(&job_id, 1).await {
                            error!("Failed to increment files processed for skipped file: {e}");
                        }

                        files_processed.fetch_add(1, Ordering::Relaxed);

                        // Check if all files AND chunks are done (important for all-unchanged case!)
                        // Must check BOTH file queue and chunk queue to avoid premature completion
                        let files_done = repository
                            .check_job_complete(&job_id)
                            .await
                            .unwrap_or(false);
                        let chunks_done = chunk_queue
                            .check_job_complete(job_id)
                            .await
                            .unwrap_or(false);

                        if files_done && chunks_done {
                            info!(
                                correlation_id = %job.correlation_id,
                                job_id = %job_id,
                                "Parser worker {worker_id}: Job complete (all files and chunks processed)"
                            );
                            if let Err(e) = repository
                                .complete_job(&job_id, JobStatus::Completed, None)
                                .await
                            {
                                error!("Failed to mark job complete: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        error!("Parser worker {worker_id}: failed to parse {file_path}: {e}");
                    }
                }
            }
            Ok(None) => {
                // No files available - sleep
                sleep(Duration::from_millis(poll_interval_ms)).await;
            }
            Err(e) => {
                error!("Parser worker {worker_id}: failed to dequeue file: {e}");
                sleep(Duration::from_millis(poll_interval_ms * 5)).await;
            }
        }
    }

    tracing::debug!("Parser worker {worker_id} shutting down");
    Ok(())
}

/// Embedder worker: pulls chunks from queue, embeds them, stores results
#[allow(clippy::too_many_arguments)]
async fn embedder_worker(
    worker_id: usize,
    embedding_service: Arc<dyn EmbeddingService>,
    chunk_queue: Arc<PostgresChunkQueue>,
    storage_cache: StorageCache,
    qdrant_url: String,
    repository: Arc<dyn FileRepository>,
    shutdown: Arc<AtomicBool>,
    batch_size: usize,
) -> IndexerResult<()> {
    tracing::debug!("Embedder worker {worker_id} starting");

    loop {
        // Check shutdown - exit immediately
        if shutdown.load(Ordering::Relaxed) {
            tracing::debug!("Embedder worker {worker_id}: shutdown signal received");
            break;
        }

        // Dequeue chunks from PostgreSQL (SKIP LOCKED pattern)
        let claimed_chunks = match chunk_queue
            .dequeue_chunks(&format!("embedder-{worker_id}"), batch_size as i32, 300)
            .await
        {
            Ok(chunks) if chunks.is_empty() => {
                // No chunks available - sleep
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            Ok(chunks) => chunks,
            Err(e) => {
                error!("Embedder worker {worker_id}: failed to dequeue chunks: {e}");
                tokio::time::sleep(Duration::from_millis(1000)).await;
                continue;
            }
        };

        let chunk_ids: Vec<uuid::Uuid> = claimed_chunks.iter().map(|(id, _)| *id).collect();
        let chunk_count = claimed_chunks.len();

        tracing::debug!("Embedder worker {worker_id}: processing {chunk_count} chunks");

        // Deserialize chunks from JSONB
        let chunks: Vec<ChunkWithMetadata> = claimed_chunks
            .iter()
            .filter_map(|(_, data)| {
                serde_json::from_value(data.clone())
                    .map_err(|e| {
                        error!("Failed to deserialize chunk: {e}");
                        e
                    })
                    .ok()
            })
            .collect();

        if chunks.is_empty() {
            error!("Embedder worker {worker_id}: all chunks failed deserialization");
            continue;
        }

        // Get storage for this namespace (use first chunk's namespace - all should be same)
        let vector_namespace = &chunks[0].vector_namespace;
        let storage =
            match get_or_create_storage_cached(&storage_cache, &qdrant_url, vector_namespace).await
            {
                Ok(s) => s,
                Err(e) => {
                    error!("Embedder worker {worker_id}: failed to get storage: {e}");
                    // Requeue chunks on storage creation failure
                    for chunk_id in &chunk_ids {
                        let _ = chunk_queue
                            .requeue_chunk(*chunk_id, &format!("Storage creation failed: {e}"))
                            .await;
                    }
                    continue;
                }
            };

        // Embed and store
        if let Err(e) =
            embed_and_store_chunks(&chunks, &embedding_service, &storage, &repository).await
        {
            error!("Embedder worker {worker_id}: failed to embed/store chunks: {e}");
            // Requeue chunks on embedding/storage failure
            for chunk_id in &chunk_ids {
                let _ = chunk_queue
                    .requeue_chunk(*chunk_id, &format!("Embedding failed: {e}"))
                    .await;
            }
            continue;
        }

        // SUCCESS! Acknowledge chunks as completed
        if let Err(e) = chunk_queue.ack_chunks(&chunk_ids).await {
            error!("Embedder worker {worker_id}: failed to ack chunks: {e}");
        }

        // Update job progress (group by job_id)
        let mut job_chunks: std::collections::HashMap<uuid::Uuid, i32> =
            std::collections::HashMap::new();
        for chunk_meta in &chunks {
            *job_chunks.entry(chunk_meta.job_id).or_insert(0) += 1;
        }

        for (job_id, chunk_count) in job_chunks {
            // Get correlation_id for this job (all chunks in same job have same correlation_id)
            let correlation_id = chunks
                .iter()
                .find(|c| c.job_id == job_id)
                .map(|c| c.correlation_id);

            if let Err(e) = repository
                .increment_chunks_created(&job_id, chunk_count)
                .await
            {
                error!("Failed to increment chunks created for {job_id}: {e}");
            }

            // Check if job is complete (all chunks processed)
            match chunk_queue.check_job_complete(job_id).await {
                Ok(true) => {
                    if let Some(corr_id) = correlation_id {
                        info!(
                            correlation_id = %corr_id,
                            job_id = %job_id,
                            worker_id = worker_id,
                            "Embedder worker: Job complete!"
                        );
                    } else {
                        info!("Embedder worker {worker_id}: Job {job_id} complete!");
                    }
                    if let Err(e) = repository
                        .complete_job(&job_id, JobStatus::Completed, None)
                        .await
                    {
                        error!("Failed to mark job complete: {e}");
                    }
                }
                Ok(false) => {}
                Err(e) => {
                    error!("Failed to check job completion: {e}");
                }
            }
        }

        // Extract correlation_id from first chunk (all chunks in batch have same correlation_id)
        let correlation_id = chunks.first().map(|c| c.correlation_id);

        if let Some(corr_id) = correlation_id {
            tracing::debug!(
                correlation_id = %corr_id,
                worker_id = worker_id,
                chunk_count = chunk_count,
                "Embedder worker stored chunks"
            );
        } else {
            tracing::debug!("Embedder worker {worker_id}: stored {chunk_count} chunks");
        }
    }

    tracing::debug!("Embedder worker {worker_id} shutting down");
    Ok(())
}

/// Parse a file and return chunks + generation, or None if skipped
async fn parse_file(
    dequeued: &codetriever_meta_data::DequeuedFile,
    job: &codetriever_meta_data::IndexingJob,
    code_parser: &CodeParser,
    repository: &Arc<dyn FileRepository>,
) -> IndexerResult<Option<(Vec<codetriever_parsing::CodeChunk>, i64)>> {
    use codetriever_parsing::get_language_from_extension;

    // 1. Detect encoding and convert to UTF-8
    let encoding_result = match detect_and_convert_to_utf8(&dequeued.file_content) {
        Some(result) => result,
        None => {
            tracing::warn!("Skipping binary file {}", dequeued.file_path);
            return Ok(None);
        }
    };

    let utf8_content = encoding_result.content;
    let detected_encoding = encoding_result.encoding_name;

    // 2. Check file state in database
    let content_hash = codetriever_meta_data::hash_content(&utf8_content);
    let state = repository
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
            tracing::debug!(
                file = %dequeued.file_path,
                hash = %content_hash,
                "Skipping unchanged file"
            );
            return Ok(None);
        }
        codetriever_meta_data::models::FileState::New { generation }
        | codetriever_meta_data::models::FileState::Updated {
            new_generation: generation,
            ..
        } => {
            // Record file indexing
            let metadata = codetriever_meta_data::models::FileMetadata {
                path: dequeued.file_path.clone(),
                content: utf8_content.clone(),
                content_hash: content_hash.clone(),
                encoding: detected_encoding.clone(),
                size_bytes: dequeued.file_content.len() as i64,
                generation,
                commit_sha: job.commit_sha.clone(),
                commit_message: job.commit_message.clone(),
                commit_date: job.commit_date,
                author: job.author.clone(),
            };
            repository
                .record_file_indexing(&job.tenant_id, &job.repository_id, &job.branch, &metadata)
                .await?;

            // For updated files, delete old chunks
            if matches!(
                state,
                codetriever_meta_data::models::FileState::Updated { .. }
            ) {
                let deleted_ids = repository
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
    let chunks = code_parser.parse(&utf8_content, language, &dequeued.file_path)?;

    if chunks.is_empty() {
        tracing::warn!("File {} produced zero chunks", dequeued.file_path);
        return Ok(None);
    }

    Ok(Some((chunks, generation)))
}

/// Embed chunks and store them in Qdrant + PostgreSQL
async fn embed_and_store_chunks(
    chunks_with_metadata: &[ChunkWithMetadata],
    embedding_service: &Arc<dyn EmbeddingService>,
    storage: &Arc<codetriever_vector_data::QdrantStorage>,
    repository: &Arc<dyn FileRepository>,
) -> IndexerResult<()> {
    use codetriever_common::CorrelationId;

    // 1. Generate embeddings
    let texts: Vec<&str> = chunks_with_metadata
        .iter()
        .map(|c| c.chunk.content.as_str())
        .collect();
    let embeddings = embedding_service.generate_embeddings(texts).await?;

    // 2. Create vector chunks with embeddings
    let vector_chunks: Vec<codetriever_vector_data::CodeChunk> = chunks_with_metadata
        .iter()
        .zip(embeddings.iter())
        .map(
            |(chunk_meta, embedding)| codetriever_vector_data::CodeChunk {
                file_path: chunk_meta.chunk.file_path.clone(),
                content: chunk_meta.chunk.content.clone(),
                start_line: chunk_meta.chunk.start_line,
                end_line: chunk_meta.chunk.end_line,
                byte_start: chunk_meta.chunk.byte_start,
                byte_end: chunk_meta.chunk.byte_end,
                kind: chunk_meta.chunk.kind.clone(),
                language: chunk_meta.chunk.language.clone(),
                name: chunk_meta.chunk.name.clone(),
                token_count: chunk_meta.chunk.token_count,
                embedding: Some(embedding.clone()),
            },
        )
        .collect();

    // 3. Build storage context (use first chunk's metadata - all from same file)
    let first = &chunks_with_metadata[0];
    let storage_context = codetriever_vector_data::ChunkStorageContext {
        tenant_id: first.tenant_id,
        repository_id: first.repository_id.clone(),
        branch: first.branch.clone(),
        generation: first.generation,
        repository_url: Some(first.repository_url.clone()),
        commit_sha: Some(first.commit_sha.clone()),
        commit_message: Some(first.commit_message.clone()),
        commit_date: Some(first.commit_date),
        author: Some(first.author.clone()),
    };

    let correlation_id = CorrelationId::new();

    // 4. Store in Qdrant
    let chunk_ids = storage
        .store_chunks(&storage_context, &vector_chunks, &correlation_id)
        .await
        .map_err(|e| IndexerError::Other(format!("Failed to store chunks in Qdrant: {e}")))?;

    // 5. Store metadata in PostgreSQL
    let chunk_metadata: Vec<codetriever_meta_data::models::ChunkMetadata> = chunks_with_metadata
        .iter()
        .zip(chunk_ids.iter())
        .map(
            |(chunk_meta, chunk_id)| codetriever_meta_data::models::ChunkMetadata {
                chunk_id: *chunk_id,
                tenant_id: chunk_meta.tenant_id,
                repository_id: chunk_meta.repository_id.clone(),
                branch: chunk_meta.branch.clone(),
                file_path: chunk_meta.chunk.file_path.clone(),
                chunk_index: chunk_meta.file_chunk_index as i32,
                generation: chunk_meta.generation,
                start_line: chunk_meta.chunk.start_line as i32,
                end_line: chunk_meta.chunk.end_line as i32,
                byte_start: chunk_meta.chunk.byte_start as i64,
                byte_end: chunk_meta.chunk.byte_end as i64,
                kind: chunk_meta.chunk.kind.clone(),
                name: chunk_meta.chunk.name.clone(),
                created_at: chrono::Utc::now(),
            },
        )
        .collect();

    repository
        .insert_chunks(
            &first.tenant_id,
            &first.repository_id,
            &first.branch,
            chunk_metadata,
        )
        .await?;

    Ok(())
}

/// Get or create storage from cache
async fn get_or_create_storage_cached(
    storage_cache: &StorageCache,
    qdrant_url: &str,
    vector_namespace: &str,
) -> IndexerResult<Arc<codetriever_vector_data::QdrantStorage>> {
    // Check cache first
    if let Some(storage) = storage_cache.get(vector_namespace) {
        return Ok(storage.clone());
    }

    // Create new storage
    let storage = codetriever_vector_data::QdrantStorage::new(
        qdrant_url.to_string(),
        vector_namespace.to_string(),
    )
    .await
    .map_err(|e| IndexerError::Other(format!("Failed to create storage: {e}")))?;

    let storage = Arc::new(storage);
    storage_cache.insert(vector_namespace.to_string(), storage.clone());

    Ok(storage)
}
