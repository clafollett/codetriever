-- Performance-critical indexes for codetriever
-- These indexes optimize the most common query patterns

-- Index for file state checking (hot path)
CREATE INDEX IF NOT EXISTS idx_indexed_files_lookup 
ON indexed_files(repository_id, branch, file_path);

-- Index for chunk retrieval by file
CREATE INDEX IF NOT EXISTS idx_chunks_by_file 
ON chunk_metadata(repository_id, branch, file_path, generation, chunk_index);

-- Index for chunk deletion by generation
CREATE INDEX IF NOT EXISTS idx_chunks_by_generation 
ON chunk_metadata(repository_id, branch, file_path, generation);

-- Index for chunk deletion by ID (for batch deletes)
CREATE INDEX IF NOT EXISTS idx_chunks_by_id 
ON chunk_metadata(chunk_id);

-- Index for job status checks
CREATE INDEX IF NOT EXISTS idx_jobs_running 
ON indexing_jobs(repository_id, branch, status) 
WHERE status IN ('pending', 'running');

-- Index for project branch lookups
CREATE INDEX IF NOT EXISTS idx_project_branches_lookup 
ON project_branches(repository_id, branch);

-- Partial index for recently indexed files (last 7 days)
CREATE INDEX IF NOT EXISTS idx_recently_indexed 
ON indexed_files(repository_id, branch, indexed_at) 
WHERE indexed_at > NOW() - INTERVAL '7 days';

-- Index for commit-based queries
CREATE INDEX IF NOT EXISTS idx_files_by_commit 
ON indexed_files(repository_id, branch, commit_sha) 
WHERE commit_sha IS NOT NULL;

-- Composite index for chunk metadata queries with covering columns
CREATE INDEX IF NOT EXISTS idx_chunks_covering 
ON chunk_metadata(repository_id, branch, file_path) 
INCLUDE (chunk_id, generation, start_line, end_line, kind, name);

-- BRIN index for time-series data (efficient for large tables)
CREATE INDEX IF NOT EXISTS idx_chunks_created_brin 
ON chunk_metadata USING brin(created_at);

-- Analyze tables to update statistics after index creation
ANALYZE indexed_files;
ANALYZE chunk_metadata;
ANALYZE indexing_jobs;
ANALYZE project_branches;