# Code Review: Configuration Consolidation & Chunking Fix Planning Document

**Reviewer:** Marvin (Code Reviewer Agent)
**Date:** 2025-10-12
**Document Reviewed:** `/docs/plans/2025-10-12-config-consolidation-and-chunking-fix.md`
**Review Type:** Architecture & Implementation Plan Review
**Priority:** CRITICAL

---

## Review Summary

**Status: CHANGES REQUESTED**

This is a well-researched and comprehensive planning document that correctly identifies critical bugs in the system. The analysis is accurate, the proposed architecture is sound, and the phased approach is methodical. However, there are several technical gaps, implementation risks, and missing validations that must be addressed before proceeding with implementation.

**Overall Assessment:**
- **Completeness:** 85% - Missing critical validation steps and rollback strategy
- **Risk Assessment:** 80% - Identifies major risks but underestimates migration complexity
- **Technical Accuracy:** 90% - Architecture is sound, but some implementation details need clarification
- **Success Criteria:** 75% - Measurable but lacks automated validation gates

---

## Critical Findings

### CRITICAL-1: Parser/Model Token Mismatch - CONFIRMED

**Status:** ACCURATE - This is indeed a critical data loss bug

**Evidence from Codebase:**
```rust
// config/src/lib.rs:45-46
const DEFAULT_MAX_CHUNK_TOKENS: usize = 512;  // Parser limit
const DEFAULT_MAX_TOKENS: usize = 512;        // Model limit

// BUT in .env.sample:59-60 (commented out but dangerous):
# CODETRIEVER_INDEXING_MAX_CHUNK_TOKENS=512
# CODETRIEVER_INDEXING_CHUNK_OVERLAP_TOKENS=128
```

**Current State Analysis:**
- The config system HAS been partially consolidated (seen in `lib.rs`)
- Profile enum still exists but is deprecated (lines 13-27)
- `overlap_tokens` still exists in parser (line 82 of `code_parser.rs`)
- Model DOES read `max_position_embeddings` from HuggingFace (lines 332-344 of `model.rs`)

**Gap in Plan:** The document states this bug exists, but I found that:
1. The model ALREADY extracts `max_position_embeddings` from config.json (implemented)
2. The model ALREADY validates user config doesn't exceed limits (lines 352-357)
3. The parser ALREADY has tokenizer integration for accurate counting

**Recommendation:** Update Phase 1 to reflect that much of this work is ALREADY DONE. Focus on removing the duplicate configs that remain.

---

### CRITICAL-2: Overlap Removal May Break Existing Search Results

**Issue Category:** MAJOR - Risk Assessment Gap

**Concern:** The plan proposes removing chunk overlap entirely (Phase 4) with the rationale:
> "Overlap adds complexity without proven search quality benefit"

**Analysis:**
- Overlap exists in THREE places:
  1. `code_parser.rs:82` - `overlap_tokens: usize` field
  2. `code_parser.rs:261` - Token-based overlap calculation
  3. `code_parser.rs:461-477` - Heuristic parsing overlap

**Missing from Plan:**
1. No A/B testing mentioned to validate search quality impact
2. No migration strategy for existing indexed data
3. No fallback mechanism if search quality degrades
4. No metrics defined to measure "search quality benefit"

**Risk:** Users with existing indexed codebases will see different search results post-migration. If search quality degrades, there's no rollback plan.

**Recommendation:**
```markdown
## Phase 4.5: Validate Overlap Removal (NEW PHASE)

Before committing to overlap removal:
1. Index test repos TWICE (with/without overlap)
2. Run 100 semantic search queries on both indexes
3. Compare results using metrics:
   - Recall@5, Recall@10
   - Mean Reciprocal Rank (MRR)
   - User-validated relevance scores
4. If overlap removal degrades results by >5%, keep overlap but optimize it
5. Document findings in GitHub issue for transparency
```

---

### CRITICAL-3: No Database Migration Strategy

**Issue Category:** MAJOR - Missing Implementation Detail

**Problem:** The plan focuses on config changes but ignores the database impact.

**Questions:**
1. What happens to existing Qdrant collections with old chunk sizes?
2. Do we need a data migration tool to re-index with new config?
3. How do we handle version conflicts between old/new chunks?

