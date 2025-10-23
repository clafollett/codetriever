# Configuration Consolidation & Chunking Fix

**Date:** 2025-10-12
**Status:** DRAFT - Awaiting Review
**Priority:** CRITICAL - Data Loss & Performance Issues
**Author:** Marvin (AI) + Cali LaFollett

---

## Executive Summary

Current configuration system has **critical bugs** causing:
1. **99.6% data loss** - Parser creates 8192-token chunks, model truncates to 32 tokens
2. **Duplicate/conflicting settings** - 3 different `batch_size` configs fighting each other
3. **2x performance regression** - Tests running 26s vs 12s in Sept (pool overhead + config mismatch)
4. **Profile complexity** - Too many environment-specific overrides, confusing for contributors

**Impact:** System is NOT indexing code properly, search results are incomplete/incorrect.

---

## Current Problems (Detailed Analysis)

### Problem 1: Parser/Model Token Mismatch

**What's Happening:**
```
Tree-sitter finds function (2000 tokens)
  ↓
Parser: "Too big! Split into chunks ≤ 8192 tokens"
  ↓ Creates chunk: 2000 tokens (fits in limit, no split)
  ↓
Model: "Too big! Truncate to 32 tokens"
  ↓ THROWS AWAY 1968 tokens (98.4% data loss!)
  ↓
Qdrant stores: 32-token fragment of function
```

**Evidence:**
- Config: `indexing.max_chunk_tokens = 8192` (line 417)
- Config: `embedding.model.max_tokens = 32` (line 220)
- Result: Chunks created with ~500-2000 tokens, model sees only first 32

**Root Cause:**
- No validation that `max_chunk_tokens ≤ model.max_tokens`
- Parser and model use different config values
- Model limit not read from HuggingFace config.json

### Problem 2: Configuration Duplication

**Three Competing `batch_size` Configs:**

1. **`embedding.performance.batch_size`** (default: 4 for tests)
   - Purpose: API concurrency batching (multiple user requests)
   - Used by: `DefaultEmbeddingService` (line 112 in service.rs)
   - Issue: Irrelevant for single-user tests, adds confusion

2. **`indexing.embedding_batch_size`** (default: 4 for tests)
   - Purpose: Memory management during indexing
   - Used by: `Indexer` (line 411 in indexer.rs)
   - Issue: Creates 53 sequential batches for 213 chunks

3. **Pool worker batching** (uses `embedding.performance.batch_size`)
   - Purpose: Collect concurrent requests before processing
   - Used by: `model_worker` (line 256 in pool.rs)
   - Issue: 10ms timeout overhead per batch

**Result:** Configs work against each other instead of together.

### Problem 3: Profile Complexity

**Current State:**
- 4 profiles: Development, Staging, Production, Test
- Each profile overrides 30+ settings
- Inconsistent defaults across profiles
- Hard to reason about actual runtime config

**Example Confusion:**
```rust
Profile::Test => 4,           // embedding batch_size
Profile::Test => 4,           // indexing embedding_batch_size
Profile::Test => 8192,        // indexing max_chunk_tokens
Profile::Test => 32,          // embedding max_tokens
Profile::Test => 1,           // pool_size
```

No clear rationale for these numbers, hard to tune.

### Problem 4: Performance Regression

**Sept 1st (12s):**
- Direct model call, no pooling
- Single batch of all chunks
- No database integration

**Current (26s → 16.5s with tuning):**
- Pool overhead (channels, dispatcher, mutex)
- 53 sequential batches with pool_size=1
- Database operations (file state, metadata)
- 10ms × 53 batches = 530ms of timeout overhead alone!

---

## Proposed Solution Architecture

### Core Principle: Single Source of Truth

**Configuration Hierarchy:**
```
HuggingFace model config.json (authoritative limits)
  ↓
Safe defaults in code (conservative, memory-efficient)
  ↓
.env file overrides (explicit user intent)
  ↓
Runtime validation (enforce limits, catch misconfigurations)
```

### Unified Embedding Configuration

**One Model Config to Rule Them All:**

