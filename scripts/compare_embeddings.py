#!/usr/bin/env python3
"""Compare Python and Rust embeddings for the same code snippets."""

import os
import numpy as np
from transformers import AutoModel, AutoTokenizer
import torch

# Test code snippets
TEST_SNIPPETS = [
    "fn quick",  # Simple Rust function
    "def hello(): print('world')",  # Python function
    "The cat sits outside",  # Natural language
    "The cat plays in the garden",  # Similar to above
    """def quicksort(arr):
    if len(arr) <= 1:
        return arr
    pivot = arr[len(arr) // 2]
    return quicksort([x for x in arr if x < pivot]) + [pivot] + quicksort([x for x in arr if x > pivot])""",  # Complex code
]

def get_python_embeddings():
    """Get embeddings using Python/HuggingFace."""
    print("Loading Python model...")
    model = AutoModel.from_pretrained(
        'jinaai/jina-embeddings-v2-base-code',
        trust_remote_code=True
    )
    tokenizer = AutoTokenizer.from_pretrained(
        'jinaai/jina-embeddings-v2-base-code',
        trust_remote_code=True
    )
    
    model.eval()
    embeddings = []
    
    with torch.no_grad():
        for text in TEST_SNIPPETS:
            print(f"  Processing: {text[:50]}...")
            inputs = tokenizer(text, return_tensors='pt', padding=True, truncation=True, max_length=512)
            outputs = model(**inputs)
            
            # Mean pooling with attention mask
            attention_mask = inputs['attention_mask']
            token_embeddings = outputs.last_hidden_state
            input_mask_expanded = attention_mask.unsqueeze(-1).expand(token_embeddings.size()).float()
            sum_embeddings = torch.sum(token_embeddings * input_mask_expanded, 1)
            sum_mask = torch.clamp(input_mask_expanded.sum(1), min=1e-9)
            mean_pooled = sum_embeddings / sum_mask
            
            # Normalize
            normalized = torch.nn.functional.normalize(mean_pooled, p=2, dim=1)
            embeddings.append(normalized[0].numpy())
    
    return embeddings

def cosine_similarity(a, b):
    """Calculate cosine similarity."""
    return np.dot(a, b) / (np.linalg.norm(a) * np.linalg.norm(b))

def main():
    print("=" * 60)
    print("Python Embeddings")
    print("=" * 60)
    
    python_embeddings = get_python_embeddings()
    
    print("\nPython embedding samples:")
    for i, (text, emb) in enumerate(zip(TEST_SNIPPETS, python_embeddings)):
        print(f"{i}. '{text[:30]}...' -> First 5 values: {emb[:5]}")
    
    print("\nPython similarities:")
    for i in range(len(TEST_SNIPPETS)):
        for j in range(i + 1, len(TEST_SNIPPETS)):
            sim = cosine_similarity(python_embeddings[i], python_embeddings[j])
            print(f"  {i} vs {j}: {sim:.4f} ('{TEST_SNIPPETS[i][:20]}...' vs '{TEST_SNIPPETS[j][:20]}...')")
    
    print("\n" + "=" * 60)
    print("Saving test snippets for Rust...")
    print("=" * 60)
    
    # Save snippets for Rust test
    with open('/tmp/test_snippets.txt', 'w') as f:
        for snippet in TEST_SNIPPETS:
            # Escape newlines for single-line format
            escaped = snippet.replace('\n', '\\n')
            f.write(escaped + '\n')
    
    print("Test snippets saved to /tmp/test_snippets.txt")
    print("\nNow run the Rust comparison with:")
    print("  cargo test test_python_rust_comparison -- --nocapture")

if __name__ == "__main__":
    main()