//! Integration tests for embedding functionality
//! These tests drive the implementation of real model loading,
//! tokenization, and transformer-based embeddings

use codetriever_api::embedding::EmbeddingModel;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::Mutex;

// Shared model instance - loaded once and reused across all tests
static MODEL: Lazy<Arc<Mutex<EmbeddingModel>>> = Lazy::new(|| {
    Arc::new(Mutex::new(EmbeddingModel::new(
        "jinaai/jina-embeddings-v2-base-code".to_string(),
    )))
});

static MODEL_LOADED: Lazy<Arc<Mutex<bool>>> = Lazy::new(|| Arc::new(Mutex::new(false)));

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
        match model.embed(vec!["test".to_string()]).await {
            Ok(_) => {
                println!("Model loaded and ready!");
                *loaded = true;
            }
            Err(e) => {
                println!("Failed to load model: {}", e);
                return None;
            }
        }
    }

    Some(Arc::clone(&MODEL))
}

#[tokio::test]
async fn test_model_requires_huggingface_token() {
    // This test forces us to implement real model downloading
    let mut model = EmbeddingModel::new("jinaai/jina-embeddings-v2-base-code".to_string());

    let test_code = vec!["print('hello world')".to_string()];
    let result = model.embed(test_code).await;

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
            "Error should mention missing token or authentication: {}",
            err
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
        "fn hello_world() { println!(\"Hello!\"); }".to_string(),
        "async fn fetch_data() -> Result<String> { Ok(data) }".to_string(),
    ];

    let mut model = model.lock().await;
    let embeddings = model
        .embed(texts)
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
            "Embedding {} should be normalized, got norm={}",
            i,
            norm
        );
    }
}

#[tokio::test]
async fn test_embeddings_are_deterministic() {
    let model = match get_model().await {
        Some(m) => m,
        None => return, // Skip test if no model available
    };

    let code = vec!["def factorial(n): return 1 if n <= 1 else n * factorial(n-1)".to_string()];

    let mut model = model.lock().await;
    let embed1 = model
        .embed(code.clone())
        .await
        .expect("First embedding failed");
    let embed2 = model
        .embed(code.clone())
        .await
        .expect("Second embedding failed");

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
        "def add(a, b): return a + b".to_string(), // Python
        "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(), // Rust
        "function add(a, b) { return a + b; }".to_string(), // JavaScript
        "public int add(int a, int b) { return a + b; }".to_string(), // Java
        "func add(a, b int) int { return a + b }".to_string(), // Go
    ];

    let mut model = model.lock().await;
    let embeddings = model
        .embed(multilang_code)
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
                "Same function in different languages should be similar: {} vs {} = {}",
                i,
                j,
                sim
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
        "def bubble_sort(arr):\n    for i in range(len(arr)):\n        for j in range(len(arr)-1):\n            if arr[j] > arr[j+1]:\n                arr[j], arr[j+1] = arr[j+1], arr[j]".to_string(),
        "def quick_sort(arr):\n    if len(arr) <= 1: return arr\n    pivot = arr[0]\n    return quick_sort([x for x in arr[1:] if x < pivot]) + [pivot] + quick_sort([x for x in arr[1:] if x >= pivot])".to_string(),
        // Completely different function
        "def send_email(to, subject, body):\n    smtp = SMTP('localhost')\n    msg = f'Subject: {subject}\\n\\n{body}'\n    smtp.sendmail('from@example.com', to, msg)".to_string(),
    ];

    let mut model = model.lock().await;
    let embeddings = model
        .embed(code)
        .await
        .expect("Failed to generate embeddings");

    let sort_similarity = cosine_similarity(&embeddings[0], &embeddings[1]);
    let diff_similarity = cosine_similarity(&embeddings[0], &embeddings[2]);

    println!("Similarity between sorting algorithms: {}", sort_similarity);
    println!("Similarity between sort and email: {}", diff_similarity);

    assert!(
        sort_similarity > diff_similarity + 0.15,
        "Sorting algorithms should be more similar to each other than to email function"
    );

    assert!(
        sort_similarity > 0.6,
        "Similar algorithms should have decent similarity: {}",
        sort_similarity
    );

    assert!(
        diff_similarity < 0.5,
        "Different functions should have low similarity: {}",
        diff_similarity
    );
}

#[tokio::test]
async fn test_handles_truncation() {
    let model = match get_model().await {
        Some(m) => m,
        None => return, // Skip test if no model available
    };

    // Generate code longer than typical 512 token limit
    let mut long_code = String::from("def process():\n");
    for i in 0..200 {
        long_code.push_str(&format!(
            "    variable_{} = calculate_value_{}(input_{})\n",
            i, i, i
        ));
    }

    let mut model = model.lock().await;
    let result = model.embed(vec![long_code]).await;
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
        batch.push(format!("def func_{}(x): return x * {}", i, i));
    }

    let start = std::time::Instant::now();
    let mut model = model.lock().await;
    let embeddings = model
        .embed(batch.clone())
        .await
        .expect("Batch processing failed");
    let duration = start.elapsed();

    assert_eq!(embeddings.len(), 16);

    // Batch should process efficiently (not 16x single processing time)
    println!("Batch of 16 processed in {:?}", duration);
    assert!(
        duration.as_secs() < 30,
        "Batch processing should complete in reasonable time"
    );
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm_a * norm_b)
}