```rust
pub struct EmbeddingModelConfig {
    /// Model ID from HuggingFace
    pub model_id: String,

    /// Max tokens per input (read from HF config, user can override lower)
    /// This is THE authoritative limit - parser MUST respect this
    pub max_tokens: usize,

    /// Embedding dimensions (read from HF config)
    pub dimensions: usize,

    /// Model's actual capability (from config.json)
    pub max_position_embeddings: usize, // e.g., 8192 for JinaBERT
}
```

### Simplified Chunking Strategy (NO OVERLAP)

**Rationale:**
- Tree-sitter provides semantic boundaries (complete functions/classes)
- Each chunk is self-contained with metadata (file_path, start_line, end_line, byte_start, byte_end)
- Can reconstruct full context on-demand using byte offsets
- Overlap adds complexity without proven search quality benefit
- Storage savings: ~20% less data in Qdrant

**Chunk Metadata (Already Implemented):**
```rust
pub struct CodeChunk {
    pub file_path: String,      // Full path for context
    pub content: String,        // The actual code text
    pub start_line: usize,      // 1-indexed line number (human-readable)
    pub end_line: usize,        // 1-indexed line number (human-readable)
    pub byte_start: usize,      // 0-indexed byte offset (for file slicing)
    pub byte_end: usize,        // 0-indexed byte offset (for file slicing)
    pub kind: Option<String>,   // "function", "class", "impl", etc.
    pub language: String,       // "rust", "python", etc.
    pub name: Option<String>,   // Function/class name if extracted
    pub token_count: Option<usize>, // Actual tokens in chunk
    pub embedding: Option<Vec<f32>>, // 768-dim vector
}
```

**Splitting Large Functions (Without Overlap):**
```
Tree-sitter finds: function "process_data" (2000 tokens, lines 100-400, bytes 5000-25000)
  ↓
Parser checks: 2000 > max_tokens (512)
  ↓
Split into token-based chunks:
  - Chunk 1: 512 tokens, lines 100-220, bytes 5000-11500, "process_data_part1"
  - Chunk 2: 512 tokens, lines 220-340, bytes 11500-18000, "process_data_part2"
  - Chunk 3: 488 tokens, lines 340-400, bytes 18000-25000, "process_data_part3"
  ↓
Each chunk independently searchable
  ↓
On match: Use byte_start/byte_end to extract full function from file if needed
```

### Separate Batch Sizes by Purpose

**Clear Naming & Responsibilities:**

```rust
/// GPU/Inference batching - controls memory usage during model forward pass
/// Smaller = less memory, faster for small loads
/// Larger = more memory, better GPU utilization for large loads
pub indexer_batch_size: usize,  // Default: 64 (balance memory/speed)

/// API request batching - only relevant for multi-user API server
/// Groups multiple concurrent user queries before processing
pub search_batch_size: usize,   // Default: 8 (typical concurrent users)

/// Pool configuration
pub pool_size: usize,            // Default: 2 (balance memory/parallelism)
pub batch_timeout_ms: u64,       // Default: 10ms
```

### Remove Profile Enum Entirely

**Before (Complex):**
```rust
unwrap_or(match profile {
    Profile::Development => 16,
    Profile::Staging => 32,
    Profile::Production => 64,
    Profile::Test => 4,
})
```

**After (Simple):**
```rust
unwrap_or(DEFAULT_INDEXER_BATCH_SIZE)  // Just use the safe default!
```

**All environment-specific config goes in `.env` files:**
- `.env.sample` - Documents all vars with safe defaults
- `.env` - Local development (gitignored)
- User can create `.env.prod`, `.env.staging` if needed

---

## Implementation Plan

### Phase 1: Read Model Config from HuggingFace (FOUNDATION)

**Goal:** Get authoritative limits from model's config.json

**Changes:**
1. Update `EmbeddingModel::ensure_model_loaded()` to parse config.json
2. Extract `max_position_embeddings` and store in `ModelConfig`
3. Use this as validation ceiling for all token-related configs

**Files:**
- `crates/codetriever-embeddings/src/embedding/model.rs`
- `crates/codetriever-config/src/lib.rs`

**Validation:**
```rust
if user_max_tokens > model_max_position_embeddings {
    error!("Config max_tokens ({}) exceeds model limit ({})", ...);
}
```

### Phase 2: Remove Profile Enum

**Goal:** Eliminate profile-based branching, use safe defaults everywhere