**Evidence:** The plan mentions "reset-dbs" command (line 633) but doesn't integrate it into the migration flow.

**Recommendation:**
```markdown
## Step 2.5: Database Migration Strategy

Before implementing config changes:
1. Add version metadata to Qdrant collections (schema version field)
2. Create migration tool: `just migrate-chunks --from-version=0.1 --to-version=0.2`
3. Support side-by-side operation: old and new collections during transition
4. Add validation: detect schema mismatch and warn users
5. Document migration path in MIGRATION.md
```

---

## Major Findings

### MAJOR-1: Batch Size Consolidation is Confusing

**Issue Category:** MAJOR - Naming & Architecture

**Current State (from codebase):**
```rust
// config/src/lib.rs
pub struct PerformanceConfig {
    pub batch_size: usize,          // Line 180 - Used by embedding service
    pub pool_size: usize,            // Line 186
    pub batch_timeout_ms: u64,       // Line 191
}

// config/src/lib.rs
pub struct IndexingConfig {
    pub embedding_batch_size: usize, // Line 456 - Used by indexer
}
```

**Plan Proposes (Phase 3):**
```rust
pub indexer_batch_size: usize,  // GPU batching
pub search_batch_size: usize,   // API batching
```

**Problem:** The plan doesn't explain:
1. How does `indexer_batch_size` differ from existing `embedding_batch_size`?
2. Why is `search_batch_size` in `EmbeddingConfig` if it's for API layer?
3. What happens to `PerformanceConfig.batch_size`?

**Architecture Flaw:** Batching concerns are being mixed across layers:
- Indexer layer (how many chunks to process at once)
- Embedding layer (how many texts to encode in parallel)
- Pool layer (how many requests to collect before processing)
- API layer (how many user queries to batch)

**Recommendation:**
```rust
// Separate batching by architectural layer
pub struct IndexingConfig {
    pub chunk_batch_size: usize,     // How many chunks to process at once
}

pub struct EmbeddingConfig {
    pub model_batch_size: usize,     // How many texts to encode in parallel (GPU)
}

pub struct PoolConfig {
    pub request_batch_size: usize,   // How many requests to collect
    pub batch_timeout_ms: u64,
}

pub struct ApiConfig {
    pub query_batch_size: usize,     // Concurrent user queries (if implemented)
}
```

**Rationale:** Each layer controls its own batching independently. No confusion about purpose.

---

### MAJOR-2: Profile Enum Removal - Incomplete Analysis

**Issue Category:** MAJOR - Breaking Change Impact

**Current State:**
- Profile enum is DEPRECATED (line 13-27 of `lib.rs`)
- But 18 files still reference it (from grep results)
- Tests still use `with_profile()` methods

**Plan States (Phase 2):**
> "Replace all `match profile` blocks with constant defaults"

**Gap:** The plan doesn't address:
1. Test refactoring effort (how many tests break?)
2. External consumers (are there any CLI tools or scripts using profiles?)
3. Backward compatibility period (do we support both temporarily?)

**Evidence from codebase:**
```rust
// config/src/lib.rs:1041 - Test still uses Profile
let mut config = ApplicationConfig::with_profile(Profile::Test);

// Multiple test files use Profile::Test
```

**Recommendation:**
1. Audit all 18 files using Profile enum
2. Create migration script to automate test updates
3. Add deprecation warnings in v0.2.0 (CURRENT VERSION)
4. Remove Profile enum entirely in v0.3.0 (next version)
5. Document breaking change in CHANGELOG.md

---

### MAJOR-3: Success Criteria Lack Automated Validation

**Issue Category:** MAJOR - Quality Assurance Gap

**Plan States (lines 641-664):**
```markdown
## Success Criteria

✅ **Correctness:**
- Zero data loss - chunks never truncated by model
- All chunks ≤ model.max_tokens (parser and model aligned)
```

**Problem:** These are assertions, not automated tests.

**Missing:**
1. No CI/CD integration tests
2. No pre-commit hooks to validate config consistency
3. No automated smoke tests for "zero data loss"

