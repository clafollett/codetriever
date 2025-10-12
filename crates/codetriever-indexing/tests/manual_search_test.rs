//! Manual search test to explore the indexed content

#[path = "test_utils.rs"]
mod test_utils;

use codetriever_indexing::indexing::Indexer;
use codetriever_search::SearchProvider;
use codetriever_vector_data::VectorStorage;
use std::{path::Path, sync::Arc};
use test_utils::{
    cleanup_test_storage, create_code_parser_with_tokenizer, create_test_embedding_service,
    create_test_repository, create_test_storage, test_config,
};

#[test]
fn test_manual_searches() {
    test_utils::get_test_runtime().block_on(async {
        // Note: This test requires Qdrant to be running locally on port 6334
        // You can start it with: docker-compose -f docker/docker-compose.qdrant.yml up -d

        // Create all required dependencies
        let config = test_config();
        let storage = create_test_storage("search_exploration")
            .await
            .expect("Failed to create storage");

        if storage.collection_exists().await.unwrap() {
            println!("Dropping collection to start with a clean slate...\n");
            match storage.drop_collection().await {
                Ok(_) => println!("Collection dropped successfully"),
                Err(e) => println!("Failed to drop collection: {e}"),
            }
        }

        println!("Creating collection...\n");
        match storage.ensure_collection().await {
            Ok(_) => println!("Collection created/verified successfully"),
            Err(e) => println!("Failed to create collection: {e}"),
        }

        let embedding_service = create_test_embedding_service();
        let repository = create_test_repository().await;

        // Load tokenizer for accurate chunking
        let code_parser = create_code_parser_with_tokenizer(&embedding_service).await;
        let mut indexer = Indexer::new(
            embedding_service,
            Arc::new(storage.clone()) as Arc<dyn VectorStorage>,
            repository,
            code_parser,
            &config,
        );

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
        let embedding_service = indexer.embedding_service();
        let vector_storage = indexer.vector_storage();

        // Create database client for search
        let db_config =
            codetriever_config::DatabaseConfig::for_profile(codetriever_config::Profile::Test);
        let pools = codetriever_meta_data::PoolManager::new(
            &db_config,
            codetriever_meta_data::PoolConfig::default(),
        )
        .await
        .expect("Failed to create pool manager");
        let db_client = std::sync::Arc::new(codetriever_meta_data::DataClient::new(pools));

        let search_service =
            codetriever_search::SearchService::new(embedding_service, vector_storage, db_client);
        let correlation_id = codetriever_common::CorrelationId::new();
        let test_result = search_service
            .search(test_queries[0], 1, &correlation_id)
            .await;

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

            let correlation_id = codetriever_common::CorrelationId::new();
            let results = search_service
                .search(query, 3, &correlation_id)
                .await
                .expect("Search failed");

            if results.is_empty() {
                println!("  ❌ No results found");
            } else {
                for (i, result) in results.iter().enumerate() {
                    println!(
                        "\n  Result #{} from {}:{}-{} (score: {:.3})",
                        i + 1,
                        result.chunk.file_path,
                        result.chunk.start_line,
                        result.chunk.end_line,
                        result.similarity
                    );

                    // Show first 3 lines of the chunk
                    let preview: Vec<&str> = result.chunk.content.lines().take(3).collect();
                    for line in preview {
                        println!("    | {line}");
                    }
                    if result.chunk.content.lines().count() > 3 {
                        println!("    | ...");
                    }
                }
            }
        }

        cleanup_test_storage(&storage)
            .await
            .expect("Failed to cleanup");

        println!("\n{:-<80}", "");
        println!("✅ Search exploration complete!");
    })
}
