<!-- IMPLEMENTATION STATUS: ✅ IMPLEMENTED via Candle device detection
- Metal acceleration: Working via candle-core automatic device selection
- Dual-mode approach: Implicit (Candle auto-detects Metal on Mac, falls back to CPU in Docker)
- Current limitation: Docker on Mac cannot access Metal GPU (Linux VM barrier)
- Reality: Simplified from plan—no separate configs, Candle handles it automatically
-->

# Codetriever Development Strategy

**Native Mac development for speed, Docker deployment for portability**

## Overview

We use a dual-mode approach:
- **Development**: Native on Mac with Metal GPU acceleration
- **Deployment**: Docker containers for consistency across environments

This gives us the best of both worlds - fast iteration during development and reliable deployment everywhere.

## Why This Approach?

### The Mac GPU Problem
- Docker on Mac runs in a Linux VM - no Metal API access
- MoltenVK/Vulkan passthrough exists but adds ~20% overhead and complexity
- Native development gives us full Metal acceleration

### The Solution
- Develop natively on Mac for maximum performance
- Test in Docker containers locally
- Deploy via Docker for production consistency

## Development Setup (Native Mac)

### Prerequisites

```bash
# Install native dependencies
brew install qdrant       # Vector database
brew install protobuf     # For gRPC support

# Optional: Rust with Metal support
rustup update
```

### Running Locally

```bash
# 1. Start Qdrant (native)
qdrant  # Runs on localhost:6333 (gRPC) and :6334 (HTTP)

# 2. Run API server (native with Metal)
cd crates/codetriever-api
cargo run

# 3. Run MCP/CLI (native)
cd crates/codetriever
cargo run -- serve --mcp
```

### Environment Configuration

Create `.env.development`:
```env
# Native Mac development
EMBEDDING_BACKEND=native
QDRANT_URL=http://localhost:6334
USE_METAL=true
EMBEDDING_MODEL=jina-embeddings-v2-base-code
LOG_LEVEL=debug
```

## Architecture for Dual Mode

### Embedding Abstraction

```rust
// src/embedding/mod.rs
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

// Native Mac with Metal
#[cfg(target_os = "macos")]
pub struct MetalEmbedder {
    model: candle::Model,
    device: Device::Metal(0),
}

// CPU for Docker
pub struct CpuEmbedder {
    model: ort::Session,  // ONNX Runtime
}

// Remote service option
pub struct RemoteEmbedder {
    client: reqwest::Client,
    endpoint: String,
}
```

### Auto-Detection Logic

```rust
pub fn create_embedder() -> Box<dyn Embedder> {
    // Check environment and capabilities
    if cfg!(target_os = "macos") && env::var("USE_METAL").is_ok() {
        info!("Using Metal-accelerated embeddings");
        Box::new(MetalEmbedder::new())
    } else if let Ok(url) = env::var("EMBEDDING_SERVICE_URL") {
        info!("Using remote embedding service at {}", url);
        Box::new(RemoteEmbedder::new(url))
    } else {
        info!("Using CPU embeddings");
        Box::new(CpuEmbedder::new())
    }
}
```

## Docker Deployment

### Production Configuration

Create `.env.docker`:
```env
# Docker deployment
EMBEDDING_BACKEND=cpu
QDRANT_URL=http://qdrant:6334
USE_METAL=false
EMBEDDING_MODEL=all-MiniLM-L6-v2  # Smaller for CPU
LOG_LEVEL=info
```

### Docker Compose

```yaml
# docker-compose.yml
version: '3.8'

services:
  api:
    build: 
      context: .
      dockerfile: docker/Dockerfile.api
    ports:
      - "8080:8080"
    environment:
      - EMBEDDING_BACKEND=${EMBEDDING_BACKEND:-cpu}
      - QDRANT_URL=${QDRANT_URL:-http://qdrant:6334}
    depends_on:
      - qdrant
    
  qdrant:
    image: qdrant/qdrant:latest
    ports:
      - "6333:6333"
      - "6334:6334"
    volumes:
      - ./data/qdrant:/qdrant/storage
```