**Recommendation:**
```rust
// Add to Phase 7: Automated Validation Suite

#[test]
fn test_config_validation_in_ci() {
    let config = ApplicationConfig::from_env();

    // MUST pass in CI - blocking test
    assert!(config.validate().is_ok(), "Config validation failed");

    // Cross-validation
    assert!(
        config.indexing.max_chunk_tokens <= config.embedding.model.max_tokens,
        "Indexing chunk size exceeds model capacity"
    );

    assert_eq!(
        config.embedding.model.dimensions,
        config.vector_storage.vector_dimension,
        "Dimension mismatch between embedding and vector storage"
    );
}

#[test]
fn test_zero_data_loss_guarantee() {
    let config = ApplicationConfig::from_env();
    let parser = CodeParser::new(/* ... */);
    let model = EmbeddingModel::new(/* ... */);

    // Parse large file
    let chunks = parser.parse(LARGE_TEST_FILE, "rust", "test.rs").unwrap();

    // Verify NO chunk exceeds model capacity
    for chunk in chunks {
        let token_count = chunk.token_count.expect("Token count missing");
        assert!(
            token_count <= config.embedding.model.max_tokens,
            "CRITICAL: Chunk with {} tokens exceeds model max of {}",
            token_count,
            config.embedding.model.max_tokens
        );
    }
}
```

**Integration:** Add these to `just check` command so they run on every commit.

---

## Minor Findings

### MINOR-1: Phase Ordering Could Be Optimized

**Current Order:**
1. Read HF config
2. Remove Profile
3. Consolidate batch sizes
4. Fix parser alignment
5. Add validation
6. Update .env.sample

**Recommended Order:**
1. Read HF config (FOUNDATION)
2. Add validation (SAFETY NET - catch issues early)
3. Fix parser alignment (CRITICAL BUG FIX)
4. Remove Profile (CLEANUP)
5. Consolidate batch sizes (CLARITY)
6. Update .env.sample (DOCUMENTATION)

**Rationale:** Add validation BEFORE making changes so we can detect regressions immediately.

---

### MINOR-2: Memory Estimation Formula Needs Validation

**Issue:** Plan references memory estimation (lines 407-412, 923-978) but the formula is untested.

**Current Formula (from `lib.rs:939-978`):**
```rust
let total_memory = base_memory        // 100 MB
    + embedding_memory                // 2048 MB (for base models)
    + vector_memory                   // (dimensions × 4 × batch_size) / 1MB
    + db_memory                       // connections × 2 MB
    + indexing_memory;                // concurrency × 50 MB
```

**Problems:**
1. No validation against actual memory usage (should use system profiler)
2. Magic numbers (why 50 MB per concurrent task?)
3. Doesn't account for tokenizer memory (~200MB for JinaBERT)

**Recommendation:**
```rust
#[test]
fn test_memory_estimation_accuracy() {
    let config = ApplicationConfig::from_env();
    let estimated = config.estimate_memory_usage_mb();

    // Run actual indexing and measure with system profiler
    let actual = measure_actual_memory_during_indexing(&config);

    // Estimation should be within 20% of actual
    let error_margin = 0.20;
    let diff = (actual as f64 - estimated as f64).abs() / actual as f64;
    assert!(
        diff < error_margin,
        "Memory estimation off by {:.1}% (estimated: {} MB, actual: {} MB)",
        diff * 100.0,
        estimated,
        actual
    );
}
```

---

### MINOR-3: Chunk Overlap Environment Variable Name is Wrong

**Issue:** `.env.sample:59` has:
```bash
# CODETRIEVER_INDEXING_CHUNK_OVERLAP_TOKENS=128
```

But this variable doesn't exist in the config loading code (checked `lib.rs`).

**Evidence:**
- `IndexingConfig::from_env()` (lines 459-489) has NO overlap parsing
- Parser creates overlap internally (hardcoded or from constructor)

**Impact:** Users setting this env var will be confused when it doesn't work.

**Recommendation:** Either:
1. Remove this from `.env.sample` (if overlap removal proceeds)
2. Or implement proper env var loading if keeping overlap

---

## Suggestions

### SUGGESTION-1: Add Telemetry for Config Changes

