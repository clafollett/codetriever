//! Integration tests for embedding functionality
//! These tests drive the implementation of real model loading,
//! tokenization, and transformer-based embeddings

use codetriever_embeddings::EmbeddingModel;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::Mutex;

// Type aliases for cleaner code
type SharedModel = Arc<Mutex<EmbeddingModel>>;
type SharedFlag = Arc<Mutex<bool>>;

// Conservative token limit for tests to avoid memory issues
const TEST_MAX_TOKENS: usize = 512;

// Shared model instance - loaded once and reused across all tests
static MODEL: Lazy<SharedModel> = Lazy::new(|| {
    Arc::new(Mutex::new(EmbeddingModel::new(
        "jinaai/jina-embeddings-v2-base-code".to_string(),
        TEST_MAX_TOKENS,
    )))
});

static MODEL_LOADED: Lazy<SharedFlag> = Lazy::new(|| Arc::new(Mutex::new(false)));

async fn get_model() -> Option<Arc<Mutex<EmbeddingModel>>> {
    // Skip if no token
    if std::env::var("HF_TOKEN").is_err() && std::env::var("HUGGING_FACE_HUB_TOKEN").is_err() {
        println!("No HF_TOKEN found, skipping model load");
        return None;
    }

    let mut loaded = MODEL_LOADED.lock().await;
    if !*loaded {
        println!("Loading model for first time (this will be reused for all tests)...");
        let mut model = MODEL.lock().await;
        // Pre-load the model by doing a dummy embed
        match model.embed(&["test"]).await {
            Ok(_) => {
                println!("Model loaded and ready!");
                *loaded = true;
            }
            Err(e) => {
                println!("Failed to load model: {e}");
                return None;
            }
        }
    }

    Some(Arc::clone(&MODEL))
}

#[tokio::test]
async fn test_model_requires_huggingface_token() {
    // This test forces us to implement real model downloading
    let mut model = EmbeddingModel::new(
        "jinaai/jina-embeddings-v2-base-code".to_string(),
        TEST_MAX_TOKENS,
    );

    let test_code = vec!["print('hello world')"];
    let result = model.embed(&test_code).await;

    // If HF_TOKEN is not set, we expect a specific error
    if std::env::var("HF_TOKEN").is_err() && std::env::var("HUGGING_FACE_HUB_TOKEN").is_err() {
        assert!(
            result.is_err(),
            "Should fail without HF_TOKEN or HUGGING_FACE_HUB_TOKEN environment variable"
        );
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("HF_TOKEN")
                || err.to_string().contains("HUGGING_FACE_HUB_TOKEN")
                || err.to_string().contains("authentication"),
            "Error should mention missing token or authentication: {err}"
        );
    }
}

#[tokio::test]
async fn test_embedding_dimensions_match_model() {
    let model = match get_model().await {
        Some(m) => m,
        None => return, // Skip test if no model available
    };

    let texts = vec![
        "fn hello_world() { println!(\"Hello!\"); }",
        "async fn fetch_data() -> Result<String> { Ok(data) }",
    ];

    let mut model = model.lock().await;
    let embeddings = model
        .embed(&texts)
        .await
        .expect("Failed to generate embeddings");

    // Jina v2 base model produces 768-dimensional embeddings
    assert_eq!(embeddings.len(), 2, "Should have 2 embeddings");
    assert_eq!(
        embeddings[0].len(),
        768,
        "Jina model should produce 768-dimensional embeddings"
    );

    // Embeddings should be normalized (unit vectors for cosine similarity)
    for (i, embedding) in embeddings.iter().enumerate() {
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 0.01,
            "Embedding {i} should be normalized, got norm={norm}"
        );
    }
}

