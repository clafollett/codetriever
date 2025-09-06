//! Full-stack integration test that verifies data in both PostgreSQL and Qdrant
//!
//! Run with: cargo test --test full_stack_integration -- --test-threads=1

use codetriever_data::{
    generate_chunk_id,
    migrations::setup_database,
    models::{IndexedFile, ProjectBranch},
    repository::DbFileRepository,
};
use codetriever_indexer::{
    indexing::{Indexer, service::FileContent},
    parsing::CodeChunk,
    storage::QdrantStorage,
};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

async fn setup_test_db() -> anyhow::Result<PgPool> {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://codetriever:codetriever@localhost:5433/codetriever_test".to_string()
    });

    // Create test database and run migrations
    setup_database(&database_url).await
}

async fn cleanup_test_data(pool: &PgPool, repo_id: &str, branch: &str) -> anyhow::Result<()> {
    // Clean up test data
    sqlx::query("DELETE FROM project_branches WHERE repository_id = $1 AND branch = $2")
        .bind(repo_id)
        .bind(branch)
        .execute(pool)
        .await?;
    Ok(())
}

#[tokio::test]
async fn test_full_stack_indexing_with_postgres_and_qdrant() {
    // Setup
    let pool = setup_test_db()
        .await
        .expect("Failed to setup test database");
    let repository = Arc::new(DbFileRepository::new(pool.clone()));

    let qdrant_url =
        std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6334".to_string());
    let storage = QdrantStorage::new(qdrant_url, "test_full_stack".to_string())
        .await
        .expect("Failed to create Qdrant storage");

    // Create indexer with repository and storage
    let mut indexer = Indexer::new_with_repository(repository.clone());
    indexer.set_storage(storage.clone());

    let test_repo = "test_repo";
    let test_branch = "main";
    let test_file = "src/main.rs";
    let test_content = r#"
fn main() {
    println!("Hello, world!");
}

fn helper() {
    println!("Helper function");
}
"#;

    // Clean up any existing test data
    cleanup_test_data(&pool, test_repo, test_branch)
        .await
        .expect("Failed to cleanup");

    // Index the file content - using project_id format "repo:branch" for database integration
    let project_id = format!("{test_repo}:{test_branch}");
    let file = FileContent {
        path: test_file.to_string(),
        content: test_content.to_string(),
        hash: String::new(), // Will be computed by indexer
    };

    let result = indexer
        .index_file_content(&project_id, vec![file])
        .await
        .expect("Failed to index file content");

    println!(
        "Indexed {} files with {} chunks",
        result.files_indexed, result.chunks_created
    );

    // Verify data in PostgreSQL
    let project_branch: Option<ProjectBranch> =
        sqlx::query_as("SELECT * FROM project_branches WHERE repository_id = $1 AND branch = $2")
            .bind(test_repo)
            .bind(test_branch)
            .fetch_optional(&pool)
            .await
            .expect("Failed to query project_branches");

    assert!(
        project_branch.is_some(),
        "Project branch should exist in PostgreSQL"
    );
    println!("✅ Project branch exists in PostgreSQL");

    // Verify indexed file in PostgreSQL
    let file_metadata: Option<IndexedFile> = sqlx::query_as(
        "SELECT * FROM indexed_files WHERE repository_id = $1 AND branch = $2 AND file_path = $3",
    )
    .bind(test_repo)
    .bind(test_branch)
    .bind(test_file)
    .fetch_optional(&pool)
    .await
    .expect("Failed to query indexed_files");

    assert!(
        file_metadata.is_some(),
        "File metadata should exist in PostgreSQL"
    );
    let metadata = file_metadata.unwrap();
    println!(
        "✅ File metadata exists in PostgreSQL with generation {}",
        metadata.generation
    );

    // Verify chunks in PostgreSQL
    let chunk_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM chunk_metadata 
         WHERE repository_id = $1 AND branch = $2 AND file_path = $3 AND generation = $4",
    )
    .bind(test_repo)
    .bind(test_branch)
    .bind(test_file)
    .bind(metadata.generation)
    .fetch_one(&pool)
    .await
    .expect("Failed to count chunks");

    assert!(chunk_count > 0, "Should have chunks in PostgreSQL");
    println!("✅ Found {chunk_count} chunks in PostgreSQL");

    // Get chunk IDs from PostgreSQL for verification
    let chunk_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT chunk_id FROM chunk_metadata 
         WHERE repository_id = $1 AND branch = $2 AND file_path = $3 AND generation = $4
         ORDER BY chunk_index",
    )
    .bind(test_repo)
    .bind(test_branch)
    .bind(test_file)
    .bind(metadata.generation)
    .fetch_all(&pool)
    .await
    .expect("Failed to fetch chunk IDs");

    println!("Chunk IDs from PostgreSQL:");
    for (i, id) in chunk_ids.iter().enumerate() {
        println!("  [{i}] {id}");
    }

    // Verify data in Qdrant by searching
    let search_embedding = vec![0.5; 768]; // Mock embedding
    let search_results = storage
        .search(search_embedding, 10)
        .await
        .expect("Failed to search Qdrant");

    // We should find our chunks
    let our_chunks: Vec<_> = search_results
        .iter()
        .filter(|chunk| chunk.file_path == test_file)
        .collect();

    assert!(!our_chunks.is_empty(), "Should find chunks in Qdrant");
    println!("✅ Found {} matching chunks in Qdrant", our_chunks.len());

    // Now test updating the file (generation 2)
    let updated_content = r#"