The plan doesn't mention observability. Consider:
```rust
impl ApplicationConfig {
    pub fn log_effective_config(&self) {
        info!("Effective Configuration:");
        info!("  Model: {}", self.embedding.model.id);
        info!("  Max Tokens: {}", self.embedding.model.max_tokens);
        info!("  Model Limit: {:?}", self.embedding.model.max_position_embeddings);
        info!("  Batch Size (Indexer): {}", self.indexing.embedding_batch_size);
        info!("  Batch Size (Embedding): {}", self.embedding.performance.batch_size);

        // Warn about misconfigurations
        if self.indexing.max_chunk_tokens > self.embedding.model.max_tokens {
            warn!("MISCONFIGURATION: Chunks may be truncated!");
        }
    }
}
```

Call this at startup to help users debug config issues.

---

### SUGGESTION-2: Create Visual Config Architecture Diagram

The plan would benefit from a diagram showing:
```
┌─────────────────────────────────────────────────────────┐
│ HuggingFace Model Config (max_position_embeddings=8192) │
└────────────────────────┬────────────────────────────────┘
                         │ (authoritative limit)
                         ↓
┌─────────────────────────────────────────────────────────┐
│ ApplicationConfig                                        │
│ ┌─────────────────┐  ┌──────────────┐  ┌─────────────┐│
│ │ max_tokens: 512 │→ │ Parser       │→ │ Chunks      ││
│ └─────────────────┘  │ max: 512     │  │ ≤512 tokens ││
│         ↓            └──────────────┘  └─────────────┘│
│ ┌─────────────────┐                                    │
│ │ Model           │                                    │
│ │ accepts: 512    │                                    │
│ └─────────────────┘                                    │
└─────────────────────────────────────────────────────────┘
```

This would clarify the data flow and help reviewers understand the fix.

---

### SUGGESTION-3: Add Health Check Endpoint

After implementing config consolidation, add:
```rust
// GET /api/v1/health/config
pub async fn config_health_check() -> Json<ConfigHealth> {
    let config = ApplicationConfig::from_env();
    let validation = config.validate();

    Json(ConfigHealth {
        valid: validation.is_ok(),
        errors: validation.err().map(|e| e.to_string()),
        warnings: check_config_warnings(&config),
        effective_config: ConfigSummary {
            model_id: config.embedding.model.id,
            max_tokens: config.embedding.model.max_tokens,
            chunk_size: config.indexing.max_chunk_tokens,
            dimensions: config.embedding.model.dimensions,
        }
    })
}
```

This helps operators validate config in production.

---

## Recommendations

### Immediate Actions (Before Implementation)

1. **Update Phase 1** to acknowledge existing HuggingFace integration
   - Most of this is already implemented
   - Focus on exposing validation to other modules

2. **Add Phase 4.5** for overlap removal validation
   - Requires A/B testing with metrics
   - Must not proceed blindly without evidence

3. **Add database migration strategy**
   - Version metadata in Qdrant collections
   - Migration tooling for existing data
   - Rollback plan if needed

4. **Refine batch size naming**
   - Separate by architectural layer
   - Document purpose of each batch size clearly
   - Show data flow diagram

5. **Audit Profile enum usage**
   - Count breaking changes
   - Create automated migration script
   - Plan deprecation timeline

6. **Add automated validation tests**
   - Config consistency checks
   - Zero data loss guarantee tests
   - Memory estimation validation
   - Integrate into CI/CD

### Implementation Order Adjustments

```markdown
## Revised Critical Path

1. Phase 5: Add Comprehensive Validation (SAFETY FIRST)
2. Phase 1: Expose HF Model Config (FOUNDATION)
3. Phase 4: Fix Parser Alignment (CRITICAL BUG)
4. Phase 4.5: Validate Overlap Removal (NEW - EVIDENCE NEEDED)
5. Phase 2: Remove Profile Enum (CLEANUP)
6. Phase 3: Consolidate Batch Sizes (CLARITY)
7. Phase 6: Update .env.sample (DOCUMENTATION)
8. Phase 7: Automated Testing (VERIFICATION)
```

**Rationale:** Validation first prevents regressions. Bug fix before cleanup.

---

### Pre-Implementation Checklist

- [ ] Audit all 18 files using Profile enum
- [ ] Create database migration strategy document
- [ ] Design A/B testing framework for overlap removal
- [ ] Define measurable success metrics (Recall@5, MRR, etc.)
- [ ] Create visual architecture diagrams
- [ ] Write automated validation tests
- [ ] Document rollback procedures
- [ ] Update CHANGELOG.md with breaking changes
- [ ] Add memory profiling to test suite
- [ ] Verify env var loading for all config fields

