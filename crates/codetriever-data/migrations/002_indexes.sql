-- Performance indexes for Codetriever database

-- Index for finding files by project and branch
CREATE INDEX IF NOT EXISTS idx_indexed_files_repo_branch 
    ON indexed_files(repository_id, branch);

-- Index for finding files by content hash (deduplication checks)
CREATE INDEX IF NOT EXISTS idx_indexed_files_content_hash 
    ON indexed_files(content_hash);

-- Index for finding chunks by file
CREATE INDEX IF NOT EXISTS idx_chunk_metadata_file 
    ON chunk_metadata(repository_id, branch, file_path);

-- Index for finding chunks by generation (for cleanup)
CREATE INDEX IF NOT EXISTS idx_chunk_metadata_generation 
    ON chunk_metadata(repository_id, branch, file_path, generation);

-- Index for finding active/pending jobs
CREATE INDEX IF NOT EXISTS idx_indexing_jobs_status 
    ON indexing_jobs(status) 
    WHERE status IN ('pending', 'running');

-- Index for finding jobs by project
CREATE INDEX IF NOT EXISTS idx_indexing_jobs_repo 
    ON indexing_jobs(repository_id, branch);

-- Index for finding unprocessed file moves
CREATE INDEX IF NOT EXISTS idx_file_moves_unprocessed 
    ON file_moves(repository_id, branch, processed) 
    WHERE processed = FALSE;

-- Index for commit SHA lookups
CREATE INDEX IF NOT EXISTS idx_indexed_files_commit_sha 
    ON indexed_files(commit_sha) 
    WHERE commit_sha IS NOT NULL;