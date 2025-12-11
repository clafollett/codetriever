//! Integration tests for Qdrant payload filtering (Issue #39)
//!
//! These tests verify that search filters (repository_id, branch) are applied
//! at the Qdrant level using payload filters, not post-filtered in memory.
//!
//! TDD RED phase: These tests are written before the implementation exists.

#[path = "test_utils.rs"]
mod test_utils;

use codetriever_common::CorrelationId;
use codetriever_parsing::CodeChunk;
use codetriever_vector_data::{
    VectorStorage,
    storage::{ChunkStorageContext, SearchFilters},
};
use test_utils::{cleanup_test_storage, create_test_storage, create_test_tenant, test_config};
use uuid::Uuid;

/// Create a test chunk with embedding
///
/// # Arguments
/// * `file_path` - Relative file path for the chunk
/// * `content` - Source code content
/// * `embedding` - Pre-computed embedding vector (should be normalized)
///
/// # Returns
/// A CodeChunk with realistic metadata for testing
fn create_test_chunk(file_path: &str, content: &str, embedding: Vec<f32>) -> CodeChunk {
    CodeChunk {
        file_path: file_path.to_string(),
        content: content.to_string(),
        start_line: 1,
        end_line: 10,
        byte_start: 0,
        byte_end: content.len(),
        kind: Some("function".to_string()),
        language: "rust".to_string(),
        name: Some("test_fn".to_string()),
        token_count: Some(50),
        embedding: Some(embedding),
    }
}

/// Create a deterministic test embedding (different for each index)
/// Uses configured vector dimensions from application config
/// Returns a normalized unit vector for realistic similarity scores
fn create_test_embedding(seed: usize, dimensions: usize) -> Vec<f32> {
    let raw: Vec<f32> = (0..dimensions)
        .map(|i| ((i + seed) as f32 / 1000.0).sin())
        .collect();

    // Normalize to unit vector (matches real embedding behavior)
    let magnitude: f32 = raw.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        raw.iter().map(|x| x / magnitude).collect()
    } else {
        raw // Edge case: avoid division by zero
    }
}

/// Create storage context for a specific repo/branch
///
/// # Arguments
/// * `tenant_id` - Unique tenant identifier for isolation
/// * `repo_id` - Repository identifier (e.g., "my-repo")
/// * `branch` - Branch name (e.g., "main", "develop")
///
/// # Returns
/// ChunkStorageContext with test defaults for commit metadata
fn create_context(tenant_id: Uuid, repo_id: &str, branch: &str) -> ChunkStorageContext {
    ChunkStorageContext {
        tenant_id,
        repository_id: repo_id.to_string(),
        branch: branch.to_string(),
        generation: 1,
        repository_url: Some(format!("https://github.com/test/{repo_id}")),
        commit_sha: Some("abc123".to_string()),
        commit_message: Some("Test commit".to_string()),
        commit_date: Some(chrono::Utc::now()),
        author: Some("Test Author".to_string()),
    }
}

#[test]
fn test_qdrant_search_filters_by_repository() {
    test_utils::get_test_runtime().block_on(async {
        let config = test_config();
        let dim = config.vector_storage.vector_dimension;

        let storage = create_test_storage("repo_filter")
            .await
            .expect("Failed to create storage");

        let repository = test_utils::create_test_repository().await;
        let tenant_id = create_test_tenant(&repository).await;
        let correlation_id = CorrelationId::new();

        // Store chunks in repo-frontend
        let ctx_frontend = create_context(tenant_id, "repo-frontend", "main");
        let frontend_chunks = vec![
            create_test_chunk(
                "src/app.tsx",
                "function App() {}",
                create_test_embedding(1, dim),
            ),
            create_test_chunk(
                "src/index.tsx",
                "ReactDOM.render(<App />)",
                create_test_embedding(2, dim),
            ),
        ];
        storage
            .store_chunks(&ctx_frontend, &frontend_chunks, &correlation_id)
            .await
            .expect("Failed to store frontend chunks");

        // Store chunks in repo-backend
        let ctx_backend = create_context(tenant_id, "repo-backend", "main");
        let backend_chunks = vec![
            create_test_chunk("src/main.rs", "fn main() {}", create_test_embedding(3, dim)),
            create_test_chunk(
                "src/lib.rs",
                "pub mod handlers;",
                create_test_embedding(4, dim),
            ),
        ];
        storage
            .store_chunks(&ctx_backend, &backend_chunks, &correlation_id)
            .await
            .expect("Failed to store backend chunks");

        // Search with repository_id filter - should only return frontend results
        let filters = SearchFilters {
            repository_id: Some("repo-frontend".to_string()),
            branch: None,
        };

        let query_embedding = create_test_embedding(1, dim); // Similar to frontend chunk
        let results = storage
            .search(&tenant_id, query_embedding, 10, &filters, &correlation_id)
            .await
            .expect("Search failed");

        // Assert: all results are from repo-frontend
        assert!(
            !results.is_empty(),
            "Should find at least one result from repo-frontend"
        );
        for result in &results {
            assert_eq!(
                result.metadata.repository_id, "repo-frontend",
                "All results should be from repo-frontend, got: {}",
                result.metadata.repository_id
            );
        }

        cleanup_test_storage(&storage)
            .await
            .expect("Failed to cleanup");
    });
}