**Changes:**
1. Remove `Profile` enum from `codetriever-config`
2. Replace all `match profile` blocks with constant defaults
3. Keep env var overrides for everything
4. Update `.env.sample` with comprehensive documentation

**Files:**
- `crates/codetriever-config/src/lib.rs` (major refactor)
- `crates/codetriever-config/src/profile.rs` (DELETE)
- `.env.sample` (expand with all defaults documented)

**Default Values (Safe for any environment):**

```rust
// Embedding Model Configuration
const DEFAULT_MODEL_ID: &str = "jinaai/jina-embeddings-v2-base-code";
const DEFAULT_MAX_TOKENS: usize = 512;        // Conservative for memory
const DEFAULT_DIMENSIONS: usize = 768;        // JinaBERT v2 standard

// Performance Configuration
const DEFAULT_INDEXER_BATCH_SIZE: usize = 64; // Balance memory/speed
const DEFAULT_SEARCH_BATCH_SIZE: usize = 8;   // Typical concurrent API users
const DEFAULT_POOL_SIZE: usize = 2;           // Minimum for parallelism
const DEFAULT_BATCH_TIMEOUT_MS: u64 = 10;     // Low latency

// Parsing Configuration
const DEFAULT_MAX_CHUNK_TOKENS: usize = 512;  // Matches model max_tokens
const DEFAULT_SPLIT_LARGE_UNITS: bool = true; // Always split large functions
// NO OVERLAP - rely on semantic boundaries and byte offsets for context

// Database (safe local defaults)
const DEFAULT_DB_HOST: &str = "localhost";
const DEFAULT_DB_PORT: u16 = 5432;
const DEFAULT_DB_MAX_CONNECTIONS: u32 = 5;    // Conservative

// API Server
const DEFAULT_API_HOST: &str = "127.0.0.1";   // Localhost only for security
const DEFAULT_API_PORT: u16 = 3000;
```

### Phase 3: Consolidate Batch Size Configs

**Goal:** Two clearly-named batch sizes for different purposes

**Changes:**

1. **Rename for clarity:**
   ```rust
   // OLD (confusing)
   embedding.performance.batch_size
   indexing.embedding_batch_size

   // NEW (clear purpose)
   embedding.indexer_batch_size   // GPU memory management
   embedding.search_batch_size    // API concurrency
   ```

2. **Update environment variables:**
   ```bash
   # OLD
   CODETRIEVER_EMBEDDING_BATCH_SIZE
   CODETRIEVER_INDEXING_EMBEDDING_BATCH_SIZE

   # NEW
   CODETRIEVER_EMBEDDING_INDEXER_BATCH_SIZE    # Default: 64
   CODETRIEVER_EMBEDDING_SEARCH_BATCH_SIZE     # Default: 8
   ```

3. **Remove service-level batching in `DefaultEmbeddingService`:**
   - Delete lines 112-131 in service.rs (the for loop)
   - Service just passes all texts directly to provider
   - Let indexer handle batching (it already does!)
   - Search only sends 1-5 queries at a time anyway

**Files:**
- `crates/codetriever-config/src/lib.rs`
- `crates/codetriever-embeddings/src/embedding/service.rs`
- `crates/codetriever-indexing/src/indexing/indexer.rs`
- `.env.sample`

### Phase 4: Fix Parser/Model Alignment & Remove Overlap

**Goal:** Ensure chunks created ≤ model max_tokens, no truncation, simplified chunking

**Changes:**

1. **Single `max_tokens` used everywhere:**
   ```bash
   # .env.sample
   # Maximum tokens per chunk (must be ≤ model's max_position_embeddings)
   # Smaller = faster, less memory; Larger = more context per chunk
   # Default: 512 (conservative, works on most hardware)
   CODETRIEVER_MAX_TOKENS=512
   ```

2. **Parser simplified to 3 parameters (remove overlap):**
   ```rust
   CodeParser::new(
       tokenizer,              // For accurate token counting
       true,                   // split_large_units (always true)
       config.max_tokens,      // ONE source of truth - no separate chunk vs model limits
   )
   ```

3. **Update `split_by_tokens()` to remove overlap logic:**
   - Delete lines 260-268 in code_parser.rs (overlap calculation)
   - Simple sequential splitting: chunk_start += max_tokens
   - Each chunk stands alone with accurate byte_start/byte_end metadata