#[tokio::test]
async fn test_embeddings_are_deterministic() {
    let model = match get_model().await {
        Some(m) => m,
        None => return, // Skip test if no model available
    };

    let code = vec!["def factorial(n): return 1 if n <= 1 else n * factorial(n-1)"];

    let mut model = model.lock().await;
    let embed1 = model.embed(&code).await.expect("First embedding failed");
    let embed2 = model.embed(&code).await.expect("Second embedding failed");

    // Same input should produce identical output
    assert_eq!(embed1.len(), embed2.len());
    for (v1, v2) in embed1[0].iter().zip(embed2[0].iter()) {
        assert!(
            (v1 - v2).abs() < 1e-6,
            "Embeddings should be deterministic, diff={}",
            (v1 - v2).abs()
        );
    }
}

#[tokio::test]
async fn test_language_agnostic_embeddings() {
    let model = match get_model().await {
        Some(m) => m,
        None => return, // Skip test if no model available
    };

    // Same functionality in different languages
    let multilang_code = vec![
        "def add(a, b): return a + b",                    // Python
        "fn add(a: i32, b: i32) -> i32 { a + b }",        // Rust
        "function add(a, b) { return a + b; }",           // JavaScript
        "public int add(int a, int b) { return a + b; }", // Java
        "func add(a, b int) int { return a + b }",        // Go
    ];

    let mut model = model.lock().await;
    let embeddings = model
        .embed(&multilang_code)
        .await
        .expect("Failed to generate embeddings");

    assert_eq!(embeddings.len(), 5, "Should handle all languages");

    // All embeddings should be 768-dimensional
    for emb in &embeddings {
        assert_eq!(emb.len(), 768);
    }

    // Similar functions across languages should have high similarity
    for i in 0..embeddings.len() {
        for j in i + 1..embeddings.len() {
            let sim = cosine_similarity(&embeddings[i], &embeddings[j]);
            assert!(
                sim > 0.7,
                "Same function in different languages should be similar: {i} vs {j} = {sim}"
            );
        }
    }
}

#[tokio::test]
async fn test_semantic_similarity() {
    let model = match get_model().await {
        Some(m) => m,
        None => return, // Skip test if no model available
    };

    let code = vec![
        // Two sorting functions (similar)
        "def bubble_sort(arr):\n    for i in range(len(arr)):\n        for j in range(len(arr)-1):\n            if arr[j] > arr[j+1]:\n                arr[j], arr[j+1] = arr[j+1], arr[j]",
        "def quick_sort(arr):\n    if len(arr) <= 1: return arr\n    pivot = arr[0]\n    return quick_sort([x for x in arr[1:] if x < pivot]) + [pivot] + quick_sort([x for x in arr[1:] if x >= pivot])",
        // Completely different function
        "def send_email(to, subject, body):\n    smtp = SMTP('localhost')\n    msg = f'Subject: {subject}\\n\\n{body}'\n    smtp.sendmail('from@example.com', to, msg)",
    ];

    let mut model = model.lock().await;
    let embeddings = model
        .embed(&code)
        .await
        .expect("Failed to generate embeddings");

    let sort_similarity = cosine_similarity(&embeddings[0], &embeddings[1]);
    let diff_similarity = cosine_similarity(&embeddings[0], &embeddings[2]);

    println!("Similarity between sorting algorithms: {sort_similarity}");
    println!("Similarity between sort and email: {diff_similarity}");

    assert!(
        sort_similarity > diff_similarity + 0.15,
        "Sorting algorithms should be more similar to each other than to email function"
    );

    assert!(
        sort_similarity > 0.6,
        "Similar algorithms should have decent similarity: {sort_similarity}"
    );

    assert!(
        diff_similarity < 0.5,
        "Different functions should have low similarity: {diff_similarity}"
    );
}

#[tokio::test]
async fn test_handles_truncation() {
    let model = match get_model().await {
        Some(m) => m,
        None => return, // Skip test if no model available
    };

    // Generate code longer than typical 2048 token limit
    let mut long_code = String::from("def process():\n");
    for i in 0..200 {
        long_code.push_str(&format!(
            "    variable_{i} = calculate_value_{i}(input_{i})\n"
        ));
    }

    let mut model = model.lock().await;
    let result = model.embed(&[long_code.as_str()]).await;
    assert!(result.is_ok(), "Should handle long code with truncation");

    let embeddings = result.unwrap();
    assert_eq!(embeddings[0].len(), 768);

    // Should still be normalized despite truncation
    let norm: f32 = embeddings[0].iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 0.01);
}

