#[path = "test_utils.rs"]
mod test_utils;

use codetriever_common::CorrelationId;
use codetriever_parsing::CodeChunk;
use codetriever_vector_data::VectorStorage;
use test_utils::{cleanup_test_storage, create_test_storage};

#[test]
fn test_store_and_retrieve_chunks() {
    codetriever_test_utils::get_test_runtime().block_on(async {
        let storage = create_test_storage("qdrant_chunks")
            .await
            .expect("Failed to create storage");

        // Create test chunks with embeddings
        let chunks = vec![
            CodeChunk {
                file_path: "test.rs".to_string(),
                content: "fn hello() { println!(\"Hello\"); }".to_string(),
                start_line: 1,
                end_line: 1,
                byte_start: 0,
                byte_end: 34,
                kind: Some("function".to_string()),
                language: "rust".to_string(),
                name: Some("hello".to_string()),
                token_count: Some(10),
                embedding: Some(vec![0.1; 768]), // 768-dim embedding
            },
            CodeChunk {
                file_path: "test.rs".to_string(),
                content: "fn world() { println!(\"World\"); }".to_string(),
                start_line: 2,
                end_line: 2,
                byte_start: 34,
                byte_end: 68,
                kind: Some("function".to_string()),
                language: "rust".to_string(),
                name: Some("world".to_string()),
                token_count: Some(10),
                embedding: Some(vec![0.2; 768]), // 768-dim embedding
            },
        ];

        let correlation_id = CorrelationId::new();

        // Store chunks
        let chunk_ids = storage
            .store_chunks("test_repo", "main", &chunks, 1, &correlation_id)
            .await
            .expect("Failed to store chunks");
        assert_eq!(chunk_ids.len(), 2);

        // Search for similar chunks
        let query_embedding = vec![0.15; 768]; // Close to first chunk
        let results = storage
            .search(query_embedding, 1, &correlation_id)
            .await
            .expect("Failed to search");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk.file_path, "test.rs");
        assert!(results[0].chunk.content.contains("hello"));

        cleanup_test_storage(&storage)
            .await
            .expect("Failed to cleanup");
    })
}