4. **Verify byte offset accuracy during splitting:**
   - Token-based splits must maintain accurate byte_start/byte_end
   - Use tokenizer.decode() to get exact text boundaries
   - Store byte offsets for precise file reconstruction

**Files:**
- `crates/codetriever-config/src/lib.rs` (remove chunk_overlap_tokens)
- `crates/codetriever-parsing/src/parsing/code_parser.rs` (simplify splitting)
- `crates/codetriever-indexing/tests/test_utils.rs` (update test helpers)

### Phase 5: Add Comprehensive Validation

**Goal:** Catch misconfigurations at startup, fail fast with clear errors

**Validation Rules:**
```rust
impl ApplicationConfig {
    pub fn validate(&self) -> Result<()> {
        // Read model's actual limits from HF
        let model_max = self.embedding.model.max_position_embeddings;

        // Ensure user config doesn't exceed model
        if self.embedding.max_tokens > model_max {
            return Err(ConfigError::InvalidValue {
                field: "max_tokens",
                value: self.embedding.max_tokens,
                reason: format!("Exceeds model limit of {model_max}"),
            });
        }

        // Ensure max_tokens is consistent across parser and model
        if self.indexing.max_chunk_tokens > self.embedding.max_tokens {
            return Err(ConfigError::InvalidValue {
                field: "max_chunk_tokens",
                reason: format!(
                    "Cannot exceed embedding model max_tokens ({})",
                    self.embedding.max_tokens
                ),
            });
        }

        // Ensure dimensions match
        if self.embedding.dimensions != self.vector_storage.dimension {
            return Err(ConfigError::DimensionMismatch { ... });
        }

        // Memory estimation
        let estimated_mb = self.estimate_memory_usage();
        if let Some(limit) = self.memory_limit_mb {
            if estimated_mb > limit {
                return Err(ConfigError::MemoryExceeded { ... });
            }
        }

        Ok(())
    }
}
```

### Phase 6: Update .env.sample Documentation

**Goal:** Single source of truth for all configuration options

