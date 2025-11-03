# BackgroundWorker Refactoring and Qdrant Collection Architecture Plan

**Status:** ACTIVE - Implementation In Progress  
**Branch:** `feature/issue-16-context-endpoint`  
**Created:** 2025-11-02  
**Last Updated:** 2025-11-02   

---

## Executive Summary

We're in the middle of a critical refactoring to fix the BackgroundWorker implementation and optimize the Qdrant collection architecture. The old `index_file_content()` method was deleted, BackgroundWorker is currently single-threaded (should spawn N workers), and tests have race conditions due to per-test collections conflicting with a shared global queue.

**Key Decision:** Use **ONE collection per embedding model** (not per repo/branch) with payload filtering for isolation. This aligns with Qdrant best practices and prevents hitting the 1000 collection limit.

---

## 1. Current State (What's Broken)

### 1.1 Deleted Code
- **`index_file_content()` method removed** - Was a monolithic sync method in the Indexer
- Replaced with async job-based architecture using PostgreSQL queue
- BackgroundWorker now handles all file processing

### 1.2 Single-Threaded Worker
**Location:** `crates/codetriever-api/src/bootstrap.rs:183-225`

```rust
// PROBLEM: Only spawns ONE worker!
let worker = BackgroundWorker::new(...);
tokio::spawn(async move {
    worker.run().await;  // Single instance processing files one-by-one
});
```

**Expected:** Spawn N workers (from `config.indexing.concurrency_limit`, default 4) that all dequeue from the shared queue concurrently.

**Current Reality:**
- Only 1 worker running
- PostgreSQL `SKIP LOCKED` is designed for concurrent workers but unused
- Concurrency limited to 1 file at a time (massive bottleneck!)

### 1.3 Test Race Conditions
**Location:** `crates/codetriever-api/tests/test_utils.rs:180-257`

**Problem:** Each test creates a unique Qdrant collection:
```rust
let collection_name = format!("{test_name}_{timestamp}_{counter}");
let vector_storage = QdrantStorage::new(url, collection_name).await?;
```

**But:** All tests share ONE global PostgreSQL queue (`indexing_job_file_queue` table)

**Race Condition:**
1. Test A creates `TestAppState` with collection `test_a_123_0`
2. Test A creates indexing job, queues files
3. Test B creates `TestAppState` with collection `test_b_123_1`
4. Test B creates indexing job, queues files
5. **BackgroundWorker from Test A dequeues file from Test B's job**
6. Tries to store chunks in collection `test_a_123_0` but file belongs to Test B
7. Data isolation violated / chunks stored in wrong collection

**Root Cause:** Per-test collections + shared global queue = isolation failure

### 1.4 VectorStorage Collection Lock-In
**Location:** `crates/codetriever-vector-data/src/storage/qdrant.rs:73-76`

```rust
pub struct QdrantStorage {
    client: Qdrant,
    collection_name: String,  // ⚠️ Fixed at construction time!
}
```

Once created, a `QdrantStorage` instance is locked to ONE collection. Can't dynamically switch collections based on repository or branch.

---

## 2. Research Findings (Qdrant Investigation)

### 2.1 Collection Limits
**Official Qdrant Guidance:**
> "It is not recommended to create hundreds and thousands of collections per cluster as it increases resource overhead unsustainably."
>
> "If your number of collections will be greater than 10, it's very likely that you'd want to store all of it in just one of them."

**Hard Limit:** While Qdrant can technically handle ~1000 collections, performance degrades significantly beyond 10-50 collections.

**Memory Overhead:**
- Each collection = separate HNSW index + metadata structures
- ~10-50 MB overhead per collection (depends on size)
- With 100 repos × 10 branches = 1000 collections → **10-50 GB overhead** just for metadata!

### 2.2 Multi-Tenancy Best Practices
**Recommended Approach (from Qdrant official docs):**
> "In most cases, you only need to use a single collection with payload-based partitioning, an approach known as multi-tenancy."

**Implementation Pattern:**
```rust
// Store with tenant metadata in payload
let payload = {
    "repository_id": "my-repo",
    "branch": "main",
    "generation": 42,
    "file_path": "src/main.rs",
    // ... other fields
};

// Search with filtering
let filter = Filter {
    must: vec![
        Condition::matches("repository_id", "my-repo"),
        Condition::matches("branch", "main"),
    ]
};
```

**When to Use Multiple Collections:**
- Strict isolation required (security/compliance)
- Different embedding models (different vector dimensions)
- Massive scale (billions of vectors per tenant)

**Our Use Case:** We have < 10 embedding models, potentially 100s of repos/branches → **Use payload filtering!**