fn main() {
    println!("Hello, updated world!");
}

fn helper() {
    println!("Updated helper function");
}

fn new_function() {
    println!("This is new!");
}
"#;

    let updated_file = FileContent {
        path: test_file.to_string(),
        content: updated_content.to_string(),
        hash: String::new(), // Will be computed by indexer
    };

    let result2 = indexer
        .index_file_content(&project_id, vec![updated_file])
        .await
        .expect("Failed to index updated content");

    println!(
        "✅ Indexed updated file with {} chunks",
        result2.chunks_created
    );

    // Get the new generation value
    let updated_metadata: IndexedFile = sqlx::query_as(
        "SELECT * FROM indexed_files WHERE repository_id = $1 AND branch = $2 AND file_path = $3",
    )
    .bind(test_repo)
    .bind(test_branch)
    .bind(test_file)
    .fetch_one(&pool)
    .await
    .expect("Failed to query updated file metadata");

    assert_eq!(
        updated_metadata.generation,
        metadata.generation + 1,
        "Generation should increment"
    );

    // Verify old chunks are deleted from PostgreSQL
    let old_chunk_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM chunk_metadata 
         WHERE repository_id = $1 AND branch = $2 AND file_path = $3 AND generation = $4",
    )
    .bind(test_repo)
    .bind(test_branch)
    .bind(test_file)
    .bind(metadata.generation)
    .fetch_one(&pool)
    .await
    .expect("Failed to count old chunks");

    assert_eq!(
        old_chunk_count, 0,
        "Old chunks should be deleted from PostgreSQL"
    );
    println!("✅ Old generation chunks deleted from PostgreSQL");

    // Verify new chunks exist
    let new_chunk_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM chunk_metadata 
         WHERE repository_id = $1 AND branch = $2 AND file_path = $3 AND generation = $4",
    )
    .bind(test_repo)
    .bind(test_branch)
    .bind(test_file)
    .bind(updated_metadata.generation)
    .fetch_one(&pool)
    .await
    .expect("Failed to count new chunks");

    assert!(new_chunk_count > 0, "New chunks should exist in PostgreSQL");
    println!("✅ Found {new_chunk_count} new generation chunks in PostgreSQL");

    // Verify Qdrant has the updated chunks
    // Since we can't easily verify deletion in Qdrant without unique identifiers,
    // we'll just ensure the new content is searchable
    let search_results2 = storage
        .search(vec![0.5; 768], 20)
        .await
        .expect("Failed to search Qdrant after update");

    let updated_chunks: Vec<_> = search_results2
        .iter()
        .filter(|chunk| chunk.file_path == test_file && chunk.content.contains("updated"))
        .collect();

    assert!(
        !updated_chunks.is_empty(),
        "Should find updated chunks in Qdrant"
    );
    println!("✅ Found {} updated chunks in Qdrant", updated_chunks.len());

    // Clean up
    cleanup_test_data(&pool, test_repo, test_branch)
        .await
        .expect("Failed to cleanup");

    storage
        .drop_collection()
        .await
        .expect("Failed to drop test collection");

    println!("\n🎉 Full-stack integration test passed!");
}