#[tokio::test]
async fn test_batch_processing() {
    let model = match get_model().await {
        Some(m) => m,
        None => return, // Skip test if no model available
    };

    // Create batch of functions
    let mut batch = Vec::new();
    for i in 0..16 {
        batch.push(format!("def func_{i}(x): return x * {i}"));
    }

    let start = std::time::Instant::now();
    let mut model = model.lock().await;
    let batch_refs: Vec<&str> = batch.iter().map(|s| s.as_str()).collect();
    let embeddings = model
        .embed(&batch_refs)
        .await
        .expect("Batch processing failed");
    let duration = start.elapsed();

    assert_eq!(embeddings.len(), 16);

    // Batch should process efficiently (not 16x single processing time)
    println!("Batch of 16 processed in {duration:?}");
    assert!(
        duration.as_secs() < 30,
        "Batch processing should complete in reasonable time"
    );
}

#[tokio::test]
async fn test_python_rust_comparison() {
    // Skip if no token
    if std::env::var("HF_TOKEN").is_err() && std::env::var("HUGGING_FACE_HUB_TOKEN").is_err() {
        println!("No HF_TOKEN found, skipping test");
        return;
    }

    let mut model = EmbeddingModel::new(
        "jinaai/jina-embeddings-v2-base-code".to_string(),
        TEST_MAX_TOKENS,
    );

    // Same test snippets as Python
    let test_snippets = vec![
        "fn quick",
        "def hello(): print('world')",
        "The cat sits outside",
        "The cat plays in the garden",
        "def quicksort(arr):\n    if len(arr) <= 1:\n        return arr\n    pivot = arr[len(arr) // 2]\n    return quicksort([x for x in arr if x < pivot]) + [pivot] + quicksort([x for x in arr if x > pivot])",
    ];

    println!("{}", "=".repeat(60));
    println!("Rust Embeddings");
    println!("{}", "=".repeat(60));

    let embeddings = model
        .embed(&test_snippets)
        .await
        .expect("Failed to generate embeddings");

    println!("\nRust embedding samples:");
    for (i, (text, emb)) in test_snippets.iter().zip(embeddings.iter()).enumerate() {
        let preview = if text.len() > 30 {
            format!("{}...", &text[..30])
        } else {
            text.to_string()
        };
        println!(
            "{}. '{}' -> First 5 values: {:?}",
            i,
            preview,
            &emb[..5.min(emb.len())]
        );
    }

    println!("\nRust similarities:");
    for i in 0..test_snippets.len() {
        for j in (i + 1)..test_snippets.len() {
            let sim = cosine_similarity(&embeddings[i], &embeddings[j]);
            let preview_i = if test_snippets[i].len() > 20 {
                format!("{}...", &test_snippets[i][..20])
            } else {
                test_snippets[i].to_string()
            };
            let preview_j = if test_snippets[j].len() > 20 {
                format!("{}...", &test_snippets[j][..20])
            } else {
                test_snippets[j].to_string()
            };
            println!("  {i} vs {j}: {sim:.4} ('{preview_i}' vs '{preview_j}')");
        }
    }

    // Python baseline embeddings (first 10 values for each snippet)
    // Generated using transformers with jinaai/jina-embeddings-v2-base-code
    let python_baseline_first_10 = [
        vec![
            -0.0281692f32,
            0.00116711f32,
            0.00292678f32,
            0.0478523f32,
            0.0229456f32,
            0.0473319f32,
            -0.0302278f32,
            -0.0522278f32,
            0.0159007f32,
            -0.0468331f32,
        ],
        vec![
            -0.0017279f32,
            -0.0530346f32,
            0.008921f32,
            -0.00968804f32,
            0.00783761f32,
            0.00373467f32,
            -0.0265722f32,
            0.0844228f32,
            -0.0157261f32,
            -0.0432881f32,
        ],
        vec![
            -0.0112376f32,
            0.0185604f32,
            0.0167898f32,
            0.0234089f32,
            0.0484961f32,
            0.0143904f32,
            0.00112754f32,
            0.0159319f32,
            0.04773f32,
            -0.0359343f32,
        ],
        vec![
            0.0127664f32,
            -0.0425533f32,
            -0.0466576f32,
            0.0207423f32,
            0.0512469f32,
            0.00575073f32,
            -0.0122236f32,
            -0.0128945f32,
            0.098394f32,
            -0.00604995f32,
        ],
        vec![
            -0.09426f32,
            0.0179691f32,
            -0.0431347f32,
            0.0340008f32,
            0.0195649f32,
            0.0391622f32,
            0.0361033f32,
            -0.0218162f32,
            -0.0198587f32,
            -0.0357688f32,
        ],
    ];

    // Python baseline similarities
    let python_similarities = [
        0.417779f32,  // 0 vs 1: fn quick vs def hello
        0.282907f32,  // 0 vs 2: fn quick vs cat sits
        0.244089f32,  // 0 vs 3: fn quick vs cat plays
        0.458354f32,  // 0 vs 4: fn quick vs quicksort
        0.33456f32,   // 1 vs 2: def hello vs cat sits
        0.189756f32,  // 1 vs 3: def hello vs cat plays
        0.0756423f32, // 1 vs 4: def hello vs quicksort
        0.660909f32,  // 2 vs 3: cat sentences
        0.0542142f32, // 2 vs 4: cat sits vs quicksort
        0.0238972f32, // 3 vs 4: cat plays vs quicksort
    ];

    println!("\n{}", "=".repeat(60));
    println!("Comparing with Python baseline:");
    println!("{}", "=".repeat(60));

    // Compare embeddings (first 10 values)
    println!("\nEmbedding comparison (first 10 values):");
    for i in 0..test_snippets.len() {
        let rust_first_10 = &embeddings[i][..10.min(embeddings[i].len())];
        let python_first_10 = &python_baseline_first_10[i];

        // Calculate difference
        let max_diff = rust_first_10
            .iter()
            .zip(python_first_10.iter())
            .map(|(r, p)| (r - p).abs())
            .fold(0.0f32, f32::max);

        println!("  Snippet {i}: max diff = {max_diff:.6}");
        if max_diff > 0.1 {
            println!("    WARNING: Large difference detected!");
            println!("    Rust:   {:?}", &rust_first_10[..5]);
            println!("    Python: {:?}", &python_first_10[..5]);
        }
    }

    // Compare similarities
    println!("\nSimilarity comparison:");
    let mut sim_idx = 0;
    for i in 0..test_snippets.len() {
        for j in (i + 1)..test_snippets.len() {
            let rust_sim = cosine_similarity(&embeddings[i], &embeddings[j]);
            let python_sim = python_similarities[sim_idx];
            let diff = (rust_sim - python_sim).abs();

            println!("  {i} vs {j}: Rust={rust_sim:.4}, Python={python_sim:.4}, diff={diff:.4}");

            // Assert similarity is within tolerance
            assert!(
                diff < 0.15,
                "Similarity {i} vs {j} differs too much: Rust={rust_sim:.4}, Python={python_sim:.4}, diff={diff:.4}"
            );
            sim_idx += 1;
        }
    }

    // Special assertion for cat sentences
    let cat_sim = cosine_similarity(&embeddings[2], &embeddings[3]);
    println!(
        "\nCat sentences similarity: {:.4} (Python: {:.4})",
        cat_sim, 0.660909
    );
    assert!(cat_sim > 0.5, "Cat sentences should be similar");
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm_a * norm_b)
}