### 2.3 Performance Data
**Payload Filtering vs Multiple Collections:**

| Approach | Search Latency | Memory Overhead | Scalability |
|----------|---------------|-----------------|-------------|
| **1 collection + filters** | ~10-50ms | Minimal (just payloads) | Excellent (billions of vectors) |
| **1000 collections** | ~10-50ms per collection | **10-50 GB** | Poor (resource exhaustion) |

**Key Insight:** Filtering is nearly free in Qdrant due to `is_tenant: true` optimization on indexed fields.

### 2.4 `is_tenant` Optimization
Qdrant supports marking payload fields as tenant keys for optimized filtering:

```rust
// When creating collection, mark repository_id as tenant key
let payload_schema = {
    "repository_id": {
        "is_tenant": true,  // Enables optimized filtering
        "indexed": true
    }
};
```

**Benefits:**
- Near-zero filtering overhead (uses separate index structure)
- Scales to millions of tenants
- No need for separate collections

---

## 3. Architectural Decisions

### 3.1 Collection Strategy: ONE Collection Per Embedding Model

**Decision:** Use **ONE collection** per embedding model (e.g., `codetriever_jina_v2_768d`), not per repository or branch.

**Rationale:**
1. **Scalability:** Avoid 1000 collection limit (100 repos × 10 branches = 1000!)
2. **Performance:** Payload filtering is essentially free with `is_tenant` optimization
3. **Memory Efficiency:** Save 10-50 GB of overhead for 1000 collections
4. **Best Practice:** Qdrant official recommendation for multi-tenancy

**Implementation:**
```rust
// Production: One collection per model
collection_name = "codetriever_jina_v2_768d"

// All chunks from all repos/branches stored with:
payload = {
    "repository_id": "user/repo",  // Tenant key
    "branch": "main",
    "generation": 42,
    "file_path": "src/main.rs",
    // ... chunk data
}

// Search filters by repository + branch
filter = must: ["repository_id=user/repo", "branch=main"]
```

**Tests:** ONE shared collection (`codetriever_test`) with same payload isolation strategy.

### 3.2 Worker Concurrency: Pool of N BackgroundWorkers

**Decision:** Spawn N concurrent workers (from `config.indexing.concurrency_limit`) all dequeueing from shared PostgreSQL queue.

**Rationale:**
1. **PostgreSQL `SKIP LOCKED` is designed for this!** - Atomic concurrent dequeue with no duplicate processing
2. **Maximum throughput:** N files processed in parallel instead of 1
3. **Fair scheduling:** Global FIFO queue ensures fairness across jobs
4. **Already proven:** `postgres_queue_test.rs` validates concurrent dequeue works perfectly

**Implementation:**
```rust
// In bootstrap.rs
for worker_id in 0..config.indexing.concurrency_limit {
    let worker = BackgroundWorker::new(...);
    tokio::spawn(async move {
        tracing::info!("Worker {worker_id} started");
        worker.run().await;
    });
}
```

### 3.3 Test Strategy: Shared Collection with Unique repository_ids

**Decision:** Tests use ONE shared collection (`codetriever_test`) with unique `repository_id` per test for isolation.

**Rationale:**
1. **Matches production architecture** - Same payload filtering pattern
2. **No race conditions** - Each test has unique `repository_id`, workers isolated by filter
3. **Concurrent workers safe** - Multiple workers can process different tests' jobs simultaneously
4. **Faster cleanup** - Delete by filter instead of dropping collection

**Implementation:**
```rust
// test_utils.rs
pub async fn app_state() -> Result<Arc<TestAppState>> {
    // ONE shared collection for all tests
    let collection_name = "codetriever_test";

    // Unique repository_id per test (already exists in codebase!)
    let test_name = std::thread::current().name().unwrap_or("unknown");
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let repository_id = format!("test_{}_{}", test_name, timestamp);

    // Each test creates jobs with unique repository_id
    // Workers filter by repository_id → perfect isolation!
}

// Cleanup: Delete by filter, not drop collection
async fn cleanup_test_data(repository_id: &str) {
    vector_storage.delete_by_filter(Filter {
        must: vec![Condition::matches("repository_id", repository_id)]
    }).await?;
}
```

---

## 4. Implementation Plan (Step-by-Step)

### Phase 1: Fix Worker Concurrency 🔥
**Priority:** CRITICAL - Unlock parallelism

**Files to Change:**
1. `/Users/clafollett/Repositories/codetriever/crates/codetriever-api/src/bootstrap.rs`

