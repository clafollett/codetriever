# Test Repositories

This directory contains git submodules of popular open-source projects used for testing and benchmarking the Codetriever indexing and search functionality.

## Repository List

### Rust
- **rust-mini-redis** - Tokio's mini Redis implementation (small-medium, ~5k LOC)

### Go  
- **go-examples** - Official Go example programs (small, varied)

### Python
- **python-algorithms** - Educational algorithms repository (large, ~100k+ LOC)

### JavaScript
- **javascript-lodash** - Popular utility library (medium, ~20k LOC)
- **javascript-express** - Node.js web framework (medium, ~15k LOC)

### C#
- **csharp-newtonsoft-json** - JSON.NET library (medium-large, ~50k LOC)
- **csharp-entity-framework** - Microsoft's ORM (very large, ~500k+ LOC)
- **csharp-serilog** - Structured logging library (small-medium, ~10k LOC)

## Setup

To initialize these test repositories efficiently (shallow clone):
```bash
cd test-repos
./init.sh
```

This will save significant bandwidth and disk space by only fetching the specific commits needed.

## Usage

These repositories are used for:
1. Integration testing of the indexer across different languages
2. Performance benchmarking of embedding generation
3. Search quality testing with real-world code
4. Load testing Qdrant with various codebase sizes

## Updating

Submodules are pinned to specific commits for reproducibility. To update:
```bash
cd test-repos/<repo-name>
git pull origin main
git checkout <specific-commit>
cd ../..
git add test-repos/<repo-name>
git commit -m "Update test repo to new commit"
```

## Note

These repos are excluded from the main build process and are only used for testing purposes.