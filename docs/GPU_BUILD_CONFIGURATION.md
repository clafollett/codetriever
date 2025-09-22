# GPU Build Configuration

This document explains how Codetriever handles GPU acceleration for embeddings while maintaining CI/CD compatibility.

## The Problem

- **Local Development**: Needs GPU acceleration (Metal on macOS, CUDA on Linux/Windows) for fast embeddings
- **CI/GitHub Actions**: Cannot compile CUDA code (no NVIDIA toolchain), needs CPU-only builds
- **Binary Distribution**: Need to support both CPU-only and GPU-accelerated variants

## The Solution

### 1. Feature-Based GPU Support

`crates/codetriever-embeddings/Cargo.toml`:
```toml
[features]
cuda = ["candle-core/cuda", "candle-nn/cuda", "candle-transformers/cuda"]
metal = ["candle-core/metal", "candle-nn/metal", "candle-transformers/metal"]
```

### 2. Build Script Auto-Detection

`crates/codetriever-embeddings/build.rs`:
- Checks `CODETRIEVER_NO_GPU` environment variable
- If set to "1", disables all GPU features (for CI)
- Otherwise, auto-detects platform and enables appropriate GPU support
- On macOS: Enables Metal
- On Linux/Windows: Enables CUDA if detected

### 3. CI Configuration

`.github/workflows/ci.yml`:
```yaml
env:
  CODETRIEVER_NO_GPU: 1  # Disable GPU features in CI
```

### 4. Local Development

`.cargo/config.toml` provides convenient aliases:
```bash
cargo gpu-build  # Build with GPU acceleration
cargo cpu-build  # Build CPU-only (test CI compatibility)
```

## How It Works

1. **CI/GitHub Actions**:
   - `CODETRIEVER_NO_GPU=1` is set
   - Build script exits early, no GPU features enabled
   - Builds successfully with CPU-only code

2. **Local Development**:
   - No environment variable set
   - Build script detects platform
   - Enables Metal on macOS or CUDA on Linux/Windows
   - Automatic GPU acceleration

3. **Testing**:
   - Unit tests (`cargo test --lib`) use mock providers
   - Integration tests (`cargo test --tests`) can use real models
   - CI only runs unit tests for speed

## Manual Control

```bash
# Force CPU-only build locally
CODETRIEVER_NO_GPU=1 cargo build

# Run tests without GPU
CODETRIEVER_NO_GPU=1 cargo test --lib

# Use just commands
just test-unit        # Fast unit tests only
just test-integration # Slower integration tests
```

## Binary Distribution Strategy

Future releases can provide:
- **CPU variant**: Built with `CODETRIEVER_NO_GPU=1`
- **GPU variant**: Built with platform-specific acceleration
- **Universal**: Runtime detection (future enhancement)

## Debugging

Check which mode is active:
```bash
cargo clean -p codetriever-embeddings
cargo build -p codetriever-embeddings 2>&1 | grep warning
```

Output:
- `GPU acceleration disabled via CODETRIEVER_NO_GPU` - CPU mode
- `Auto-enabled Metal GPU acceleration for macOS` - GPU mode
- `Auto-enabled CUDA GPU acceleration` - GPU mode
- `CUDA not detected, using CPU-only mode` - CPU mode (no CUDA)