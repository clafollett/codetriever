//! Manual search test to explore the indexed content

use codetriever_api::{config::Config, indexing::Indexer, storage::QdrantStorage};
use std::path::Path;

#[tokio::test]
async fn test_manual_searches() {
    // Skip if no Qdrant available
    if std::env::var("QDRANT_URL").is_err() {
        println!("Skipping - QDRANT_URL not set");
        return;
    }

    // First, index the mini-redis repo
    let config = Config::default();
    let storage = QdrantStorage::new(
        "http://localhost:6334".to_string(),
        "test_search_exploration".to_string(),
    )
    .await
    .expect("Failed to create storage");

    let mut indexer = Indexer::with_config_and_storage(&config, storage);

    // Check if we need to index first
    let test_queries = vec![
        "parse command from client",
        "redis connection handling",
        "async tokio spawn",
        "error handling and logging",
        "pub struct Connection",
        "impl Display",
        "fn new",
        "mutex lock deadlock",
        "tcp socket accept",
        "hash map insert",
    ];

    // Try a search to see if already indexed
    let test_result = indexer.search(test_queries[0], 1).await;

    if test_result.is_err() || test_result.unwrap().is_empty() {
        println!("Index is empty, indexing mini-redis first...");
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let test_path = Path::new(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("test-repos/rust-mini-redis/src");

        let result = indexer
            .index_directory(&test_path, true)
            .await
            .expect("Failed to index");

        println!(
            "Indexed {} files, {} chunks\n",
            result.files_indexed, result.chunks_created
        );
    }

    // Now run our test queries
    println!("\n🔍 Running test queries:\n");
    println!("{:-<80}", "");

    for query in test_queries {
        println!("\n📝 Query: \"{query}\"");
        println!("{:-<80}", "");

        let results = indexer.search(query, 3).await.expect("Search failed");

        if results.is_empty() {
            println!("  ❌ No results found");
        } else {
            for (i, chunk) in results.iter().enumerate() {
                println!(
                    "\n  Result #{} from {}:{}-{}",
                    i + 1,
                    chunk.file_path,
                    chunk.start_line,
                    chunk.end_line
                );

                // Show first 3 lines of the chunk
                let preview: Vec<&str> = chunk.content.lines().take(3).collect();
                for line in preview {
                    println!("    | {line}");
                }
                if chunk.content.lines().count() > 3 {
                    println!("    | ...");
                }
            }
        }
    }

    println!("\n{:-<80}", "");
    println!("✅ Search exploration complete!");
}