#[test]
fn test_qdrant_search_filters_by_branch() {
    test_utils::get_test_runtime().block_on(async {
        let config = test_config();
        let dim = config.vector_storage.vector_dimension;

        let storage = create_test_storage("branch_filter")
            .await
            .expect("Failed to create storage");

        let repository = test_utils::create_test_repository().await;
        let tenant_id = create_test_tenant(&repository).await;
        let correlation_id = CorrelationId::new();

        // Store chunks in main branch
        let ctx_main = create_context(tenant_id, "my-repo", "main");
        let main_chunks = vec![create_test_chunk(
            "src/lib.rs",
            "fn stable_fn() {}",
            create_test_embedding(10, dim),
        )];
        storage
            .store_chunks(&ctx_main, &main_chunks, &correlation_id)
            .await
            .expect("Failed to store main chunks");

        // Store chunks in develop branch
        let ctx_develop = create_context(tenant_id, "my-repo", "develop");
        let develop_chunks = vec![create_test_chunk(
            "src/lib.rs",
            "fn experimental_fn() {}",
            create_test_embedding(20, dim),
        )];
        storage
            .store_chunks(&ctx_develop, &develop_chunks, &correlation_id)
            .await
            .expect("Failed to store develop chunks");

        // Search with branch filter - should only return develop results
        let filters = SearchFilters {
            repository_id: None,
            branch: Some("develop".to_string()),
        };

        let query_embedding = create_test_embedding(20, dim); // Similar to develop chunk
        let results = storage
            .search(&tenant_id, query_embedding, 10, &filters, &correlation_id)
            .await
            .expect("Search failed");

        // Assert: all results are from develop branch
        assert!(
            !results.is_empty(),
            "Should find at least one result from develop branch"
        );
        for result in &results {
            assert_eq!(
                result.metadata.branch, "develop",
                "All results should be from develop branch, got: {}",
                result.metadata.branch
            );
        }

        cleanup_test_storage(&storage)
            .await
            .expect("Failed to cleanup");
    });
}

#[test]
fn test_qdrant_search_filters_by_repo_and_branch() {
    test_utils::get_test_runtime().block_on(async {
        let config = test_config();
        let dim = config.vector_storage.vector_dimension;

        let storage = create_test_storage("combined_filter")
            .await
            .expect("Failed to create storage");

        let repository = test_utils::create_test_repository().await;
        let tenant_id = create_test_tenant(&repository).await;
        let correlation_id = CorrelationId::new();

        // Store chunks in 3 combinations:
        // (repo-a, main), (repo-a, develop), (repo-b, main)
        let combos = [
            ("repo-a", "main", 100),
            ("repo-a", "develop", 200),
            ("repo-b", "main", 300),
        ];

        for (repo, branch, seed) in combos {
            let ctx = create_context(tenant_id, repo, branch);
            let chunks = vec![create_test_chunk(
                &format!("src/{repo}_{branch}.rs"),
                &format!("fn {repo}_{branch}() {{}}"),
                create_test_embedding(seed, dim),
            )];
            storage
                .store_chunks(&ctx, &chunks, &correlation_id)
                .await
                .expect("Failed to store chunks");
        }

        // Search with BOTH filters - should only return (repo-a, main)
        let filters = SearchFilters {
            repository_id: Some("repo-a".to_string()),
            branch: Some("main".to_string()),
        };

        let query_embedding = create_test_embedding(100, dim); // Similar to (repo-a, main)
        let results = storage
            .search(&tenant_id, query_embedding, 10, &filters, &correlation_id)
            .await
            .expect("Search failed");

        // Assert: exactly one result matching both filters
        assert_eq!(
            results.len(),
            1,
            "Should find exactly 1 result for (repo-a, main)"
        );
        let result = &results[0];
        assert_eq!(result.metadata.repository_id, "repo-a");
        assert_eq!(result.metadata.branch, "main");

        // Verify we DIDN'T get the other combinations (negative assertions)
        assert_ne!(
            (
                result.metadata.repository_id.as_str(),
                result.metadata.branch.as_str()
            ),
            ("repo-a", "develop"),
            "Should not match (repo-a, develop)"
        );
        assert_ne!(
            (
                result.metadata.repository_id.as_str(),
                result.metadata.branch.as_str()
            ),
            ("repo-b", "main"),
            "Should not match (repo-b, main)"
        );

        cleanup_test_storage(&storage)
            .await
            .expect("Failed to cleanup");
    });
}

