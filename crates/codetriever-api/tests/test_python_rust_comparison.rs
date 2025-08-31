//! Compare Rust embeddings with Python embeddings
//!
//! This test ensures our Rust implementation matches the Python baseline.
//! The baseline values are generated from the official HuggingFace transformers
//! implementation using the exact same input texts.

use codetriever_api::embedding::EmbeddingModel;

#[tokio::test]
async fn test_python_rust_comparison() {
    // Skip if no token
    if std::env::var("HF_TOKEN").is_err() && std::env::var("HUGGING_FACE_HUB_TOKEN").is_err() {
        println!("No HF_TOKEN found, skipping test");
        return;
    }

    let mut model = EmbeddingModel::new("jinaai/jina-embeddings-v2-base-code".to_string());

    // Same test snippets as Python
    let test_snippets = vec![
        "fn quick".to_string(),
        "def hello(): print('world')".to_string(),
        "The cat sits outside".to_string(),
        "The cat plays in the garden".to_string(),
        "def quicksort(arr):\n    if len(arr) <= 1:\n        return arr\n    pivot = arr[len(arr) // 2]\n    return quicksort([x for x in arr if x < pivot]) + [pivot] + quicksort([x for x in arr if x > pivot])".to_string(),
    ];

    println!("{}", "=".repeat(60));
    println!("Rust Embeddings");
    println!("{}", "=".repeat(60));

    let embeddings = model
        .embed(test_snippets.clone())
        .await
        .expect("Failed to generate embeddings");

    println!("\nRust embedding samples:");
    for (i, (text, emb)) in test_snippets.iter().zip(embeddings.iter()).enumerate() {
        let preview = if text.len() > 30 {
            format!("{}...", &text[..30])
        } else {
            text.clone()
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
                test_snippets[i].clone()
            };
            let preview_j = if test_snippets[j].len() > 20 {
                format!("{}...", &test_snippets[j][..20])
            } else {
                test_snippets[j].clone()
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
