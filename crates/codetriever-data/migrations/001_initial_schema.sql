-- Initial schema for Codetriever database state management
-- Supports branch-aware indexing with generation-based chunk management

-- Track repository/branch combinations
CREATE TABLE IF NOT EXISTS project_branches (
    repository_id TEXT NOT NULL,  -- e.g. "github.com/clafollett/codetriever"
    branch TEXT NOT NULL,          -- e.g. "main", "feature/new-parser"
    repository_url TEXT,           -- Full Git URL for reference
    first_seen TIMESTAMPTZ DEFAULT NOW(),
    last_indexed TIMESTAMPTZ,
    PRIMARY KEY (repository_id, branch)
);

-- Track indexed files per branch with generation numbers
CREATE TABLE IF NOT EXISTS indexed_files (
    repository_id TEXT NOT NULL,
    branch TEXT NOT NULL,
    file_path TEXT NOT NULL,       -- Always relative: "src/main.rs"
    content_hash TEXT NOT NULL,    -- SHA256 of file content
    generation BIGINT NOT NULL DEFAULT 1,
    
    -- Git metadata (not part of primary key)
    commit_sha TEXT,
    commit_message TEXT,
    commit_date TIMESTAMPTZ,
    author TEXT,
    indexed_at TIMESTAMPTZ DEFAULT NOW(),
    
    PRIMARY KEY (repository_id, branch, file_path),
    FOREIGN KEY (repository_id, branch) 
        REFERENCES project_branches(repository_id, branch) ON DELETE CASCADE
);

-- Track chunks for cleanup when files are re-indexed
CREATE TABLE IF NOT EXISTS chunk_metadata (
    chunk_id TEXT PRIMARY KEY,     -- Deterministic hash
    repository_id TEXT NOT NULL,
    branch TEXT NOT NULL,
    file_path TEXT NOT NULL,
    chunk_index INT NOT NULL,       -- Position within file (0-based)
    generation BIGINT NOT NULL,
    
    -- Semantic info for debugging and analysis
    start_line INT NOT NULL,
    end_line INT NOT NULL,
    kind TEXT,                      -- "function", "class", "module", etc.
    name TEXT,                      -- Function/class/module name if applicable
    
    created_at TIMESTAMPTZ DEFAULT NOW(),
    
    -- Ensure uniqueness of chunk position within a generation
    UNIQUE(repository_id, branch, file_path, chunk_index, generation),
    
    -- Foreign key to ensure file exists
    FOREIGN KEY (repository_id, branch, file_path) 
        REFERENCES indexed_files(repository_id, branch, file_path) ON DELETE CASCADE
);

-- Track background indexing jobs
CREATE TABLE IF NOT EXISTS indexing_jobs (
    job_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    repository_id TEXT NOT NULL,
    branch TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    files_total INT,
    files_processed INT DEFAULT 0,
    chunks_created INT DEFAULT 0,
    commit_sha TEXT,
    started_at TIMESTAMPTZ DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    error_message TEXT,
    
    -- Ensure valid status values
    CONSTRAINT valid_status CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    
    -- Foreign key to project
    FOREIGN KEY (repository_id, branch) 
        REFERENCES project_branches(repository_id, branch) ON DELETE CASCADE
);

-- Track file moves/renames for cleanup
CREATE TABLE IF NOT EXISTS file_moves (
    id SERIAL PRIMARY KEY,
    repository_id TEXT NOT NULL,
    branch TEXT NOT NULL,
    old_path TEXT NOT NULL,
    new_path TEXT NOT NULL,
    detected_at TIMESTAMPTZ DEFAULT NOW(),
    processed BOOLEAN DEFAULT FALSE,
    
    -- Foreign key to project
    FOREIGN KEY (repository_id, branch) 
        REFERENCES project_branches(repository_id, branch) ON DELETE CASCADE
);