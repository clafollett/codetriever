-- Initial schema for Codetriever database state management
-- Supports multi-tenant, branch-aware indexing with generation-based chunk management

-- Multi-tenancy: Each tenant gets isolated data (perfect for tests + production SaaS)
CREATE TABLE IF NOT EXISTS tenants (
    tenant_id UUID PRIMARY KEY DEFAULT uuidv7(),
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ,
    metadata JSONB  -- Extensible: plan limits, features, billing info, etc.
);

-- Track repository/branch combinations per tenant
CREATE TABLE IF NOT EXISTS project_branches (
    tenant_id UUID NOT NULL REFERENCES tenants(tenant_id) ON DELETE CASCADE,
    repository_id TEXT NOT NULL,  -- e.g. "github.com/clafollett/codetriever"
    branch TEXT NOT NULL,          -- e.g. "main", "feature/new-parser"
    repository_url TEXT,           -- Full Git URL for reference
    first_seen TIMESTAMPTZ DEFAULT NOW(),
    last_indexed TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, repository_id, branch)
);

-- Index for efficient tenant-based queries
CREATE INDEX IF NOT EXISTS idx_project_branches_tenant ON project_branches(tenant_id);

-- Track indexed files per branch with generation numbers
CREATE TABLE IF NOT EXISTS indexed_files (
    tenant_id UUID NOT NULL,
    repository_id TEXT NOT NULL,
    branch TEXT NOT NULL,
    file_path TEXT NOT NULL,       -- Always relative: "src/main.rs"
    file_content TEXT NOT NULL,    -- Full file content (converted to UTF-8)
    content_hash TEXT NOT NULL,    -- SHA256 of file content
    encoding TEXT NOT NULL,        -- Original encoding ("UTF-8", "UTF-16LE", "Windows-1252", etc.)
    size_bytes BIGINT NOT NULL,    -- Original file size in bytes (before UTF-8 conversion)
    generation BIGINT NOT NULL DEFAULT 1,

    -- Git metadata
    commit_sha TEXT,
    commit_message TEXT,
    commit_date TIMESTAMPTZ,
    author TEXT,

    -- Timestamps
    indexed_at TIMESTAMPTZ DEFAULT NOW(),

    PRIMARY KEY (tenant_id, repository_id, branch, file_path),
    FOREIGN KEY (tenant_id, repository_id, branch)
        REFERENCES project_branches(tenant_id, repository_id, branch) ON DELETE CASCADE
);

-- Track chunks for cleanup when files are re-indexed
CREATE TABLE IF NOT EXISTS chunk_metadata (
    chunk_id UUID PRIMARY KEY,      -- Deterministic UUID v5
    tenant_id UUID NOT NULL,
    repository_id TEXT NOT NULL,
    branch TEXT NOT NULL,
    file_path TEXT NOT NULL,
    chunk_index INT NOT NULL,       -- Position within file (0-based)
    generation BIGINT NOT NULL,

    -- Line-based info for user-facing display
    start_line INT NOT NULL,
    end_line INT NOT NULL,

    -- Byte-based info for system stability
    byte_start BIGINT NOT NULL,
    byte_end BIGINT NOT NULL,

    -- Semantic info for debugging and analysis
    kind TEXT,                      -- "function", "class", "module", etc.
    name TEXT,                      -- Function/class/module name if applicable

    created_at TIMESTAMPTZ DEFAULT NOW(),

    -- Ensure uniqueness of chunk position within a generation
    UNIQUE(tenant_id, repository_id, branch, file_path, chunk_index, generation),

    -- Foreign key to ensure file exists
    FOREIGN KEY (tenant_id, repository_id, branch, file_path)
        REFERENCES indexed_files(tenant_id, repository_id, branch, file_path) ON DELETE CASCADE
);

-- Track background indexing jobs
CREATE TABLE IF NOT EXISTS indexing_jobs (
    job_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    repository_id TEXT NOT NULL,
    branch TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    files_total INT,
    files_processed INT DEFAULT 0,
    chunks_created INT DEFAULT 0,

    -- Git commit metadata (required - indexing always happens in Git context)
    repository_url TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    commit_message TEXT NOT NULL,
    commit_date TIMESTAMPTZ NOT NULL,
    author TEXT NOT NULL,

    -- Vector storage namespace - workers use this to route to correct collection/index
    -- Maps to: Qdrant collection, Pinecone index, Milvus collection, etc.
    vector_namespace TEXT NOT NULL,

    started_at TIMESTAMPTZ DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    error_message TEXT,

    -- Ensure valid status values
    CONSTRAINT valid_status CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),

    -- Foreign key to project
    FOREIGN KEY (tenant_id, repository_id, branch)
        REFERENCES project_branches(tenant_id, repository_id, branch) ON DELETE CASCADE
);

-- Index for efficient tenant-based job queries
CREATE INDEX IF NOT EXISTS idx_indexing_jobs_tenant ON indexing_jobs(tenant_id);

-- Per-file indexing job queue (persistent, survives restarts)
-- Uses UUID v7 for time-ordered primary keys (better insert performance than random UUIDs)
CREATE TABLE IF NOT EXISTS indexing_job_file_queue (
    id UUID PRIMARY KEY DEFAULT uuidv7(),  -- Time-ordered UUID for sequential inserts (PG 18+)
    job_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    repository_id TEXT NOT NULL,
    branch TEXT NOT NULL,
    file_path TEXT NOT NULL,
    file_content TEXT NOT NULL,       -- Full file content to be indexed
    content_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    priority INT DEFAULT 0,
    retry_count INT DEFAULT 0,
    error_message TEXT,

    -- Timestamps
    created_at TIMESTAMPTZ DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,

    -- Foreign key to parent job
    FOREIGN KEY (job_id) REFERENCES indexing_jobs(job_id) ON DELETE CASCADE
);

-- Index for efficient queue polling (SELECT ... FOR UPDATE SKIP LOCKED)
-- Includes tenant_id for tenant-isolated queue processing
CREATE INDEX IF NOT EXISTS idx_queue_status_priority
    ON indexing_job_file_queue (tenant_id, status, priority DESC, created_at ASC);

-- Track file moves/renames for cleanup
CREATE TABLE IF NOT EXISTS file_moves (
    id SERIAL PRIMARY KEY,
    tenant_id UUID NOT NULL,
    repository_id TEXT NOT NULL,
    branch TEXT NOT NULL,
    old_path TEXT NOT NULL,
    new_path TEXT NOT NULL,
    detected_at TIMESTAMPTZ DEFAULT NOW(),
    processed BOOLEAN DEFAULT FALSE,

    -- Foreign key to project
    FOREIGN KEY (tenant_id, repository_id, branch)
        REFERENCES project_branches(tenant_id, repository_id, branch) ON DELETE CASCADE
);