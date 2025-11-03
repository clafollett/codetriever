-- Helper functions for Codetriever database operations

-- Function to atomically replace chunks for a file
CREATE OR REPLACE FUNCTION replace_file_chunks(
    p_tenant_id UUID,
    p_repository_id TEXT,
    p_branch TEXT,
    p_file_path TEXT,
    p_new_generation BIGINT
) RETURNS TABLE(deleted_chunk_id UUID) AS $$
BEGIN
    -- Return the chunk IDs that will be deleted (for Qdrant cleanup)
    RETURN QUERY
    DELETE FROM chunk_metadata
    WHERE tenant_id = p_tenant_id
      AND repository_id = p_repository_id
      AND branch = p_branch
      AND file_path = p_file_path
      AND generation < p_new_generation
    RETURNING chunk_id;
END;
$$ LANGUAGE plpgsql;

-- Function to clean up orphaned chunks (chunks without corresponding file)
CREATE OR REPLACE FUNCTION cleanup_orphaned_chunks()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM chunk_metadata cm
    WHERE NOT EXISTS (
        SELECT 1 FROM indexed_files if
        WHERE if.tenant_id = cm.tenant_id
          AND if.repository_id = cm.repository_id
          AND if.branch = cm.branch
          AND if.file_path = cm.file_path
    );

    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- Function to get indexing statistics for a project/branch
CREATE OR REPLACE FUNCTION get_indexing_stats(
    p_tenant_id UUID,
    p_repository_id TEXT,
    p_branch TEXT
) RETURNS TABLE(
    total_files BIGINT,
    total_chunks BIGINT,
    total_size_bytes BIGINT,
    last_indexed TIMESTAMPTZ,
    unique_commits BIGINT
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        COUNT(DISTINCT if.file_path)::BIGINT as total_files,
        COUNT(DISTINCT cm.chunk_id)::BIGINT as total_chunks,
        0::BIGINT as total_size_bytes, -- Placeholder, could track if needed
        MAX(if.indexed_at) as last_indexed,
        COUNT(DISTINCT if.commit_sha)::BIGINT as unique_commits
    FROM indexed_files if
    LEFT JOIN chunk_metadata cm
        ON cm.tenant_id = if.tenant_id
        AND cm.repository_id = if.repository_id
        AND cm.branch = if.branch
        AND cm.file_path = if.file_path
    WHERE if.tenant_id = p_tenant_id
      AND if.repository_id = p_repository_id
      AND if.branch = p_branch;
END;
$$ LANGUAGE plpgsql;

-- Function to handle file renames/moves
CREATE OR REPLACE FUNCTION process_file_move(
    p_tenant_id UUID,
    p_repository_id TEXT,
    p_branch TEXT,
    p_old_path TEXT,
    p_new_path TEXT
) RETURNS VOID AS $$
BEGIN
    -- Update the file path in indexed_files
    UPDATE indexed_files
    SET file_path = p_new_path
    WHERE tenant_id = p_tenant_id
      AND repository_id = p_repository_id
      AND branch = p_branch
      AND file_path = p_old_path;

    -- Update the file path in chunk_metadata
    UPDATE chunk_metadata
    SET file_path = p_new_path
    WHERE tenant_id = p_tenant_id
      AND repository_id = p_repository_id
      AND branch = p_branch
      AND file_path = p_old_path;

    -- Mark the move as processed
    UPDATE file_moves
    SET processed = TRUE
    WHERE tenant_id = p_tenant_id
      AND repository_id = p_repository_id
      AND branch = p_branch
      AND old_path = p_old_path
      AND new_path = p_new_path;
END;
$$ LANGUAGE plpgsql;