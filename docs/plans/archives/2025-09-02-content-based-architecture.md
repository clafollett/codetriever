<!-- IMPLEMENTATION STATUS: ✅ FULLY IMPLEMENTED
- Content-based API: ✅ API accepts file content, not paths
- Generation-based chunking: ✅ Implemented in PostgreSQL schema
- State management: ✅ PostgreSQL (not SQLite as planned)
- Crate structure: ✅ Exceeded plan—10 crates instead of 3
- Chunk cleanup: ✅ Generation tracking working
- Key evolution: UUID-based chunk IDs instead of SHA256 hashes
-->

# Content-Based Architecture Refactor

## Overview

Refactor Codetriever to use a content-based architecture where the API server processes file content directly rather than reading from the filesystem. This enables Docker deployment, remote processing, and better separation of concerns.

## Problem Statement

Current architecture has the API server reading directly from the filesystem via paths, which:
- Breaks in Docker containers (no access to host filesystem)
- Prevents remote/SaaS deployment
- Couples the API to local filesystem access
- Makes testing complex with filesystem mocks

## Architecture Decision

### Three-Tier Architecture

```
┌──────────────────┐      ┌──────────────────┐      ┌──────────────────┐
│   MCP Server     │      │   HTTP API       │      │   Vector DB      │
│   (Local)        │─────▶│   (Anywhere)     │─────▶│   (Qdrant)       │
├──────────────────┤      ├──────────────────┤      ├──────────────────┤
│ - File I/O       │      │ - Content proc   │      │ - Embeddings     │
│ - JSON state     │      │ - SQLite/PG DB   │      │ - Search         │
│ - Async indexing │      │ - Generations    │      │ - Storage        │
└──────────────────┘      └──────────────────┘      └──────────────────┘
```

### Key Principles

1. **MCP Server** handles all filesystem operations locally
2. **API Server** owns state management and generation tracking
3. **Fast MCP responses** via background indexing threads
4. **Resilient** to interruptions via proper state tracking
5. **Idempotent** operations prevent duplication

## Crate Structure

```
crates/
├── codetriever-indexer/      # Core indexing logic (new)
│   ├── src/
│   │   ├── chunking.rs       # Content splitting algorithms
│   │   ├── parser.rs         # Language parsing
│   │   ├── embeddings.rs     # Embedding generation
│   │   └── lib.rs
│   └── tests/
│       └── integration.rs    # Test with mini-redis repo
│
├── codetriever-api/          # HTTP API server
│   ├── src/
│   │   ├── routes/
│   │   │   └── index.rs      # Content-based endpoints
│   │   └── main.rs
│   └── Cargo.toml            # Depends on codetriever-indexer
│
└── codetriever/              # MCP server
    ├── src/
    │   ├── indexing/
    │   │   ├── background.rs # Async indexing jobs
    │   │   └── state.rs      # JSON state persistence
    │   └── main.rs
    └── Cargo.toml            # HTTP client, serde_json

```

## Implementation Phases

### Phase 1: Create `codetriever-indexer` Crate

**Goal**: Extract and refactor core indexing logic into a standalone crate.

1. Create new crate structure
2. Move indexing logic from API crate
3. Refactor to work with content strings instead of file paths
4. Add integration tests using mini-redis repository

**Key APIs**:
```rust
pub struct FileContent {
    pub relative_path: String,
    pub content: String,
    pub language: Option<String>,
}

pub async fn index_content(
    files: Vec<FileContent>,
    project_id: &str,
    storage: &dyn VectorStorage,
) -> Result<IndexResult>
```

### Phase 2: Refactor API Endpoints

**Goal**: Change API to accept content instead of paths.

1. Modify `/index` endpoint request format:
```rust
struct IndexRequest {
    project_id: String,
    files: Vec<FileContent>,
}

struct FileContent {
    path: String,         // Relative path for display
    content: String,      // File content
    hash: String,         // Content hash for dedup
}
```

2. Remove all filesystem operations from API
3. API becomes a thin orchestration layer

### Phase 3: State Management Implementation

**Goal**: Implement proper state tracking for resilience and consistency.

#### API Server State (SQLite/PostgreSQL)

The API server owns all generation and indexing state management:

