# Known Limitations & Honest Warnings

## 🔴 What's Actually Broken Right Now

### MCP Server Untested
- **Issue**: MCP server exists but never tested with Claude
- **Impact**: Can't integrate with Claude Code or other MCP clients yet
- **Fix ETA**: Need community testing
- **Workaround**: Use REST API directly

## ⚠️ Performance Limitations

### Embedding Generation is CPU-Heavy
- **Jina BERT v2** runs locally (no API keys!)
- **CPU-only**: 5-10 seconds per file on Intel/AMD
- **Apple Silicon**: 1-2 seconds per file on M1/M2/M3
- **NVIDIA GPU**: <0.5 seconds per file with CUDA
- **Large repos**: Indexing 10k files takes 30-60 minutes on CPU

### Memory Requirements
- **Minimum**: 8GB RAM (4GB for embeddings model)
- **Recommended**: 16GB+ for smooth operation
- **Model loading**: Initial load takes 2GB RAM
- **Per-file overhead**: ~50MB during embedding generation

### Storage Requirements
- **Qdrant**: ~1GB per 10k code chunks
- **PostgreSQL**: ~100MB per 10k files
- **Embedding models**: 2GB download on first run

## 🚧 Feature Gaps

### CLI Is Incomplete
- Only `mcp` command works
- No `search`, `similar`, `context` commands yet
- Can't configure via CLI flags

### API Endpoints Missing
- `/similar` - Not implemented yet
- `/context` - Not implemented yet
- `/usages` - Not implemented yet
- `/stats` - Not implemented yet
- `/clean` - Not implemented yet
- `/compact` - Not implemented yet

### No File Watching
- Manual re-indexing required
- No git integration for incremental updates
- No automatic detection of changes

## 🐛 Known Bugs

### Docker Required
- Won't work without Docker installed
- PostgreSQL and Qdrant must run in containers
- No embedded database option

### Platform Issues
- **Windows**: Only works in WSL2
- **Linux**: Requires glibc 2.31+ (Ubuntu 20.04+)
- **macOS**: Requires macOS 11+ for ARM64

### Language Support
- Some Tree-sitter parsers may fail on edge cases
- Complex template languages not fully supported
- Binary files cause indexer to skip

## 📝 Missing Documentation

- API endpoint specifications incomplete
- No performance tuning guide
- Limited troubleshooting docs
- No deployment guide for production

## 🔮 Not Yet Implemented

- Multiple embedding models
- Streaming responses
- Batch operations API
- Rate limiting
- Authentication/authorization
- Multi-tenancy
- Distributed indexing
- Cloud storage backends
- Web UI

## 💭 Design Decisions You Might Hate

### Opinionated Chunking
- Fixed chunk size (1024 tokens)
- Overlap strategy might not suit all use cases
- No configuration options yet

### Rust Only
- No Python bindings
- No Node.js SDK
- CLI requires Rust toolchain to build

### Local-First
- No cloud hosting (yet)
- No SaaS option
- You manage your own infrastructure

## 🤝 How to Help

**We know about these issues!** This is a 2-week-old project built by one human and one AI. We're being transparent so you know what you're getting into.

Want to help fix something? Check [CONTRIBUTING.md](CONTRIBUTING.md) and grab an issue!

---

*Last updated: September 21, 2025 - Search API now working*