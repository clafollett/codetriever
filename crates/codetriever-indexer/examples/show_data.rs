//! Example to index data and show what's stored in PostgreSQL and Qdrant

use codetriever_data::{migrations::setup_database, repository::DbFileRepository};
use codetriever_indexer::{
    indexing::{Indexer, service::FileContent},
    storage::QdrantStorage,
};
use std::sync::Arc;

// Type aliases to simplify complex types
type BranchRow = (String, String, Option<String>);
type FileRow = (String, String, String, String, i64);
type ChunkRow = (
    uuid::Uuid,
    String,
    i32,
    i32,
    i32,
    Option<String>,
    Option<String>,
);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Setup database
    let database_url = "postgresql://codetriever:codetriever@localhost:5433/codetriever";
    let pool = setup_database(database_url).await?;
    let repository = Arc::new(DbFileRepository::new(pool.clone()));

    // Setup Qdrant
    let storage = QdrantStorage::new(
        "http://localhost:6334".to_string(),
        "demo_collection".to_string(),
    )
    .await?;

    // Create indexer
    let mut indexer = Indexer::new_with_repository(repository.clone());
    indexer.set_storage(storage.clone());

    // Index some sample code
    let sample_code = r#"
use std::collections::HashMap;

fn main() {
    println!("Hello, Codetriever!");
    let mut data = HashMap::new();
    data.insert("key", "value");
}

fn process_data(input: &str) -> String {
    format!("Processed: {}", input)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_process() {
        assert_eq!(process_data("test"), "Processed: test");
    }
}
"#;

    let file = FileContent {
        path: "src/demo.rs".to_string(),
        content: sample_code.to_string(),
        hash: String::new(),
    };

    println!("Indexing sample file...\n");
    let result = indexer
        .index_file_content("demo_repo:main", vec![file])
        .await?;

    println!("Indexing complete!");
    println!("  Files indexed: {}", result.files_indexed);
    println!("  Chunks created: {}\n", result.chunks_created);

    // Query PostgreSQL data
    println!("=== PostgreSQL Data ===\n");

    // Project branches
    let branches: Vec<BranchRow> =
        sqlx::query_as("SELECT repository_id, branch, repository_url FROM project_branches")
            .fetch_all(&pool)
            .await?;

    println!("Project Branches:");
    for (repo, branch, url) in branches {
        println!("  - {repo}/{branch} (url: {url:?})");
    }

    // Indexed files
    let files: Vec<FileRow> = sqlx::query_as(
        "SELECT repository_id, branch, file_path, content_hash, generation FROM indexed_files",
    )
    .fetch_all(&pool)
    .await?;

    println!("\nIndexed Files:");
    for (repo, branch, path, hash, generation) in files {
        println!(
            "  - {}/{}: {} (gen: {}, hash: {}...)",
            repo,
            branch,
            path,
            generation,
            &hash[..8]
        );
    }

    // Chunk metadata
    let chunks: Vec<ChunkRow> = sqlx::query_as(
        "SELECT chunk_id, file_path, chunk_index, start_line, end_line, kind, name 
             FROM chunk_metadata ORDER BY file_path, chunk_index",
    )
    .fetch_all(&pool)
    .await?;

    println!("\nChunk Metadata:");
    for (id, path, idx, start, end, kind, name) in chunks {
        println!(
            "  [{:3}] {} ({}) lines {}-{}: {:?} {:?}",
            idx,
            &id.to_string()[..8],
            path,
            start,
            end,
            kind,
            name
        );
    }

    // Query Qdrant data
    println!("\n=== Qdrant Data ===\n");

    // Search for chunks
    let search_results = storage.search(vec![0.5; 768], 10).await?;

    println!("Found {} chunks in Qdrant:", search_results.len());
    for (i, chunk) in search_results.iter().enumerate() {
        println!("\n  Chunk {}:", i + 1);
        println!("    File: {}", chunk.file_path);
        println!("    Lines: {}-{}", chunk.start_line, chunk.end_line);
        println!("    Type: {:?}", chunk.kind);
        println!("    Name: {:?}", chunk.name);
        println!(
            "    Content preview: {}",
            chunk
                .content
                .chars()
                .take(60)
                .collect::<String>()
                .replace('\n', " ")
        );
    }

    Ok(())
}