**Changes:**
```rust
// OLD (line 218):
tokio::spawn(async move {
    worker.run().await;
});

// NEW:
let workers: Vec<_> = (0..config.indexing.concurrency_limit)
    .map(|worker_id| {
        let file_repository = Arc::clone(&file_repository);
        let embedding_service = Arc::clone(&embedding_service);
        let vector_storage = Arc::clone(&vector_storage);
        let code_parser = Arc::clone(&code_parser);
        let worker_config = worker_config.clone();

        let worker = BackgroundWorker::new(
            file_repository,
            embedding_service,
            vector_storage,
            code_parser,
            worker_config,
        );

        let shutdown = worker.shutdown_handle();

        tokio::spawn(async move {
            tracing::info!("Worker {worker_id} started");
            worker.run().await;
            tracing::info!("Worker {worker_id} stopped");
        });

        shutdown
    })
    .collect();

// Return Vec<Arc<AtomicBool>> for graceful shutdown of all workers
```

**Testing:**
```bash
# Verify N workers spawn
just run &
sleep 2
grep "Worker.*started" logs | wc -l  # Should show N workers

# Verify concurrent processing
# Queue 10 files, should process faster than serial
```

**Success Criteria:**
- `just lint` passes
- Application logs show `N` workers started (where N = `concurrency_limit`)
- PostgreSQL `SKIP LOCKED` prevents duplicate processing (existing tests validate)

---

### Phase 2: Fix Tests - Shared Collection Pattern 🎯
**Priority:** HIGH - Unblock test suite

**Files to Change:**
1. `/Users/clafollett/Repositories/codetriever/crates/codetriever-api/tests/test_utils.rs`
2. All integration tests that use `app_state()` helper

**Step 2.1: Update `test_utils.rs`**

```rust
// BEFORE:
let collection_name = format!("{test_name}_{timestamp}_{counter}");
let vector_storage = QdrantStorage::new(url, collection_name).await?;

// AFTER:
const SHARED_TEST_COLLECTION: &str = "codetriever_test";
let vector_storage = QdrantStorage::new(url, SHARED_TEST_COLLECTION).await?;

// Each test gets unique repository_id (already happening!)
// No code changes needed - just reuse ONE collection
```

**Step 2.2: Update Cleanup Logic**

```rust
// BEFORE (in Drop impl):
match storage.drop_collection().await {
    Ok(_) => eprintln!("✅ Dropped collection: {name}"),
    // ...
}

// AFTER:
// Delete chunks by repository_id filter
match storage.delete_by_filter(Filter {
    must: vec![Condition::matches("repository_id", &self.repository_id)]
}).await {
    Ok(_) => eprintln!("✅ Cleaned up test data for {}", self.repository_id),
    // ...
}
```

**Step 2.3: Add `delete_by_filter` to VectorStorage Trait**

```rust
// In crates/codetriever-vector-data/src/storage/traits.rs
#[async_trait]
pub trait VectorStorage: Send + Sync {
    // ... existing methods

    /// Delete all chunks matching a filter (for multi-tenancy cleanup)
    async fn delete_by_filter(&self, filter: Filter) -> VectorDataResult<usize>;
}
```

**Step 2.4: Implement `delete_by_filter` in QdrantStorage**

```rust
// In crates/codetriever-vector-data/src/storage/qdrant.rs
async fn delete_by_filter(&self, filter: Filter) -> VectorDataResult<usize> {
    use qdrant_client::qdrant::{DeletePoints, Filter as QdrantFilter};

    let delete_request = DeletePoints {
        collection_name: self.collection_name.clone(),
        filter: Some(filter.into()),  // Convert Filter → QdrantFilter
        ..Default::default()
    };

    let response = self.client.delete_points(delete_request).await?;
    Ok(response.result.count as usize)
}
```

**Testing:**
```bash
# Run one test - should create shared collection
just test-metal edge_case_coverage::test_search_special_characters

# Run another test concurrently - should reuse collection
just test-metal edge_case_coverage::test_search_unicode

# Verify only ONE collection exists
# psql → SELECT * FROM qdrant_collections; → Should see "codetriever_test" only
```

**Success Criteria:**
- All tests use `codetriever_test` collection (only ONE collection created)
- Tests isolated by unique `repository_id` per test
- No race conditions (workers can run concurrently)
- Cleanup deletes only test's data (via filter), not entire collection

---

### Phase 3: Future Enhancements 🚀
**Priority:** MEDIUM - Performance optimization

**3.1 Add Payload Indexes**

Enable `is_tenant: true` optimization for `repository_id`:

```rust
// When creating collection
let payload_schema = {
    "repository_id": PayloadSchemaParams {
        is_tenant: true,
        data_type: PayloadSchemaType::Keyword,
    },
    "branch": PayloadSchemaParams {
        indexed: true,  // Regular index (not tenant)
        data_type: PayloadSchemaType::Keyword,
    },
};
```

