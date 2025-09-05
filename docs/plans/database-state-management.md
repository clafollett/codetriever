# Database State Management with Branch-Aware Indexing

## Overview

Implementation of PostgreSQL-based state management for Codetriever with branch-aware indexing and generation-based chunk management.

## Key Concepts

### Repository Identity
- Use Git remote URL as stable project identifier
- Track repository + branch combinations
- Path-independent indexing (portable across environments)

### Generation-Based Chunking
- Each file has a generation number that increments on content change
- Chunks are replaced atomically as a complete set per generation
- Old generation chunks are deleted before new ones are inserted
- Prevents orphaned chunks and ensures consistency

### Branch Isolation
- Each branch maintains its own index
- Enables feature branch development without affecting main
- Supports multi-version documentation and comparison

## Database Schema

### Core Tables

```sql
-- Repository/branch combinations
CREATE TABLE project_branches (
    repository_id TEXT NOT NULL,  -- "github.com/clafollett/codetriever"
    branch TEXT NOT NULL,          -- "main", "feature/xyz"
    repository_url TEXT,
    first_seen TIMESTAMPTZ DEFAULT NOW(),
    last_indexed TIMESTAMPTZ,
    PRIMARY KEY (repository_id, branch)
);

-- Indexed files per branch
CREATE TABLE indexed_files (
    repository_id TEXT NOT NULL,
    branch TEXT NOT NULL,
    file_path TEXT NOT NULL,       -- Always relative: "src/main.rs"
    content_hash TEXT NOT NULL,
    generation BIGINT NOT NULL DEFAULT 1,
    
    -- Git metadata (not part of primary key)
    commit_sha TEXT,
    commit_message TEXT,
    commit_date TIMESTAMPTZ,
    indexed_at TIMESTAMPTZ DEFAULT NOW(),
    
    PRIMARY KEY (repository_id, branch, file_path),
    FOREIGN KEY (repository_id, branch) 
        REFERENCES project_branches(repository_id, branch)
);

-- Chunk tracking for cleanup
CREATE TABLE chunk_metadata (
    chunk_id TEXT PRIMARY KEY,     -- Deterministic hash
    repository_id TEXT NOT NULL,
    branch TEXT NOT NULL,
    file_path TEXT NOT NULL,
    chunk_index INT NOT NULL,       -- Position within file
    generation BIGINT NOT NULL,
    
    -- Semantic info for debugging
    start_line INT,
    end_line INT,
    kind TEXT,                      -- "function", "class", etc.
    name TEXT,                      -- Function/class name
    
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(repository_id, branch, file_path, chunk_index, generation)
);

-- Background indexing jobs
CREATE TABLE indexing_jobs (
    job_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id TEXT NOT NULL,
    branch TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    files_total INT,
    files_processed INT DEFAULT 0,
    commit_sha TEXT,
    started_at TIMESTAMPTZ DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    error_message TEXT
);
```

## Chunk ID Generation

Deterministic chunk IDs based on all identifying components:

```rust
pub fn generate_chunk_id(
    repository_id: &str,
    branch: &str,
    file_path: &str,
    generation: i64,
    chunk_index: u32,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(repository_id);
    hasher.update(":");
    hasher.update(branch);
    hasher.update(":");
    hasher.update(file_path);
    hasher.update(":");
    hasher.update(generation.to_le_bytes());
    hasher.update(":");
    hasher.update(chunk_index.to_le_bytes());
    
    format!("{:x}", hasher.finalize())
}
```

## Repository Detection

Extract stable repository ID from Git:

```rust
pub fn normalize_git_url(url: &str) -> String {
    // Convert various Git URL formats to consistent ID
    // https://github.com/user/repo.git -> github.com/user/repo
    // git@github.com:user/repo.git -> github.com/user/repo
    // https://gitlab.com/org/project -> gitlab.com/org/project
    
    url.trim_end_matches(".git")
       .replace("https://", "")
       .replace("git@", "")
       .replace(":", "/")
       .to_lowercase()
}
```

## Migration Strategy

Use PostgreSQL advisory locks to prevent concurrent migrations:

```rust
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    // Acquire advisory lock (non-blocking check first)
    let locked = sqlx::query_scalar!(
        "SELECT pg_try_advisory_lock(1337)"
    )
    .fetch_one(pool)
    .await?;
    
    if !locked {
        // Wait for lock
        sqlx::query!("SELECT pg_advisory_lock(1337)")
            .execute(pool)
            .await?;
    }
    
    // Run migrations
    sqlx::migrate!("./migrations").run(pool).await?;
    
    // Release lock
    sqlx::query!("SELECT pg_advisory_unlock(1337)")
        .execute(pool)
        .await?;
    
    Ok(())
}
```

## Atomic Chunk Replacement

When a file changes:

1. **Check file state** - Compare content hash
2. **Increment generation** - If content changed
3. **Begin transaction**
4. **Delete old chunks** - All chunks with old generation
5. **Insert new chunks** - All with new generation
6. **Update file record** - New generation and metadata
7. **Commit transaction**

This ensures no mixed state or orphaned chunks.

## API Changes

### Index Request Structure
```json
{
    "repository": {
        "id": "github.com/clafollett/codetriever",
        "url": "https://github.com/clafollett/codetriever.git",
        "branch": "feature/content-indexing"
    },
    "files": [
        {
            "path": "src/main.rs",  // Always relative
            "content": "...",
            "hash": "sha256..."
        }
    ]
}
```

### Vector Storage Payload
```json
{
    "repository_id": "github.com/clafollett/codetriever",
    "branch": "main",
    "file_path": "src/indexer.rs",
    "generation": 2,
    "chunk_index": 3,
    "content": "...",
    // ... other metadata
}
```

## Benefits

1. **Path Independence** - Repository can move without breaking index
2. **Branch Isolation** - Each branch has separate index
3. **No Orphans** - Generation-based replacement is atomic
4. **Multi-Tenant Ready** - Add org_id for SaaS deployment
5. **Git Aware** - Tracks commits, branches, dirty state
6. **Idempotent** - Re-indexing same content is no-op
7. **Concurrent Safe** - PostgreSQL handles multiple workers

## Implementation Status

- [ ] Create codetriever-data crate
- [ ] Design database schema
- [ ] Implement repository detection
- [ ] Build migration runner
- [ ] Create repository layer
- [ ] Implement chunk ID generation
- [ ] Add PostgreSQL to Docker
- [ ] Update indexer for generations
- [ ] Update API initialization
- [ ] Update Qdrant storage
- [ ] Add integration tests
- [ ] Test atomic replacement

## Future Enhancements

- Add organization/user column for multi-tenancy
- Track commit SHA for time-travel features
- Implement file move detection
- Add metrics for indexing performance
- Support for monorepo sub-projects