---

## Risk Assessment Update

### Critical Risks (Plan Identified)

| Risk | Plan Mitigation | Review Assessment | Additional Mitigation |
|------|----------------|-------------------|----------------------|
| Breaking Changes | Migration guide | INSUFFICIENT | Add deprecation period, automated migration script |
| Performance Regressions | Benchmark suite | GOOD | Add continuous performance monitoring |
| Config Migration Pain | .env.sample examples | INSUFFICIENT | Add `codetriever config validate` CLI command |
| Missing Edge Cases | Extensive testing | GOOD | Add property-based testing for config validation |

### Critical Risks (Plan Missed)

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Search Quality Degradation | HIGH | MEDIUM | A/B testing before committing to overlap removal |
| Database Schema Conflicts | HIGH | HIGH | Version metadata + migration tooling |
| External Consumer Breakage | MEDIUM | LOW | Deprecation timeline + public announcement |
| Memory Estimation Inaccuracy | MEDIUM | MEDIUM | Validation testing with actual profiler |

---

## Architectural Concerns

### AC-1: Circular Dependency Risk

**Concern:** The plan proposes validation in `ApplicationConfig` that checks model limits, but model limits come from HuggingFace which requires async I/O.

**Evidence:** `model.rs:299-441` shows `ensure_model_loaded()` is async, but config validation (line 370-418 of `lib.rs`) is sync.

**Problem:** Can't call async code from sync validation.

**Solution:**
```rust
impl ApplicationConfig {
    /// Sync validation (doesn't check model limits)
    pub fn validate(&self) -> ConfigResult<()> {
        // Basic validation: ranges, URLs, non-empty fields
        self.validate_basic()?;
        Ok(())
    }

    /// Async validation (checks against actual model limits)
    pub async fn validate_with_model(&self) -> ConfigResult<()> {
        self.validate()?; // Basic checks first

        // Load model to get actual limits
        let mut model = EmbeddingModel::new(
            self.embedding.model.id.clone(),
            self.embedding.model.max_tokens
        );
        model.ensure_model_loaded().await?;

        // Validate against actual model
        if let Some(model_max) = model.max_position_embeddings() {
            if self.embedding.model.max_tokens > model_max {
                return Err(ConfigError::Generic {
                    message: format!(
                        "max_tokens ({}) exceeds model limit ({})",
                        self.embedding.model.max_tokens,
                        model_max
                    )
                });
            }
        }

        Ok(())
    }
}
```

**Plan Update Needed:** Add this distinction to Phase 5.

---

### AC-2: Configuration Reload Strategy

**Missing:** The plan doesn't address hot-reloading of configuration.

**Question:** If a user changes `.env` and wants to apply changes without restarting:
1. How do we reload config?
2. Do we need to recreate model pool?
3. What happens to in-flight indexing operations?

**Recommendation:** Either:
1. Document that restart is required (simplest)
2. Or implement graceful config reload (complex but useful)

---

## Testing Gaps

### TG-1: No Integration Tests for Config Changes

**Gap:** Phase 7 has unit tests but no integration tests.

**Needed:**
```rust
#[tokio::test]
async fn test_end_to_end_indexing_with_new_config() {
    // Full stack test: config → parser → embeddings → storage
    let config = ApplicationConfig::from_env();
    let indexer = Indexer::new(config).await.unwrap();

    let result = indexer.index_file("test-repos/mini-redis/src/lib.rs").await;

    assert!(result.is_ok());

    // Verify chunks stored correctly
    let stored_chunks = query_qdrant_for_file("lib.rs").await;

    for chunk in stored_chunks {
        // Verify NO chunk exceeds limits
        assert!(chunk.token_count <= config.embedding.model.max_tokens);

        // Verify embeddings have correct dimensions
        assert_eq!(chunk.embedding.len(), config.embedding.model.dimensions);
    }
}
```

---

### TG-2: No Performance Regression Tests

**Gap:** Success criteria mention "Manual search test: ≤15s" but no automated test.