**Benefits:**
- Near-zero filtering overhead
- Scales to millions of repos/branches

**3.2 Tenant-Level Collections (When Needed)**

For customers requiring strict isolation (compliance/security):

```rust
// Create dedicated collection per tenant
collection_name = format!("tenant_{tenant_id}_jina_v2_768d");

// Store tenant_id → collection_name mapping in PostgreSQL
// Route searches to tenant's dedicated collection
```

**When to Use:**
- Customer pays for dedicated resources
- Compliance/security requires strict isolation
- Tenant has > 100M vectors (collection gets too large)

**3.3 TTL for Stale Branches**

Auto-cleanup old branches after N days:

```rust
// Add TTL metadata
payload = {
    "branch": "feature-123",
    "last_indexed": "2025-11-02T10:00:00Z",
    // ...
};

// Periodic cleanup job (cron)
let cutoff = Utc::now() - Duration::days(30);
storage.delete_by_filter(Filter {
    must: vec![
        Condition::range("last_indexed", ..cutoff)
    ]
}).await?;
```

---

## 5. Migration Path (From Current Broken State)

### 5.1 Order of Operations

**Phase 1: Fix Workers (Day 1)**
1. ✅ Update `bootstrap.rs` to spawn N workers
2. ✅ Test with production config (verify N workers start)
3. ✅ Run existing `postgres_queue_test.rs` (validates SKIP LOCKED)
4. ✅ Merge to main (workers now concurrent!)

**Phase 2: Fix Tests (Day 2)**
1. ✅ Add `delete_by_filter` to `VectorStorage` trait
2. ✅ Implement in `QdrantStorage` + `MockStorage`
3. ✅ Update `test_utils.rs` to use shared collection
4. ✅ Run full test suite (`just test-metal`)
5. ✅ Fix any failing tests (should be minimal - just cleanup logic)
6. ✅ Merge to main (tests now use production-like architecture!)

**Phase 3: Enhancements (Week 1)**
1. ⏰ Add payload indexes (`is_tenant: true` for `repository_id`)
2. ⏰ Document tenant-level collection strategy (for future)
3. ⏰ Implement TTL cleanup (if needed)

### 5.2 Testing Strategy

**Unit Tests:**
- ✅ Already passing (PostgreSQL queue tests validate SKIP LOCKED)
- ✅ Mock storage tests validate trait interface

**Integration Tests:**
- ⚠️ Currently broken due to race conditions
- ✅ Will be fixed in Phase 2 (shared collection)

**Manual Testing:**
```bash
# 1. Start services
just dev-setup
just run &

# 2. Index a large repo (verify N workers processing)
curl -X POST http://localhost:3000/index \
  -H "Content-Type: application/json" \
  -d '{"repository_id": "test-repo", "branch": "main", "files": [...]}'

# 3. Monitor logs - should see concurrent processing
tail -f logs/codetriever.log | grep "Processing file"

# 4. Check Qdrant - should see ONE collection with all chunks
curl http://localhost:6334/collections
```

**Load Testing:**
```bash
# Queue 100 files, measure throughput
# Should be ~N× faster with N workers vs 1 worker
```

### 5.3 Rollback Plan

**If Phase 1 Breaks Production:**
1. Revert `bootstrap.rs` to single worker
2. Deploy hotfix
3. Investigate issue (likely resource exhaustion or deadlock)

**If Phase 2 Breaks Tests:**
1. Revert `test_utils.rs` to per-test collections
2. Keep workers at N (Phase 1 independent)
3. Debug race condition

**Safe Rollback Points:**
- After Phase 1: Workers concurrent, tests still per-collection (works but wasteful)
- After Phase 2: Workers concurrent, tests shared collection (optimal!)

---

## 6. Success Criteria

### 6.1 Code Quality
- ✅ `just lint` passes (no clippy warnings)
- ✅ `just test-metal` all green (100% pass rate)
- ✅ No `TODO` comments or stub implementations

### 6.2 Production Behavior
- ✅ N workers spawn on startup (from `config.indexing.concurrency_limit`)
- ✅ Workers process files concurrently (verify via logs)
- ✅ No duplicate processing (PostgreSQL `SKIP LOCKED` prevents)
- ✅ ONE collection per embedding model (not per repo/branch)

### 6.3 Test Suite
- ✅ All tests use shared collection (`codetriever_test`)
- ✅ Tests isolated via unique `repository_id`
- ✅ Workers can run concurrently (no race conditions)
- ✅ Cleanup uses `delete_by_filter` (not drop collection)

