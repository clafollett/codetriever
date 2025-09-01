#[cfg(test)]
mod tests {
    use codetriever_api::{indexing::CodeChunk, storage::QdrantStorage};

    #[tokio::test]
    async fn test_store_and_retrieve_chunks() {
        // Skip if no Qdrant available
        if std::env::var("QDRANT_URL").is_err() {
            println!("Skipping Qdrant test - QDRANT_URL not set");
            return;
        }

        let storage = QdrantStorage::new(
            "http://localhost:6334".to_string(),
            "test_collection".to_string(),
        )
        .await
        .expect("Failed to create storage");

        // Create test chunks with embeddings
        let chunks = vec![
            CodeChunk {
                file_path: "test.rs".to_string(),
                content: "fn hello() { println!(\"Hello\"); }".to_string(),
                start_line: 1,
                end_line: 1,
                embedding: Some(vec![0.1; 768]), // 768-dim embedding
            },
            CodeChunk {
                file_path: "test.rs".to_string(),
                content: "fn world() { println!(\"World\"); }".to_string(),
                start_line: 2,
                end_line: 2,
                embedding: Some(vec![0.2; 768]), // 768-dim embedding
            },
        ];

        // Store chunks
        let stored_count = storage
            .store_chunks(&chunks)
            .await
            .expect("Failed to store chunks");
        assert_eq!(stored_count, 2);

        // Search for similar chunks
        let query_embedding = vec![0.15; 768]; // Close to first chunk
        let results = storage
            .search(query_embedding, 1)
            .await
            .expect("Failed to search");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "test.rs");
        assert!(results[0].content.contains("hello"));
    }
}