**Schema**:
```sql
-- Track indexed files and generations
CREATE TABLE indexed_files (
    project_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    generation INTEGER NOT NULL,
    indexed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (project_id, file_path)
);

-- Track chunk generations for cleanup
CREATE TABLE chunk_generations (
    project_id TEXT NOT NULL,
    file_path TEXT NOT NULL,
    current_generation INTEGER NOT NULL,
    chunks_count INTEGER NOT NULL,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (project_id, file_path)
);

-- Track file movements for cleanup
CREATE TABLE file_moves (
    project_id TEXT NOT NULL,
    old_path TEXT NOT NULL,
    new_path TEXT NOT NULL,
    detected_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

#### MCP Server State (JSON)

The MCP server uses simple JSON files for local job tracking:

**Location**: `~/.codetriever/mcp-state.json`

**Structure**:
```json
{
  "current_jobs": {
    "job-uuid-123": {
      "project_id": "my-project",
      "path": "/home/user/project",
      "started_at": "2024-01-15T10:00:00Z",
      "status": "sending_batch_3_of_10",
      "files_processed": 30,
      "files_total": 100
    }
  },
  "recent_paths": [
    "/home/user/project",
    "/home/user/other-project"
  ],
  "last_sync": "2024-01-15T09:00:00Z"
}
```

**Atomic Write Pattern**:
```rust
// Write atomically to prevent corruption
let temp_file = format!("{}.tmp", state_file);
fs::write(&temp_file, serde_json::to_string_pretty(&state)?)?;
fs::rename(temp_file, state_file)?; // Atomic on same filesystem
```

### Phase 4: Implement Background Indexing

**Goal**: Make MCP responses instant while indexing happens async.

```rust
// Instant response to MCP tool call
async fn index_handler(path: String) -> Result<CallToolResult> {
    let job_id = Uuid::new_v4().to_string();
    
    // Start background task
    tokio::spawn(async move {
        background_index(job_id, path).await;
    });
    
    // Return immediately
    Ok(json!({
        "status": "indexing_started",
        "job_id": job_id,
        "message": "Indexing in background"
    }))
}

async fn background_index(job_id: String, path: PathBuf) {
    // 1. Create job in JSON state
    // 2. Walk directory recursively
    // 3. For each file:
    //    - Read content
    //    - Calculate hash
    //    - Batch files for sending
    // 4. Send batches to API
    // 5. Update JSON state with progress
    // 6. Handle interruptions gracefully
}
```

## Vector Storage Strategy

### The Chunk ID Problem

**Challenge**: Need stable IDs for chunks that:
- Support multiple chunks per file
- Don't create orphans when code changes
- Handle file renames gracefully

### Solution: Generation-Based Chunking

```rust
struct ChunkMetadata {
    project_id: String,
    file_path: String,
    chunk_index: u32,
    generation: u64,    // Increments each time file is indexed
    content_hash: String,
    indexed_at: DateTime<Utc>,
}

// Chunk ID = hash(project_id + file_path + chunk_index + generation)
fn generate_chunk_id(meta: &ChunkMetadata) -> String {
    let input = format!(
        "{}:{}:{}:{}",
        meta.project_id,
        meta.file_path,
        meta.chunk_index,
        meta.generation
    );
    sha256(input)
}
```

### Cleanup Strategy

Before re-indexing a file (handled by API):
1. Get current generation from database
2. Increment generation
3. Delete all chunks with `generation < current` from Qdrant
4. Insert new chunks with new generation
5. Update generation in database

This ensures:
- No orphaned chunks
- Clean updates
- Efficient storage
- Consistent state management

### Handling Renames

When a file is renamed:
1. Detect via git or filesystem monitoring
2. Record in `file_moves` table
3. On next index:
   - Delete chunks for old path
   - Index under new path
4. Periodic cleanup of old generations

## API Changes

### Current (Path-Based)
```json
POST /index
{
    "path": "/home/user/project",
    "recursive": true
}
```

### New (Content-Based)
```json
POST /index
{
    "project_id": "my-project",
    "files": [
        {
            "path": "src/main.rs",
            "content": "fn main() { ... }",
            "hash": "sha256..."
        }
    ]
}
```

## Benefits

1. **Docker Ready**: API has no filesystem dependencies
2. **Scalable**: Can deploy API anywhere (Lambda, K8s, etc.)
3. **Resilient**: Proper state tracking, can resume after crashes
4. **Fast**: MCP returns instantly, indexing happens async
5. **Clean**: No orphaned chunks, efficient updates
6. **Testable**: Pure functions, no filesystem mocks needed
7. **Future-Proof**: Ready for SaaS model
8. **Simple**: MCP uses JSON, API uses database for state

## Migration Strategy

1. Build new components alongside existing code
2. Test thoroughly with integration tests
3. Switch over once stable
4. Delete old code (no backward compatibility needed)

## Open Questions

1. **Batch Size**: What's optimal for sending files to API?
   - Suggestion: 10 files or 1MB, whichever comes first

2. **Rate Limiting**: Should MCP throttle API calls?
   - Suggestion: Configurable, default to 10 req/sec

3. **Progress Reporting**: How detailed should progress be?
   - Suggestion: Track per-file and overall percentage

4. **Error Recovery**: How to handle partial failures?
   - Suggestion: Retry individual files, mark job as "partial"

## Success Metrics

- [ ] MCP response time < 50ms
- [ ] Can index 10,000 files without memory issues
- [ ] Zero orphaned chunks after 100 re-indexes
- [ ] API works in Docker container
- [ ] 90% less code in API server
- [ ] Integration tests pass with real repositories

## Timeline Estimate

- Phase 1 (Indexer Crate): 2-3 hours
- Phase 2 (API Refactor): 1-2 hours
- Phase 3 (SQLite State): 2-3 hours
- Phase 4 (Background Indexing): 2-3 hours
- Testing & Polish: 2-3 hours

**Total**: ~10-14 hours of focused work

## Conclusion

This architecture provides a clean separation of concerns, enables flexible deployment options, and sets up Codetriever for future growth as a potential SaaS product while maintaining excellent local development experience.