#[tokio::test]
async fn test_uuid_based_chunk_deletion() {
    // Setup
    let pool = setup_test_db()
        .await
        .expect("Failed to setup test database");
    let _repository = Arc::new(DbFileRepository::new(pool.clone()));

    let qdrant_url =
        std::env::var("QDRANT_URL").unwrap_or_else(|_| "http://localhost:6334".to_string());
    let storage = QdrantStorage::new(qdrant_url, "test_uuid_deletion".to_string())
        .await
        .expect("Failed to create Qdrant storage");

    let test_repo = "test_deletion";
    let test_branch = "main";
    let test_file = "test.rs";

    // Clean up any existing test data
    cleanup_test_data(&pool, test_repo, test_branch)
        .await
        .expect("Failed to cleanup");

    // Create test chunks with known UUIDs
    let generation = 1i64;
    let chunk1_id = generate_chunk_id(test_repo, test_branch, test_file, generation, 0);
    let chunk2_id = generate_chunk_id(test_repo, test_branch, test_file, generation, 1);

    println!("Generated chunk IDs:");
    println!("  Chunk 1: {chunk1_id}");
    println!("  Chunk 2: {chunk2_id}");

    // Store chunks in Qdrant with deterministic IDs
    let chunks = vec![
        CodeChunk {
            file_path: test_file.to_string(),
            content: "fn test1() {}".to_string(),
            start_line: 1,
            end_line: 1,
            language: "rust".to_string(),
            embedding: Some(vec![0.1; 768]),
            token_count: Some(5),
            kind: Some("function".to_string()),
            name: Some("test1".to_string()),
        },
        CodeChunk {
            file_path: test_file.to_string(),
            content: "fn test2() {}".to_string(),
            start_line: 2,
            end_line: 2,
            language: "rust".to_string(),
            embedding: Some(vec![0.2; 768]),
            token_count: Some(5),
            kind: Some("function".to_string()),
            name: Some("test2".to_string()),
        },
    ];

    let stored_ids = storage
        .store_chunks_with_ids(test_repo, test_branch, &chunks, generation)
        .await
        .expect("Failed to store chunks with IDs");

    assert_eq!(stored_ids.len(), 2);
    assert_eq!(stored_ids[0], chunk1_id);
    assert_eq!(stored_ids[1], chunk2_id);
    println!("✅ Stored 2 chunks with deterministic UUIDs");

    // Verify chunks exist in Qdrant
    let search_results = storage
        .search(vec![0.15; 768], 10)
        .await
        .expect("Failed to search");

    assert!(search_results.len() >= 2, "Should find at least 2 chunks");
    println!("✅ Verified chunks exist in Qdrant");

    // Delete the chunks using their UUIDs
    storage
        .delete_chunks(&stored_ids)
        .await
        .expect("Failed to delete chunks");

    println!("✅ Deleted chunks using UUIDs: {stored_ids:?}");

    // Verify chunks are deleted
    let search_after_delete = storage
        .search(vec![0.15; 768], 10)
        .await
        .expect("Failed to search after delete");

    let remaining = search_after_delete
        .iter()
        .filter(|c| c.file_path == test_file)
        .count();

    assert_eq!(remaining, 0, "No chunks should remain after deletion");
    println!("✅ Verified chunks were deleted from Qdrant");

    // Clean up
    storage
        .drop_collection()
        .await
        .expect("Failed to drop test collection");

    cleanup_test_data(&pool, test_repo, test_branch)
        .await
        .expect("Failed to cleanup");

    println!("\n🎉 UUID-based deletion test passed!");
}