#[test]
fn test_qdrant_search_without_filters_returns_all() {
    test_utils::get_test_runtime().block_on(async {
        let config = test_config();
        let dim = config.vector_storage.vector_dimension;

        let storage = create_test_storage("no_filter")
            .await
            .expect("Failed to create storage");

        let repository = test_utils::create_test_repository().await;
        let tenant_id = create_test_tenant(&repository).await;
        let correlation_id = CorrelationId::new();

        // Store chunks in multiple repos/branches
        let combos = [
            ("repo-1", "main", 1),
            ("repo-2", "develop", 2),
            ("repo-3", "feature", 3),
        ];

        for (repo, branch, seed) in combos {
            let ctx = create_context(tenant_id, repo, branch);
            let chunks = vec![create_test_chunk(
                "src/lib.rs",
                "fn test() {}",
                create_test_embedding(seed, dim),
            )];
            storage
                .store_chunks(&ctx, &chunks, &correlation_id)
                .await
                .expect("Failed to store chunks");
        }

        // Search WITHOUT filters (backward compatibility)
        let filters = SearchFilters::default(); // Both None

        let query_embedding = create_test_embedding(1, dim);
        let results = storage
            .search(&tenant_id, query_embedding, 10, &filters, &correlation_id)
            .await
            .expect("Search failed");

        // Assert: should find results from multiple repos (tenant-filtered only)
        // Note: The actual count depends on similarity scores, so we just verify we got some results
        assert!(
            results.len() >= 2,
            "Should find results from all repos when no filters, got {}",
            results.len()
        );

        // Verify we got chunks from different repos
        let repo_ids: std::collections::HashSet<_> = results
            .iter()
            .map(|r| r.metadata.repository_id.as_str())
            .collect();
        assert!(
            repo_ids.len() >= 2,
            "Should have results from multiple repos"
        );

        cleanup_test_storage(&storage)
            .await
            .expect("Failed to cleanup");
    });
}

#[test]
fn test_qdrant_filter_respects_tenant_isolation() {
    test_utils::get_test_runtime().block_on(async {
        let config = test_config();
        let dim = config.vector_storage.vector_dimension;

        let storage = create_test_storage("tenant_isolation")
            .await
            .expect("Failed to create storage");

        let repository = test_utils::create_test_repository().await;
        let tenant_a = create_test_tenant(&repository).await;
        let tenant_b = create_test_tenant(&repository).await;
        let correlation_id = CorrelationId::new();

        // Store chunk for tenant A in "shared-repo"
        let ctx_a = create_context(tenant_a, "shared-repo", "main");
        let chunks_a = vec![create_test_chunk(
            "src/a.rs",
            "fn tenant_a() {}",
            create_test_embedding(1, dim),
        )];
        storage
            .store_chunks(&ctx_a, &chunks_a, &correlation_id)
            .await
            .expect("Failed to store tenant A chunks");

        // Store chunk for tenant B in same "shared-repo" name
        let ctx_b = create_context(tenant_b, "shared-repo", "main");
        let chunks_b = vec![create_test_chunk(
            "src/b.rs",
            "fn tenant_b() {}",
            create_test_embedding(2, dim),
        )];
        storage
            .store_chunks(&ctx_b, &chunks_b, &correlation_id)
            .await
            .expect("Failed to store tenant B chunks");

        // Search as tenant A with repo filter
        let filters = SearchFilters {
            repository_id: Some("shared-repo".to_string()),
            branch: None,
        };

        let results = storage
            .search(
                &tenant_a,
                create_test_embedding(1, dim),
                10,
                &filters,
                &correlation_id,
            )
            .await
            .expect("Search failed");

        // Assert: tenant A should ONLY see their own data
        assert_eq!(
            results.len(),
            1,
            "Tenant A should only see their 1 chunk, not tenant B's"
        );
        assert!(
            results[0].chunk.content.contains("tenant_a"),
            "Should be tenant A's chunk"
        );

        cleanup_test_storage(&storage)
            .await
            .expect("Failed to cleanup");
    });
}