### Testing Docker Locally

```bash
# Build and test Docker setup
docker-compose build
docker-compose up

# Or use the Makefile
make docker-test
```

## Makefile for Convenience

```makefile
# Makefile
.PHONY: dev docker-test clean

# Native development
dev:
	@echo "Starting native development environment..."
	@qdrant > /tmp/qdrant.log 2>&1 &
	@cd crates/codetriever-api && cargo run &
	@cd crates/codetriever && cargo run -- serve --mcp

# Test Docker locally
docker-test:
	@echo "Testing Docker deployment..."
	docker-compose build
	docker-compose up

# Clean everything
clean:
	@pkill qdrant || true
	@pkill codetriever || true
	docker-compose down -v
	cargo clean
```

## Model Selection by Environment

### Development (Mac Native)
- **Model**: jina-embeddings-v2-base-code (161M params)
- **Backend**: Candle with Metal
- **Performance**: ~2000 embeddings/sec
- **Memory**: ~320MB

### Docker (CPU)
- **Model**: all-MiniLM-L6-v2 (22M params)
- **Backend**: ONNX Runtime
- **Performance**: ~1000 embeddings/sec
- **Memory**: <100MB

### Future Docker (GPU)
- **Model**: jina-embeddings-v2-base-code
- **Backend**: ONNX with CUDA/ROCm
- **Performance**: ~3000 embeddings/sec
- **Memory**: ~500MB

## Performance Comparison

| Environment | Model | Backend | Speed | Memory |
|------------|-------|---------|-------|--------|
| Mac Native | Jina-v2-base | Metal | 2000/s | 320MB |
| Docker CPU | MiniLM-L6 | ONNX | 1000/s | 100MB |
| Docker GPU | Jina-v2-base | CUDA | 3000/s | 500MB |

## CI/CD Pipeline

```yaml
# .github/workflows/ci.yml
name: CI

on: [push, pull_request]

jobs:
  test-native:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --all
      - run: cargo build --release
      
  test-docker:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: docker-compose build
      - run: docker-compose run api cargo test
      
  benchmark:
    runs-on: macos-latest
    if: github.ref == 'refs/heads/main'
    steps:
      - uses: actions/checkout@v4
      - run: cargo bench
```

## Migration Path

### Phase 1: Native Development (Week 1)
- Mac-only development
- Jina-v2-base-code with Metal
- Native Qdrant
- Fast iteration

### Phase 2: Docker Testing (Week 2)
- Add Docker configs
- CPU-only embeddings
- Test deployment locally
- Verify portability

### Phase 3: Production (Week 3+)
- Choose deployment target (cloud/on-prem)
- Optimize model selection
- Add GPU support if available
- Scale horizontally

## Troubleshooting

### Issue: Slow embeddings in Docker
**Solution**: Switch to smaller model (MiniLM) or add GPU support

### Issue: Can't connect to Qdrant
**Solution**: Check if native Qdrant is running (`ps aux | grep qdrant`)

### Issue: Metal not detected
**Solution**: Ensure Xcode command line tools installed (`xcode-select --install`)

### Issue: Docker build fails
**Solution**: Clear Docker cache (`docker system prune -a`)

## Key Decisions

1. **Why native development?**
   - 10x faster iteration
   - Full GPU acceleration
   - No virtualization overhead

2. **Why Docker deployment?**
   - Consistent across environments
   - Easy cloud deployment
   - Standard DevOps practices

3. **Why abstract embeddings?**
   - Swap backends without code changes
   - Optimize per environment
   - Future-proof architecture

## Next Steps

1. Create `codetriever-api` crate with embedding abstraction
2. Set up native Qdrant for development
3. Implement Metal-accelerated embeddings
4. Create Docker configs for deployment
5. Test both modes end-to-end