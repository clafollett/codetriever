//! Embedding model pool with request batching for parallel inference
//!
//! Provides a pool of embedding models that can process requests in parallel
//! while batching requests together for better GPU utilization.
//!
//! Architecture:
//! - Incoming requests → Main queue → Dispatcher → Per-worker queues → Workers
//! - Each worker has exclusive model access (Mutex) but processes in parallel
//! - Dispatcher uses round-robin to distribute load evenly

use super::dispatcher::{Dispatcher, RoundRobinDispatcher};
use super::model::EmbeddingModel;
use crate::{EmbeddingError, EmbeddingResult};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, OnceCell, mpsc, oneshot};

/// Type alias for embedding response to reduce complexity
type EmbeddingResponse = EmbeddingResult<Vec<Vec<f32>>>;

/// Request for generating embeddings
struct EmbeddingRequest {
    texts: Vec<String>,
    response_tx: oneshot::Sender<EmbeddingResponse>,
}

/// Pool of embedding models with request batching
///
/// Maintains multiple model instances that process requests in parallel.
/// Uses a dispatcher to distribute requests round-robin across workers.
pub struct EmbeddingModelPool {
    request_tx: mpsc::UnboundedSender<EmbeddingRequest>,
    pool_size: usize,
    pool_id: String,                                 // Unique ID for debugging
    tokenizer: OnceCell<Arc<tokenizers::Tokenizer>>, // Lazy-loaded tokenizer (thread-safe, no Mutex)
    model_id: String,
    max_tokens: usize,
}

// Global pool counter for debugging
static POOL_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl EmbeddingModelPool {
    /// Create a new embedding model pool with round-robin dispatcher
    ///
    /// # Arguments
    /// * `model_id` - HuggingFace model identifier
    /// * `max_tokens` - Maximum tokens per input
    /// * `pool_size` - Number of model instances (recommended: 2-3)
    /// * `batch_size` - Maximum texts to batch together
    /// * `batch_timeout` - Max time to wait collecting batch
    pub fn new(
        model_id: String,
        max_tokens: usize,
        pool_size: usize,
        batch_size: usize,
        batch_timeout: Duration,
    ) -> Self {
        let pool_id = format!(
            "pool-{}",
            POOL_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );
        tracing::debug!("Creating pool {pool_id} with {pool_size} workers");

        // Main request queue
        let (request_tx, request_rx) = mpsc::unbounded_channel();

        // Create per-worker channels
        let mut worker_channels = Vec::new();
        for worker_id in 0..pool_size {
            let (worker_tx, worker_rx) = mpsc::unbounded_channel();
            worker_channels.push(worker_tx);

            // Spawn worker with its own channel
            let model = Arc::new(Mutex::new(EmbeddingModel::new(
                model_id.clone(),
                max_tokens,
            )));

            let pool_id_clone = pool_id.clone();
            tokio::spawn(async move {
                tracing::debug!("Worker {worker_id} starting for {pool_id_clone}");

                // Wrap in panic catcher to see if worker is panicking
                let result = std::panic::AssertUnwindSafe(model_worker(
                    worker_id,
                    model,
                    worker_rx,
                    batch_size,
                    batch_timeout,
                ));

                match futures_util::future::FutureExt::catch_unwind(result).await {
                    Ok(_) => {
                        tracing::debug!("Worker {worker_id} exited normally for {pool_id_clone}");
                    }
                    Err(e) => {
                        tracing::error!("Worker {worker_id} PANICKED for {pool_id_clone}: {:?}", e);
                    }
                }
            });
        }

        // Spawn dispatcher task to distribute requests round-robin
        let pool_id_for_dispatcher = pool_id.clone();
        tokio::spawn(async move {
            tracing::debug!("Dispatcher starting for {pool_id_for_dispatcher}");

            // Wrap in panic catcher
            let result = std::panic::AssertUnwindSafe(dispatcher_task(
                request_rx,
                RoundRobinDispatcher::new(worker_channels),
            ));

            match futures_util::future::FutureExt::catch_unwind(result).await {
                Ok(_) => {
                    tracing::debug!("Dispatcher exited normally for {pool_id_for_dispatcher}");
                }
                Err(e) => {
                    tracing::error!("Dispatcher PANICKED for {pool_id_for_dispatcher}: {e:?}");
                }
            }
        });

        Self {
            request_tx,
            pool_size,
            pool_id,
            tokenizer: OnceCell::new(), // Thread-safe lazy init, no Mutex needed!
            model_id,
            max_tokens,
        }
    }

    /// Get the tokenizer for token counting (loads on first call, cached)
    ///
    /// Tokenizers are thread-safe for encoding, so we can share via Arc without Mutex.
    /// OnceCell ensures it's only loaded once across all threads.
    pub async fn get_tokenizer(&self) -> EmbeddingResult<Option<Arc<tokenizers::Tokenizer>>> {
        let tokenizer = self
            .tokenizer
            .get_or_try_init(|| async {
                // Load tokenizer by creating a temporary model
                let mut temp_model = EmbeddingModel::new(self.model_id.clone(), self.max_tokens);
                temp_model.ensure_model_loaded().await?;
                temp_model.get_tokenizer().ok_or_else(|| {
                    EmbeddingError::Embedding("Failed to load tokenizer".to_string())
                })
            })
            .await?;

        Ok(Some(tokenizer.clone()))
    }

    /// Submit a request for embedding generation
    ///
    /// Returns embeddings for the provided texts. Requests are distributed
    /// round-robin across the pool for parallel processing.
    pub async fn embed(&self, texts: Vec<String>) -> EmbeddingResult<Vec<Vec<f32>>> {
        let (response_tx, response_rx) = oneshot::channel();

        let request = EmbeddingRequest { texts, response_tx };

        if self.request_tx.send(request).is_err() {
            tracing::error!(
                pool_id = %self.pool_id,
                closed = self.request_tx.is_closed(),
                "Pool closed - dispatcher dropped"
            );
            return Err(EmbeddingError::Embedding("Pool closed".to_string()));
        }

        response_rx.await.map_err(|_| {
            tracing::error!("[{}] Worker dropped response!", self.pool_id);
            EmbeddingError::Embedding("Worker dropped response".to_string())
        })?
    }

    /// Get the number of models in the pool
    pub fn pool_size(&self) -> usize {
        self.pool_size
    }
}