**Needed:**
```rust
#[tokio::test]
async fn test_performance_target_met() {
    let config = ApplicationConfig::from_env();
    let indexer = Indexer::new(config).await.unwrap();

    let start = Instant::now();
    let result = indexer.index_directory("test-repos/mini-redis/src").await;
    let elapsed = start.elapsed();

    assert!(result.is_ok());
    assert!(
        elapsed.as_secs() <= 15,
        "Indexing took {}s, expected ≤15s",
        elapsed.as_secs()
    );
}
```

Make this a blocking CI test with performance budgets.

---

## Documentation Gaps

### DG-1: Missing Migration Guide

**Gap:** Plan mentions migration guide (line 694) but doesn't specify contents.

**Needed:** `docs/MIGRATION_v0.2_to_v0.3.md`:
```markdown
# Migration Guide: v0.2 → v0.3

## Breaking Changes

### 1. Profile Enum Removed
**Before:**
```rust
let config = ApplicationConfig::with_profile(Profile::Production);
```

**After:**
```rust
let config = ApplicationConfig::from_env();
```

**Action:** Search your codebase for `with_profile` and replace with `from_env()`.

### 2. Environment Variables Renamed
| Old | New | Change |
|-----|-----|--------|
| `CODETRIEVER_EMBEDDING_BATCH_SIZE` | `CODETRIEVER_INDEXER_BATCH_SIZE` | Purpose clarified |

### 3. Chunk Overlap Removed
**Impact:** Existing indexed data may need re-indexing for consistency.

**Action:** Run `just reset-dbs && just index` to rebuild index.
```

---

### DG-2: Missing .env.sample Audit

**Gap:** Plan proposes comprehensive `.env.sample` (lines 419-540) but doesn't mention auditing current state.

**Action Needed:**
1. Compare proposed `.env.sample` with actual `.env.sample`
2. Identify obsolete variables
3. Add deprecation comments for removed variables
4. Verify all env vars in code have corresponding `.env.sample` entries

---

## Security Considerations

### SEC-1: Credential Exposure in Logs

**Concern:** Plan adds config logging (suggested) but doesn't mention credential scrubbing.

**Risk:** If we log `self.database.password`, credentials leak to logs.

**Recommendation:**
```rust
impl fmt::Debug for DatabaseConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DatabaseConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("username", &self.username)
            .field("password", &"***REDACTED***")  // Never log passwords!
            .field("ssl_mode", &self.ssl_mode)
            .finish()
    }
}
```

Already implemented in `lib.rs:729-735` with `safe_connection_string()` - good!

---

## Final Verdict

### Approve Implementation With Changes

**Conditions:**
1. Address all CRITICAL findings before proceeding
2. Implement revised phase order (validation first)
3. Add A/B testing framework for overlap removal
4. Create database migration strategy
5. Write automated validation tests
6. Document breaking changes and migration path

**Strengths of This Plan:**
- Correctly identifies the root cause of data loss
- Comprehensive analysis of current problems
- Well-structured phased approach
- Good rationale for design decisions
- Acknowledges technical debt cleanup opportunity

**Weaknesses:**
- Underestimates migration complexity
- Missing validation framework
- Insufficient testing strategy
- No rollback plan
- Overlooks database schema versioning

**Estimated Implementation Effort:**
- Original Plan: 3-4 days
- With Review Recommendations: 5-7 days (includes validation, testing, migration tooling)

**Recommendation:** Approve with mandatory changes. This is critical infrastructure work that must be done right. The extra 2-3 days for proper validation and testing will prevent costly bugs in production.

---

## Reviewer Notes

**Review Methodology:**
- Analyzed planning document (722 lines)
- Cross-referenced with actual codebase implementation
- Verified claims against code evidence
- Identified gaps between plan and current state
- Applied industry best practices (OWASP, Google SRE, Microsoft standards)

**Code Quality Standards Applied:**
- Configuration management best practices
- Database migration patterns
- Breaking change management
- Testing pyramid (unit → integration → e2e)
- Observability and debugging support

**Follow-up Required:**
- [ ] Author addresses all CRITICAL and MAJOR findings
- [ ] Updated plan submitted for re-review
- [ ] Implementation proceeds in revised phase order
- [ ] Progress tracked with daily stand-ups
- [ ] Code review scheduled for each phase completion

---

**Review Completed:** 2025-10-12
**Reviewer:** Marvin (Code Reviewer Agent)
**Next Review:** After plan updates or Phase 1 implementation completion