**Structure:**
```bash
# =============================================================================
# CODETRIEVER CONFIGURATION
# =============================================================================
# All settings have safe defaults. Only override what you need to change.
# Configuration hierarchy: HuggingFace model config → defaults → .env overrides

# -----------------------------------------------------------------------------
# EMBEDDING MODEL CONFIGURATION
# -----------------------------------------------------------------------------

# Model identifier from HuggingFace
# Default: jinaai/jina-embeddings-v2-base-code
CODETRIEVER_EMBEDDING_MODEL=jinaai/jina-embeddings-v2-base-code

# Maximum tokens per chunk (must be ≤ model's max_position_embeddings)
# JinaBERT v2 supports up to 8192, but smaller is faster and uses less memory
# Default: 512 (good balance of context and performance)
# Range: 128-8192 (validated against model limits at runtime)
CODETRIEVER_MAX_TOKENS=512

# Embedding vector dimensions (must match model output)
# JinaBERT v2 produces 768-dimensional vectors
# Default: 768 (read from model config)
CODETRIEVER_EMBEDDING_DIMENSIONS=768

# -----------------------------------------------------------------------------
# BATCHING & PERFORMANCE CONFIGURATION
# -----------------------------------------------------------------------------

# Indexer batch size - controls GPU memory usage during indexing
# How many chunks processed in parallel during model forward pass
# Smaller = less memory, Larger = better GPU utilization
# Default: 64 (balance for most systems)
# Memory usage ≈ batch_size × max_tokens × dimensions × 4 bytes × 12 layers
CODETRIEVER_INDEXER_BATCH_SIZE=64

# Search batch size - how many user queries batched together (API only)
# Only relevant for multi-user API scenarios
# Default: 8 (typical concurrent users)
CODETRIEVER_SEARCH_BATCH_SIZE=8

# Model pool size - number of model instances for parallel inference
# More models = more parallelism but uses more memory (2GB per model)
# Default: 2 (minimum for parallelism)
CODETRIEVER_POOL_SIZE=2

# Batch collection timeout in milliseconds
# How long to wait collecting requests before processing batch
# Default: 10ms (low latency)
CODETRIEVER_BATCH_TIMEOUT_MS=10

# -----------------------------------------------------------------------------
# PARSING & CHUNKING CONFIGURATION
# -----------------------------------------------------------------------------

# Whether to split large code units (functions/classes) that exceed max_tokens
# If false, large functions stored as single chunks (will be truncated by model!)
# Default: true (always split for accuracy)
# NOTE: Overlap removed - we rely on semantic boundaries + byte offsets for context
CODETRIEVER_SPLIT_LARGE_UNITS=true

# -----------------------------------------------------------------------------
# DATABASE CONFIGURATION
# -----------------------------------------------------------------------------

DB_HOST=localhost
DB_PORT=5432
DB_NAME=codetriever
DB_USER=codetriever
DB_PASSWORD=localdev123
DB_SSLMODE=disable

# Connection pool settings
CODETRIEVER_DATABASE_MAX_CONNECTIONS=5
CODETRIEVER_DATABASE_MIN_CONNECTIONS=2
CODETRIEVER_DATABASE_TIMEOUT_SECONDS=30

# -----------------------------------------------------------------------------
# VECTOR DATABASE (QDRANT) CONFIGURATION
# -----------------------------------------------------------------------------

QDRANT_URL=http://localhost:6334
# QDRANT_API_KEY=your_api_key  # Optional, for production

CODETRIEVER_VECTOR_STORAGE_COLLECTION=codetriever
CODETRIEVER_VECTOR_STORAGE_TIMEOUT_SECONDS=30

# -----------------------------------------------------------------------------
# API SERVER CONFIGURATION
# -----------------------------------------------------------------------------

CODETRIEVER_API_HOST=127.0.0.1  # Localhost only for security
CODETRIEVER_API_PORT=3000
CODETRIEVER_API_TIMEOUT_SECONDS=60
CODETRIEVER_API_ENABLE_CORS=true
CODETRIEVER_API_ENABLE_DOCS=true

# -----------------------------------------------------------------------------
# ADVANCED / OPTIONAL SETTINGS
# -----------------------------------------------------------------------------

# GPU acceleration (auto-detected by default)
# CODETRIEVER_USE_GPU=true
# CODETRIEVER_GPU_DEVICE=metal  # metal, cuda:0, mps

# Model caching
# CODETRIEVER_CACHE_DIR=~/.cache/codetriever
# CODETRIEVER_CACHE_ENABLED=true

# Memory limit (MB) - prevents OOM
# CODETRIEVER_MEMORY_LIMIT_MB=8192

# Telemetry
# CODETRIEVER_TELEMETRY_ENABLED=false
# CODETRIEVER_TRACING_LEVEL=info  # trace, debug, info, warn, error
```

### Phase 7: Testing Strategy

**Verification Tests:**

1. **Chunk size validation test:**
   ```rust
   #[test]
   fn test_chunks_never_exceed_model_max_tokens() {
       let config = ApplicationConfig::from_env();
       let parser = create_parser_with_tokenizer(&config);

       // Parse a large file
       let chunks = parser.parse(LARGE_RUST_FILE, "rust", "test.rs");

       // Verify EVERY chunk respects limit
       for chunk in chunks {
           assert!(chunk.token_count.unwrap() <= config.max_tokens,
               "Chunk has {} tokens, exceeds max_tokens {}",
               chunk.token_count.unwrap(), config.max_tokens);
       }
   }
   ```

2. **Byte offset accuracy test:**
   ```rust
   #[test]
   fn test_byte_offsets_allow_exact_reconstruction() {
       let config = ApplicationConfig::from_env();
       let parser = create_parser_with_tokenizer(&config);
       let source_file = read_to_string("test.rs").unwrap();

       let chunks = parser.parse(&source_file, "rust", "test.rs").unwrap();

       // Verify each chunk's byte offsets extract correct content
       for chunk in chunks {
           let extracted = &source_file[chunk.byte_start..chunk.byte_end];
           assert_eq!(extracted, chunk.content,
               "Byte offsets don't match chunk content!");
       }
   }
   ```

