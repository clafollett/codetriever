# Queue-Based Indexing Refactor Plan

## Current State (19.35s with bugs fixed)

The indexer processes files in batches, accumulating all chunks in memory, then embedding them sequentially with timeout overhead from pool-level request batching.

## Problems with Current Approach

1. **Pool-level request batching designed for API concurrency, not indexing**
   - Workers wait up to `batch_timeout` (1ms) to collect `indexer_batch_size` requests
   - Indexing is sequential → each request times out → pure overhead

2. **Monolithic batching** - All chunks accumulated in `all_chunks` Vec
   - Works but not optimal for large codebases
   - Hard to add async/persistent queuing later

3. **Config naming misleading**
   - `INDEXER_CHUNK_BATCH_SIZE` controls pool request batching, not chunk batching!

## Proposed Queue Architecture

### Components

**FileContentQueue (Unbounded)**
- Accepts all incoming index requests
- Parsers pull from this queue

**ChunkQueue (Bounded at 1000)**
- Holds parsed chunks awaiting embedding
- Back pressure when full (blocks parsers)
- Embedders pull batches from this queue

### Worker Flow

```
HTTP Request
    ↓ push
[FileContentQueue] (unbounded)
    ↓ pop (CONCURRENT_FILE_LIMIT workers)
Parser Workers → parse, check DB state, produce chunks
    ↓ push_batch (blocks if queue full!)
[ChunkQueue] (capacity: 1000)
    ↓ pop_batch(CHUNK_BATCH_SIZE)
Embedding Workers (POOL_SIZE) → embed, store Qdrant + Postgres
```

### Key Changes to indexer.rs

**Replace lines 144-344 with:**

```rust
// Create queues
let file_queue = Arc::new(InMemoryFileQueue::new());
let chunk_queue = Arc::new(InMemoryChunkQueue::new(
    self.config.indexing.chunk_queue_capacity
));

// Push all files to queue
for file in files {
    file_queue.push(file).await?;
}

// Shared result tracking
let files_indexed = Arc::new(AtomicUsize::new(0));
let chunks_created = Arc::new(AtomicUsize::new(0));
let chunks_stored = Arc::new(AtomicUsize::new(0));

// Spawn parser workers
let mut parser_handles = vec![];
for worker_id in 0..self.config.indexing.concurrency_limit {
    let handle = spawn_parser_worker(
        worker_id,
        file_queue.clone(),
        chunk_queue.clone(),
        self.code_parser.clone(),
        self.repository.clone(),
        repository_id.clone(),
        branch.clone(),
        files_indexed.clone(),
    );
    parser_handles.push(handle);
}

// Spawn embedding workers
let mut embedding_handles = vec![];
for worker_id in 0..self.config.embedding.performance.pool_size {
    let handle = spawn_embedding_worker(
        worker_id,
        chunk_queue.clone(),
        self.embedding_service.clone(),
        self.storage.clone(),
        self.repository.clone(),
        repository_id.clone(),
        branch.clone(),
        self.config.embedding.performance.indexer_batch_size,
        chunks_created.clone(),
        chunks_stored.clone(),
    );
    embedding_handles.push(handle);
}

// Wait for all files to be parsed
for handle in parser_handles {
    handle.await??;
}

// Close file queue (signal parsers done)
drop(file_queue);

// Wait for all chunks to be embedded
for handle in embedding_handles {
    handle.await??;
}

let result = IndexResult {
    files_indexed: files_indexed.load(Ordering::Relaxed),
    chunks_created: chunks_created.load(Ordering::Relaxed),
    chunks_stored: chunks_stored.load(Ordering::Relaxed),
};
```

### Worker Functions

**parser_worker:**
1. Pop file from file_queue
2. Check file state in DB
3. Parse file → chunks
4. Push chunks to chunk_queue (blocks if full!)
5. Increment files_indexed counter

**embedding_worker:**
1. Pop batch of chunks from chunk_queue (up to CHUNK_BATCH_SIZE)
2. Call generate_embeddings() with batch
3. Store in Qdrant + Postgres
4. Increment counters

## Benefits

1. **No timeout overhead** - Each embedding call processes immediately
2. **Better resource control** - Bounded chunk queue prevents OOM
3. **Parallelism across files** - Chunks from different files mixed in batches
4. **Preparation for Issue #35** - Traits ready for Postgres implementation

## Expected Performance

- Current: 19.35s (sequential with timeout overhead)
- Target: ~12-15s (parallel, no timeouts, better GPU util)

## Testing Strategy

1. Run mini-redis test (20 files, 214 chunks)
2. Verify same result counts
3. Measure performance
4. Check memory usage stays reasonable

## Risks

- Complex refactor (200+ lines changed)
- New async coordination logic
- Potential race conditions in result tracking
- Need thorough testing before merging
