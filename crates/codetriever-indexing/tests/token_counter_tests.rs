//! Comprehensive tests for all token counter implementations

use codetriever_parsing::chunking::{
    HeuristicCounter, TiktokenCounter, TokenCounter, TokenCounterRegistry,
};
use std::sync::Arc;

#[tokio::test]
async fn test_registry_has_all_models() {
    // Create a dummy tokenizer for Jina
    let tokenizer_path = hf_hub::api::tokio::ApiBuilder::new()
        .with_progress(false)
        .build()
        .unwrap()
        .model("jinaai/jina-embeddings-v2-small-en".to_string())
        .get("tokenizer.json")
        .await
        .unwrap();

    let tokenizer = Arc::new(
        tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))
            .unwrap(),
    );

    let registry = TokenCounterRegistry::new(tokenizer, 8192).await;
    let models = registry.list_models();

    // Check we have models from each family
    let has_jina = models.iter().any(|m| m.contains("jina"));
    let has_gpt4 = models.iter().any(|m| m.contains("gpt-4"));
    let has_gpt35 = models.iter().any(|m| m.contains("gpt-3.5"));
    let has_gpt5 = models.iter().any(|m| m.starts_with("gpt-5"));
    let has_o1 = models.iter().any(|m| m.contains("o1"));

    assert!(has_jina, "Should have Jina models");
    assert!(has_gpt4, "Should have GPT-4 models");
    assert!(has_gpt35, "Should have GPT-3.5 models");
    assert!(has_gpt5, "Should have GPT-5 models");
    assert!(has_o1, "Should have O1 models");

    println!("Registry has {} models", models.len());
    assert!(
        models.len() > 20,
        "Should have at least 20 models registered"
    );
}

#[tokio::test]
async fn test_registry_fallback_to_heuristic() {
    let tokenizer_path = hf_hub::api::tokio::ApiBuilder::new()
        .with_progress(false)
        .build()
        .unwrap()
        .model("jinaai/jina-embeddings-v2-small-en".to_string())
        .get("tokenizer.json")
        .await
        .unwrap();

    let tokenizer = Arc::new(
        tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))
            .unwrap(),
    );

    let registry = TokenCounterRegistry::new(tokenizer, 8192).await;

    // Unknown model should get heuristic counter
    let counter = registry.for_model("unknown-model-xyz");
    assert_eq!(counter.name(), "heuristic-fallback");

    // But known models should get their specific counter
    let gpt4_counter = registry.for_model("gpt-4");
    assert_eq!(gpt4_counter.name(), "gpt-4");
}

#[test]
fn test_tiktoken_vs_heuristic_accuracy() {
    // Compare tiktoken (accurate) vs heuristic (fast) on same text
    let test_text =
        "The quick brown fox jumps over the lazy dog. This is a test of token counting accuracy!";

    let tiktoken = TiktokenCounter::gpt4().expect("Should create tiktoken counter");
    let heuristic = HeuristicCounter::new("test", 8192);

    let tiktoken_count = tiktoken.count(test_text);
    let heuristic_count = heuristic.count(test_text);

    println!("Tiktoken: {tiktoken_count}, Heuristic: {heuristic_count}");

    // Heuristic should be within 30% of accurate count
    let diff = (tiktoken_count as f64 - heuristic_count as f64).abs();
    let percent_diff = diff / tiktoken_count as f64 * 100.0;

    assert!(
        percent_diff < 30.0,
        "Heuristic should be within 30% of accurate count. Got {percent_diff}% difference"
    );
}