/// Dispatcher task - pulls from main queue and distributes to workers
async fn dispatcher_task<D>(
    mut request_rx: mpsc::UnboundedReceiver<EmbeddingRequest>,
    mut dispatcher: D,
) where
    D: Dispatcher<EmbeddingRequest> + Send + 'static,
{
    let mut request_count = 0;
    tracing::trace!("Dispatcher: entering main loop, waiting for requests...");

    while let Some(request) = request_rx.recv().await {
        request_count += 1;
        tracing::trace!("Dispatcher: received request #{request_count} from main queue");

        // Dispatch takes ownership - if it fails, request is already moved
        tracing::trace!("Dispatcher: dispatching request #{request_count} to worker...");
        if !dispatcher.dispatch(request).await {
            tracing::error!(
                "Failed to dispatch request #{request_count} - all workers unavailable"
            );
        } else {
            tracing::trace!("Dispatcher: request #{request_count} dispatched successfully");
        }

        tracing::trace!(
            "Dispatcher: waiting for next request (total processed: {request_count})..."
        );
    }
    tracing::debug!("Dispatcher shutting down after {request_count} requests (request_rx closed)");
}

impl Drop for EmbeddingModelPool {
    fn drop(&mut self) {
        tracing::debug!("Dropping pool: {}", self.pool_id);
    }
}

/// Worker task that processes embedding requests with batching
///
/// Each worker:
/// 1. Pulls requests from its own queue (no contention!)
/// 2. Batches them together (up to batch_size or batch_timeout)
/// 3. Generates embeddings with exclusive model access
/// 4. Returns results to requesters
async fn model_worker(
    worker_id: usize,
    model: Arc<Mutex<EmbeddingModel>>,
    mut request_rx: mpsc::UnboundedReceiver<EmbeddingRequest>,
    batch_size: usize,
    batch_timeout: Duration,
) {
    tracing::trace!("Worker {worker_id}: entering main loop");
    loop {
        // Collect a batch of requests
        let mut batch: Vec<EmbeddingRequest> = Vec::new();

        // Get first request (blocking)
        tracing::trace!("Worker {worker_id}: waiting for request from dispatcher...");
        match request_rx.recv().await {
            Some(req) => {
                tracing::trace!("Worker {worker_id}: received request from dispatcher!");
                batch.push(req);
            }
            None => {
                tracing::trace!("Worker {worker_id}: channel closed, shutting down");
                return; // Channel closed - shutdown
            }
        }

        // Try to collect more requests (non-blocking with timeout)
        let deadline = tokio::time::Instant::now() + batch_timeout;
        while batch.len() < batch_size {
            match tokio::time::timeout_at(deadline, request_rx.recv()).await {
                Ok(Some(req)) => batch.push(req),
                Ok(None) => break, // Channel closed
                Err(_) => break,   // Timeout - process current batch
            }
        }

        let batch_len = batch.len();
        tracing::trace!("Worker {worker_id}: processing batch of {batch_len} requests");

        // Collect all texts from batch
        let all_texts: Vec<&str> = batch
            .iter()
            .flat_map(|r| r.texts.iter().map(|s| s.as_str()))
            .collect();
        tracing::trace!(
            "Worker {worker_id}: collected {} texts total",
            all_texts.len()
        );

        // Generate embeddings for entire batch (exclusive model access)
        tracing::trace!("Worker {worker_id}: acquiring model lock...");
        let result = {
            let mut model = model.lock().await;
            tracing::trace!("Worker {worker_id}: lock acquired, calling embed()...");
            let embed_result = model.embed(&all_texts).await;
            tracing::trace!("Worker {worker_id}: embed() returned, releasing lock");
            embed_result
        };

        // Distribute results back to requesters
        tracing::trace!("Worker {worker_id}: distributing results...");

        match result {
            Ok(embeddings) => {
                tracing::trace!(
                    "Worker {worker_id}: got {} embeddings, distributing...",
                    embeddings.len()
                );
                let mut offset = 0;
                for (idx, request) in batch.into_iter().enumerate() {
                    let count = request.texts.len();
                    let request_embeddings = embeddings[offset..offset + count].to_vec();
                    offset += count;

                    // Send result back (ignore if requester dropped)
                    if request.response_tx.send(Ok(request_embeddings)).is_err() {
                        tracing::warn!(
                            "Worker {worker_id}: requester {idx} dropped response channel"
                        );
                    }
                }
                tracing::trace!("Worker {worker_id}: all responses sent");
            }
            Err(e) => {
                tracing::error!("Worker {worker_id}: embedding failed: {e}");
                // Send error to all requesters in batch
                let error_msg = e.to_string();
                for request in batch {
                    let _ = request
                        .response_tx
                        .send(Err(EmbeddingError::Embedding(error_msg.clone())));
                }
            }
        }

        // Log batching stats
        if batch_len > 1 {
            tracing::debug!("Worker {worker_id}: batched {batch_len} requests");
        }
    }
}
