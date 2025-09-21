//! Comprehensive tests for the chunking system

use codetriever_parsing::{
    ChunkingService, CodeSpan, TokenBudget, TokenCounter, TokenCounterRegistry,
};
use std::sync::Arc;

/// Mock token counter for deterministic testing
struct MockTokenCounter {
    chars_per_token: usize,
}

impl MockTokenCounter {
    fn new(chars_per_token: usize) -> Self {
        Self { chars_per_token }
    }
}

impl TokenCounter for MockTokenCounter {
    fn name(&self) -> &str {
        "mock-counter"
    }

    fn max_tokens(&self) -> usize {
        100
    }

    fn count(&self, text: &str) -> usize {
        // Simple approximation: chars divided by chars_per_token
        text.len() / self.chars_per_token
    }
}

#[test]
fn test_token_counter_trait() {
    let counter = MockTokenCounter::new(4); // ~4 chars per token

    assert_eq!(counter.name(), "mock-counter");
    assert_eq!(counter.max_tokens(), 100);
    assert_eq!(counter.count("hello world"), 2); // 11 chars / 4 = 2
    assert_eq!(counter.count("a"), 0); // 1 char / 4 = 0
    assert_eq!(counter.count("test"), 1); // 4 chars / 4 = 1
}

#[test]
fn test_token_counter_batch() {
    let counter = MockTokenCounter::new(4);
    let texts = vec!["hello", "world", "test"];
    let counts = counter.count_batch(&texts);

    assert_eq!(counts, vec![1, 1, 1]);
}

#[tokio::test]
async fn test_token_counter_registry() {
    // Create a mock tokenizer
    let tokenizer = create_mock_tokenizer();
    let registry = TokenCounterRegistry::new(tokenizer, 8192).await;

    // Test default counter
    let default_counter = registry.default();
    assert_eq!(default_counter.max_tokens(), 8192);

    // Test model-specific lookup
    let jina_counter = registry.for_model("jinaai/jina-embeddings-v2-small-en");
    assert_eq!(jina_counter.max_tokens(), 8192);

    // Test fallback for unknown model
    let unknown_counter = registry.for_model("unknown-model");
    assert_eq!(unknown_counter.max_tokens(), 8192); // Should get default
}

#[test]
fn test_token_budget() {
    let budget = TokenBudget::new(100, 10);

    assert_eq!(budget.hard, 100);
    assert_eq!(budget.soft, 90); // 90% of hard
    assert_eq!(budget.overlap, 10);
}

#[test]
fn test_chunking_service_simple() {
    let counter = Arc::new(MockTokenCounter::new(1)); // 1 char = 1 token
    let budget = TokenBudget {
        hard: 50,
        soft: 45,
        overlap: 5,
    };
    let service = ChunkingService::new(counter, budget);

    let spans = vec![
        CodeSpan {
            content: "a".repeat(20), // 20 tokens
            start_line: 1,
            end_line: 5,
            byte_start: 0,
            byte_end: 20,
            kind: Some("function".to_string()),
            name: Some("test_func".to_string()),
            language: "rust".to_string(),
        },
        CodeSpan {
            content: "b".repeat(20), // 20 tokens
            start_line: 6,
            end_line: 10,
            byte_start: 20,
            byte_end: 40,
            kind: Some("function".to_string()),
            name: Some("another_func".to_string()),
            language: "rust".to_string(),
        },
        CodeSpan {
            content: "c".repeat(20), // 20 tokens
            start_line: 11,
            end_line: 15,
            byte_start: 40,
            byte_end: 60,
            kind: Some("function".to_string()),
            name: Some("third_func".to_string()),
            language: "rust".to_string(),
        },
    ];

    let chunks = service.chunk_spans("test.rs", spans).unwrap();

    // Should create 2 chunks: first two spans fit in soft limit (40 < 45)
    // Third span goes to second chunk
    assert_eq!(chunks.len(), 2);

    // First chunk should have spans 1 and 2
    assert_eq!(chunks[0].start_line, 1);
    assert_eq!(chunks[0].end_line, 10);
    assert_eq!(chunks[0].byte_start, 0);
    assert_eq!(chunks[0].byte_end, 40);

    // Second chunk should have span 3
    assert_eq!(chunks[1].start_line, 11);
    assert_eq!(chunks[1].end_line, 15);
    assert_eq!(chunks[1].byte_start, 40);
    assert_eq!(chunks[1].byte_end, 60);
}

#[test]
fn test_chunking_service_large_span_splitting() {
    let counter = Arc::new(MockTokenCounter::new(1)); // 1 char = 1 token
    let budget = TokenBudget {
        hard: 50,
        soft: 45,
        overlap: 5,
    };
    let service = ChunkingService::new(counter, budget);

    // Create a span that exceeds hard limit
    let large_content = (0..10)
        .map(|i| format!("line{i}\n"))
        .collect::<Vec<_>>()
        .join("");

    let spans = vec![CodeSpan {
        content: large_content.clone(),
        start_line: 1,
        end_line: 10,
        byte_start: 0,
        byte_end: large_content.len(),
        kind: Some("function".to_string()),
        name: Some("huge_func".to_string()),
        language: "rust".to_string(),
    }];

    let chunks = service.chunk_spans("test.rs", spans).unwrap();

    // Should split the large span into multiple chunks
    assert!(chunks.len() > 1);

    // Each chunk should be within budget
    for chunk in &chunks {
        let token_count = chunk.token_count.unwrap_or(0);
        assert!(
            token_count <= budget.hard,
            "Chunk exceeds hard limit: {} > {}",
            token_count,
            budget.hard
        );
    }
}

#[test]
fn test_code_span_with_byte_ranges() {
    let span = CodeSpan {
        content: "fn main() {}".to_string(),
        start_line: 1,
        end_line: 1,
        byte_start: 0,
        byte_end: 12,
        kind: Some("function".to_string()),
        name: Some("main".to_string()),
        language: "rust".to_string(),
    };

    assert_eq!(span.byte_end - span.byte_start, span.content.len());
}

// Helper to create a mock tokenizer for testing
fn create_mock_tokenizer() -> Arc<tokenizers::Tokenizer> {
    use tokenizers::{models::bpe::BPE, tokenizer::Tokenizer};

    // Create a simple BPE tokenizer for testing
    // We don't need normalizers or pre-tokenizers for our mock
    let tokenizer = Tokenizer::new(BPE::default());

    Arc::new(tokenizer)
}

#[test]
fn test_byte_range_stability() {
    // Test that byte ranges provide stable IDs even if tokenization changes
    use codetriever_meta_data::generate_chunk_id;

    let repo = "github.com/test/repo";
    let branch = "main";
    let file = "src/main.rs";
    let generation = 1;

    // Same byte range should produce same ID
    let id1 = generate_chunk_id(repo, branch, file, generation, 0, 100);
    let id2 = generate_chunk_id(repo, branch, file, generation, 0, 100);
    assert_eq!(id1, id2);

    // Different byte ranges should produce different IDs
    let id3 = generate_chunk_id(repo, branch, file, generation, 100, 200);
    assert_ne!(id1, id3);

    // Same content at different locations should have different IDs
    let id4 = generate_chunk_id(repo, branch, file, generation, 50, 150);
    assert_ne!(id1, id4);
    assert_ne!(id3, id4);
}