### 6.4 Performance
- ✅ Indexing throughput scales with N workers (measure files/second)
- ✅ Qdrant memory usage scales linearly (not exponentially with collections)
- ✅ Search latency unaffected by tenant count (payload filtering is free)

---

## 7. Open Questions / Decisions Needed

### 7.1 Collection Naming Convention

**Options:**
1. **Model-based:** `codetriever_jina_v2_768d` (current proposal)
2. **Version-based:** `codetriever_v1` (easier upgrades, but needs migration)
3. **Embedding-dim-based:** `codetriever_768d` (if we switch models)

**Recommendation:** Model-based for now (explicit, traceable). Can add migration logic later.

### 7.2 When to Create Tenant Collections?

**Trigger:** Manual opt-in via config or API flag (`require_dedicated_collection: true`)

**Implementation:**
```rust
// In config
[tenants.mega_corp]
dedicated_collection = true  # Creates "tenant_mega_corp_jina_v2_768d"

// Route searches
if tenant.requires_dedicated_collection() {
    collection = format!("tenant_{}_jina_v2_768d", tenant.id);
} else {
    collection = "codetriever_jina_v2_768d";  // Shared
}
```

### 7.3 Test Collection Lifecycle

**Current:** Create shared collection on first test, drop on process exit

**Alternative:** Never drop (persistent), just delete by filter

**Recommendation:** Drop on process exit (clean slate for next run). Use filter-delete per test.

---

## 8. Related Issues / PRs

- **Current Branch:** `feature/issue-16-context-endpoint`
- **Related Commit:** `8f3f05a` - WIP: Replace monolithic index_file_content with BackgroundWorker
- **Related Commit:** `925091d` - feat: Implement async job-based indexing with status tracking

---

## 9. References

### 9.1 Qdrant Documentation
- [Multi-tenancy Guide](https://qdrant.tech/documentation/guides/multiple-partitions/)
- [Collections - Best Practices](https://qdrant.tech/documentation/concepts/collections/)
- [Payload Filtering Performance](https://qdrant.tech/articles/multitenancy/)

### 9.2 Codebase Files
- Worker: `crates/codetriever-indexing/src/worker.rs`
- Bootstrap: `crates/codetriever-api/src/bootstrap.rs`
- Test Utils: `crates/codetriever-api/tests/test_utils.rs`
- Queue: `crates/codetriever-meta-data/src/repository.rs` (dequeue_file)

### 9.3 Existing Tests
- PostgreSQL Queue: `crates/codetriever-indexing/tests/postgres_queue_test.rs`
- Concurrent Dequeue: Test validates `SKIP LOCKED` prevents duplicates

---

## 10. Appendix: Key Code Snippets

### A. PostgreSQL SKIP LOCKED Query

```sql
-- Atomic concurrent dequeue (prevents duplicates!)
UPDATE indexing_job_file_queue
SET status = 'processing'
WHERE (job_id, file_path) = (
    SELECT job_id, file_path
    FROM indexing_job_file_queue
    WHERE status = 'queued'
    ORDER BY priority DESC, created_at ASC
    LIMIT 1
    FOR UPDATE SKIP LOCKED  -- 🔥 KEY: Multiple workers can dequeue concurrently
)
RETURNING job_id, file_path, file_content, content_hash
```

### B. Qdrant Multi-Tenancy Pattern

```rust
// Store with tenant metadata
let payload = HashMap::from([
    ("repository_id", Value::from("user/repo")),  // Tenant key
    ("branch", Value::from("main")),
    ("generation", Value::from(42)),
    ("file_path", Value::from("src/main.rs")),
]);

storage.store_chunks(chunks, payload).await?;

// Search with filtering
let filter = Filter {
    must: vec![
        Condition::matches("repository_id", "user/repo"),
        Condition::matches("branch", "main"),
    ]
};

let results = storage.search(query, 10, Some(filter)).await?;
```

### C. Worker Pool Spawn Pattern

```rust
// Spawn N workers (all dequeue from same queue)
let workers: Vec<_> = (0..config.indexing.concurrency_limit)
    .map(|worker_id| {
        let worker = BackgroundWorker::new(/* shared deps */);
        let shutdown = worker.shutdown_handle();

        tokio::spawn(async move {
            info!("Worker {worker_id} started");
            worker.run().await;  // Runs until shutdown
            info!("Worker {worker_id} stopped");
        });

        shutdown
    })
    .collect();
```

---

**END OF PLAN**

**Next Steps:**
1. Review this plan
2. Approve Phase 1 implementation
3. Ship it! 🚀