3. **No truncation test:**
   ```rust
   #[test]
   fn test_model_accepts_chunks_without_truncation() {
       let config = ApplicationConfig::from_env();
       let parser = create_parser_with_tokenizer(&config);
       let model = EmbeddingModel::new(config);

       // Create max-size chunk
       let chunk_text = create_text_with_exact_tokens(config.max_tokens);

       // Model should accept without truncation
       let embeddings = model.embed(vec![chunk_text]).await.unwrap();
       assert_eq!(embeddings.len(), 1);
       // No warning logs about truncation
   }
   ```

3. **Performance benchmark:**
   ```rust
   #[test]
   fn test_indexing_performance_acceptable() {
       let start = Instant::now();

       // Index mini-redis (213 chunks)
       let result = indexer.index_directory("test-repos/mini-redis/src").await;

       let elapsed = start.elapsed().as_secs();
       assert!(elapsed < 20, "Indexing took {elapsed}s, expected <20s");
       assert_eq!(result.chunks_created, 213);
   }
   ```

---

## Migration Steps

### Step 1: Create Backup
- Stash/commit current changes
- Document current config values in migration guide

### Step 2: Implement in Order (Critical Path)
1. Phase 1: Read HF model config (REQUIRED for validation)
2. Phase 4: Fix parser alignment (CRITICAL - fixes data loss)
3. Phase 5: Add validation (SAFETY)
4. Phase 2: Remove profiles (CLEANUP)
5. Phase 3: Rename batch configs (CLARITY)
6. Phase 6: Update .env.sample (DOCUMENTATION)

### Step 3: Verification
- Run full test suite
- Verify performance (target: 12-15s for manual_search_test)
- Check memory usage (target: <10GB for 213 chunks)
- Inspect indexed content in Qdrant (verify full functions stored)

---

## Success Criteria

✅ **Correctness:**
- Zero data loss - chunks never truncated by model
- All chunks ≤ model.max_tokens (parser and model aligned)
- Byte offsets accurate for context reconstruction
- Semantic boundaries preserved (complete functions/classes)

✅ **Performance:**
- Manual search test: ≤15s (currently 26s)
- Memory usage: <10GB peak (currently 7-36GB depending on config)
- All tests passing

✅ **Clarity:**
- Contributors can understand config in <5 minutes
- .env.sample documents everything
- No Profile enum complexity
- Clear naming (indexer_batch_size vs search_batch_size)

✅ **Safety:**
- Runtime validation catches misconfigurations
- Fails fast with clear error messages
- Impossible to set max_chunk_tokens > model limit

---

## Decisions Made

1. **Indexer vs Search batch sizes:**
   - ✅ **CONFIRMED:** Separate `INDEXER_BATCH_SIZE` and `SEARCH_BATCH_SIZE`
   - Different use cases, different optimal values

2. **Chunk overlap:**
   - ✅ **REMOVED:** Rely on semantic boundaries + byte offsets instead
   - Simpler code, no storage duplication, cleaner search results

3. **Auto-calculation of batch size:**
   - ✅ **GitHub issue for future:** Heuristics for optimal batch sizing
   - **For now:** Manual config via env vars with safe defaults

4. **Model validation:**
   - ✅ **Fail at startup** if config exceeds model limits
   - Clear error messages, no auto-adjust (explicit is better than implicit)

5. **Profile removal:**
   - ✅ **DELETE Profile enum entirely**
   - Safe defaults for all environments, env vars for overrides

---

## Risks & Mitigation

**Risk 1: Breaking Changes**
- Mitigation: Comprehensive migration guide, deprecation warnings

**Risk 2: Performance Regressions**
- Mitigation: Benchmark suite, comparison with Sept 1st baseline

**Risk 3: Config Migration Pain**
- Mitigation: Auto-migration script, clear .env.sample examples

**Risk 4: Missing Edge Cases**
- Mitigation: Extensive testing with various model configs, chunk sizes

---

## Next Steps

1. **Cali reviews this plan** - iterate until approved
2. **Compact context** - fresh start for implementation
3. **Phase-by-phase implementation** - systematic, tested at each step
4. **Final validation** - prove system works as intended

---

## Notes & Context

- Current test results: 27/33 passing (6 content_indexing failures FIXED!)
- Performance: 26s → 16.5s with pool_size=3 (still 37% slower than Sept 1st)
- Memory: 7GB (optimal) to 36GB (pathological) depending on batch_size
- Root cause: Config fragmentation + no model limit validation