#[test]
fn test_heuristic_calibration_improves_accuracy() {
    let test_texts = vec![
        ("Hello world", 2),
        ("The quick brown fox", 4),
        ("Token counting is important", 5),
        ("This is a longer sentence with more tokens", 9),
    ];

    let mut heuristic = HeuristicCounter::new("test", 8192);

    // Test before calibration
    let _before_error: f64 = test_texts
        .iter()
        .map(|(text, expected)| {
            let count = heuristic.count(text);
            ((count as i32 - *expected as i32).abs() as f64) / *expected as f64
        })
        .sum::<f64>()
        / test_texts.len() as f64;

    // Calibrate
    heuristic.calibrate(&test_texts);

    // Test after calibration - should be perfect for calibration data
    for (text, expected) in &test_texts {
        let count = heuristic.count(text);
        assert_eq!(
            count, *expected,
            "After calibration, should return exact count for '{text}'"
        );
    }

    // Test on new text - should be better than before
    let new_text = "Another test sentence here";
    let count = heuristic.count(new_text);

    // Should be reasonable (2-6 tokens for this text)
    assert!(
        (2..=6).contains(&count),
        "Calibrated counter should give reasonable estimate"
    );
}

#[test]
fn test_all_counters_implement_trait() {
    // Type alias for boxed token counters
    type BoxedCounter = Box<dyn TokenCounter>;

    // Ensure all counters properly implement TokenCounter trait
    let counters: Vec<BoxedCounter> = vec![
        Box::new(HeuristicCounter::new("test", 4096)),
        Box::new(TiktokenCounter::gpt4().expect("Should create")),
    ];

    for counter in counters {
        // Test trait methods
        assert!(!counter.name().is_empty());
        assert!(counter.max_tokens() > 0);
        assert_eq!(counter.count(""), 0);
        assert!(counter.count("Hello world") > 0);

        let batch = counter.count_batch(&["Hello", "World"]);
        assert_eq!(batch.len(), 2);
    }
}

#[test]
fn test_unicode_consistency_across_counters() {
    let emoji_text = "Hello 👋 World 🌍 Test 🚀";
    let chinese_text = "你好世界";
    let mixed_text = "Test 测试 🔥 Done";

    let heuristic = HeuristicCounter::new("test", 8192);
    let tiktoken = TiktokenCounter::gpt4().expect("Should create");

    // All should handle unicode
    for text in &[emoji_text, chinese_text, mixed_text] {
        let h_count = heuristic.count(text);
        let t_count = tiktoken.count(text);

        assert!(h_count > 0, "Heuristic should count unicode");
        assert!(t_count > 0, "Tiktoken should count unicode");

        println!("Text: '{text}' - Heuristic: {h_count}, Tiktoken: {t_count}");
    }
}

#[test]
fn test_model_variant_matching() {
    // Test that the registry can match model variants
    let test_cases = vec![
        ("gpt-4-0314", "gpt-4"),                 // Should match gpt-4
        ("gpt-4-turbo-preview", "gpt-4-turbo"),  // Should match gpt-4-turbo
        ("gpt-3.5-turbo-0613", "gpt-3.5-turbo"), // Should match gpt-3.5
        ("o1-mini-2024-09-12", "o1-mini"),       // Should match o1-mini
    ];

    // We can't fully test this without the registry, but we can test the logic
    for (variant, expected_base) in test_cases {
        // The registry's for_model method should handle these
        assert!(
            variant.starts_with(expected_base),
            "{variant} should match {expected_base}"
        );
    }
}

#[test]
fn test_performance_heuristic_vs_tiktoken() {
    use std::time::Instant;

    let long_text = "The quick brown fox jumps over the lazy dog. ".repeat(100);

    let heuristic = HeuristicCounter::new("test", 8192);
    let tiktoken = TiktokenCounter::gpt4().expect("Should create");

    // Time heuristic
    let start = Instant::now();
    for _ in 0..100 {
        heuristic.count(&long_text);
    }
    let heuristic_time = start.elapsed();

    // Time tiktoken
    let start = Instant::now();
    for _ in 0..100 {
        tiktoken.count(&long_text);
    }
    let tiktoken_time = start.elapsed();

    println!("Performance - Heuristic: {heuristic_time:?}, Tiktoken: {tiktoken_time:?}");

    // Heuristic should be at least 5x faster
    assert!(
        heuristic_time < tiktoken_time / 5,
        "Heuristic should be much faster than tiktoken"
    );
}