#[test]
fn test_qdrant_search_with_nonexistent_filters_returns_empty() {
    test_utils::get_test_runtime().block_on(async {
        let config = test_config();
        let dim = config.vector_storage.vector_dimension;

        let storage = create_test_storage("nonexistent_filter")
            .await
            .expect("Failed to create storage");

        let repository = test_utils::create_test_repository().await;
        let tenant_id = create_test_tenant(&repository).await;
        let correlation_id = CorrelationId::new();

        // Store some test data
        let ctx = create_context(tenant_id, "existing-repo", "main");
        let chunks = vec![create_test_chunk(
            "src/lib.rs",
            "fn test() {}",
            create_test_embedding(1, dim),
        )];
        storage
            .store_chunks(&ctx, &chunks, &correlation_id)
            .await
            .expect("Failed to store chunks");

        // Search with non-existent repository filter
        let filters = SearchFilters {
            repository_id: Some("nonexistent-repo".to_string()),
            branch: None,
        };

        let results = storage
            .search(
                &tenant_id,
                create_test_embedding(1, dim),
                10,
                &filters,
                &correlation_id,
            )
            .await
            .expect("Search should not error on non-match");

        // Assert: empty results, not error
        assert_eq!(
            results.len(),
            0,
            "Should return empty Vec for nonexistent repository"
        );

        cleanup_test_storage(&storage)
            .await
            .expect("Failed to cleanup");
    });
}

#[test]
fn test_qdrant_search_filters_with_special_characters() {
    test_utils::get_test_runtime().block_on(async {
        let config = test_config();
        let dim = config.vector_storage.vector_dimension;

        let storage = create_test_storage("special_chars")
            .await
            .expect("Failed to create storage");

        let repository = test_utils::create_test_repository().await;
        let tenant_id = create_test_tenant(&repository).await;
        let correlation_id = CorrelationId::new();

        // Store chunks with special characters in repo/branch names
        let ctx = create_context(
            tenant_id,
            "repo-with-dashes_and_underscores",
            "feature/my-branch",
        );
        let chunks = vec![create_test_chunk(
            "src/special.rs",
            "fn with_special_chars() {}",
            create_test_embedding(100, dim),
        )];
        storage
            .store_chunks(&ctx, &chunks, &correlation_id)
            .await
            .expect("Failed to store chunks");

        // Search with exact special character match
        let filters = SearchFilters {
            repository_id: Some("repo-with-dashes_and_underscores".to_string()),
            branch: Some("feature/my-branch".to_string()),
        };

        let results = storage
            .search(
                &tenant_id,
                create_test_embedding(100, dim),
                10,
                &filters,
                &correlation_id,
            )
            .await
            .expect("Search failed");

        // Assert: special characters handled correctly
        assert_eq!(results.len(), 1, "Should find chunk with special chars");
        assert_eq!(
            results[0].metadata.repository_id,
            "repo-with-dashes_and_underscores"
        );
        assert_eq!(results[0].metadata.branch, "feature/my-branch");

        cleanup_test_storage(&storage)
            .await
            .expect("Failed to cleanup");
    });
}

#[test]
#[ignore] // Run separately: cargo test --test qdrant_filter_tests perf -- --ignored
fn test_qdrant_filter_performance_baseline() {
    test_utils::get_test_runtime().block_on(async {
        let config = test_config();
        let dim = config.vector_storage.vector_dimension;

        let storage = create_test_storage("perf_baseline")
            .await
            .expect("Failed to create storage");

        let repository = test_utils::create_test_repository().await;
        let tenant_id = create_test_tenant(&repository).await;
        let correlation_id = CorrelationId::new();

        // Store 100 chunks across 10 repositories (10 chunks per repo)
        for repo_idx in 0..10 {
            let repo_id = format!("perf-repo-{repo_idx}");
            for chunk_idx in 0..10 {
                let ctx = create_context(tenant_id, &repo_id, "main");
                let chunks = vec![create_test_chunk(
                    &format!("src/file_{chunk_idx}.rs"),
                    &format!("fn perf_test_{chunk_idx}() {{}}"),
                    create_test_embedding(repo_idx * 100 + chunk_idx, dim),
                )];
                storage
                    .store_chunks(&ctx, &chunks, &correlation_id)
                    .await
                    .expect("Failed to store chunks");
            }
        }

        // Search with filter to specific repository
        let filters = SearchFilters {
            repository_id: Some("perf-repo-5".to_string()),
            branch: None,
        };

        let start = std::time::Instant::now();
        let results = storage
            .search(
                &tenant_id,
                create_test_embedding(500, dim),
                10,
                &filters,
                &correlation_id,
            )
            .await
            .expect("Search failed");
        let duration = start.elapsed();

        // Assert: Filter search completes in reasonable time
        assert!(
            duration < std::time::Duration::from_millis(500),
            "Search with filters took too long: {duration:?}"
        );

        // Assert: Returns results only from filtered repo
        assert!(
            !results.is_empty(),
            "Should find results from filtered repo"
        );
        for result in &results {
            assert_eq!(
                result.metadata.repository_id, "perf-repo-5",
                "All results should be from perf-repo-5"
            );
        }

        println!(
            "✅ Performance baseline: {} results in {:?}",
            results.len(),
            duration
        );

        cleanup_test_storage(&storage)
            .await
            .expect("Failed to cleanup");
    });
}
