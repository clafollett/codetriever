# 🔍 Codetriever

**Semantic code search for every AI coding agent. Built in Rust. MCP-native.**

> Give your AI perfect memory of your entire codebase - built in 2 weeks during dog walks 🐕

## 🚀 What's This?

Codetriever is a semantic code search engine with (untested) MCP support. Built in Rust in 2 weeks as an experiment. The goal is to give AI agents memory of codebases through the [Model Context Protocol](https://modelcontextprotocol.io).

```bash
# These commands exist but may not work:
codetriever mcp  # MCP server (untested, may crash)

# These commands don't exist yet:
codetriever index /path/to/repo
codetriever search "database connection pooling logic"
```

## 🎯 The Problem We Solve

- **Context windows overflow** - Even 200k tokens can't fit real codebases
- **AI agents forget** - They can't see your whole project structure
- **Keyword search fails** - "auth logic" should find authentication code regardless of naming
- **Cloud is risky** - Your proprietary code shouldn't leave your machine

## 🔥 Why This Matters

Every AI coding tool needs semantic search. We're building the **open protocol** that powers them all through MCP. Not locked to Claude, Copilot, or Cursor - works with everything.

## 📊 Current Status - Week 3 of Development

### ✅ What Actually Works
- **Indexing pipeline** - Parse, chunk, embed, store your codebase
- **Tree-sitter parsing** - Semantic understanding of 25+ languages
- **Smart chunking** - Respects token limits, preserves context
- **Vector storage** - Qdrant for embeddings, PostgreSQL for metadata
- **Database tracking** - Knows what files have been indexed

### 🤷 What Might Work (Untested)
- **MCP server** - Agenterra scaffolded it, never tested with Claude
- **Incremental updates** - Code exists, not proven

### 🚧 Coming Soon
- **Remaining API endpoints**
  - /search - Semantic code search (THE broken one)
  - /similar - Find similar code chunks
  - /context - Get surrounding code context
  - /usages - Find symbol usages
  - /index - Trigger reindexing
  - /status - System health and metrics
  - /stats - Quick statistics
  - /clean - Remove stale entries
  - /compact - Optimize database
- **CLI commands** - Direct terminal access to all features
- **Similar code finder** - Find code patterns across your codebase
- **Usage finder** - Track where symbols are used

### ⚠️ Limitations

See [LIMITATIONS.md](LIMITATIONS.md) for known issues, hardware requirements, and missing features.

## 🏗️ Architecture

```
Your Code → Tree-sitter Parser → Semantic Chunks → Vector Embeddings → Qdrant
     ↑                                                                    ↓
File Tracking ←────────────────────────────────────────────────────→ Search
(PostgreSQL)                                                        (MCP/API)
```

## 💻 System Requirements

### Minimum
- **RAM**: 8GB (4GB available for embeddings)
- **CPU**: Any x64 or ARM64 processor
- **Disk**: 2GB for models + space for index
- **OS**: macOS, Linux, Windows (WSL2)

### Recommended
- **RAM**: 16GB+
- **CPU**: Apple Silicon (M1/M2/M3) or modern x64 with AVX
- **GPU**: NVIDIA with CUDA (optional but 10x faster)
- **Disk**: SSD with 10GB+ free

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Just (command runner)
cargo install just
# or on macOS
brew install just

# Docker (for PostgreSQL and Qdrant)
# Install Docker Desktop from https://www.docker.com/products/docker-desktop
```

### From Source

```bash
git clone https://github.com/clafollett/codetriever
cd codetriever

# Setup development environment
source stack.env
just dev-setup

# Build and install
cargo install --path crates/codetriever
cargo install --path crates/codetriever-api
```

## Quick Start (What Actually Works)

```bash
# Initialize Docker services and database
just init

# Start API server
codetriever-api

# Index via API (requires file CONTENT, not filesystem paths - SaaS-ready!)
# Path should be repo-relative (e.g., "src/main.rs" not "/Users/bob/code/project/src/main.rs")
curl -X POST http://localhost:3000/index \
  -H "Content-Type: application/json" \
  -d '{
    "project_id": "my-project",
    "files": [
      {
        "path": "src/main.rs",
        "content": "fn main() { println!(\"Hello\"); }",
        "hash": "abc123"
      }
    ]
  }'

# Search via API (returns empty array - BROKEN)
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query": "database migrations", "limit": 10}'

# MCP server (exists but untested, probably broken)
codetriever mcp
```

## Development

### Key Commands

```bash
# Initial setup
just dev-setup        # Install dependencies and setup environment
source stack.env      # Load development environment

# Infrastructure (Docker services)
just init            # Initialize Docker services and database
just docker-up       # Start PostgreSQL and Qdrant
just docker-down     # Stop all containers  
just docker-reset    # Clean reset of Docker environment
just docker-logs     # View service logs

# Database
just db-setup        # Initialize database schema
just db-migrate      # Run migrations
just db-reset        # Drop and recreate database

# Development workflow
just test            # Run all tests
just test-unit       # Run unit tests only (fast)
just test-integration # Run integration tests
just fmt             # Format code
just lint            # Run clippy lints
just clippy-fix      # Fix clippy warnings
just check           # Run all quality checks (fmt + lint + test)
just watch           # Watch mode for development

# Building & Running
just build           # Build debug version
just build-release   # Build optimized release
just run [args]      # Run CLI with arguments
just api             # Run API server

# Utility
just clean           # Clean build artifacts
just docs            # Generate and open documentation
just stats           # Show project statistics
just update          # Update dependencies
just audit           # Security audit
just clean-test-data # Clean Qdrant test collections
```

### Common Workflows

```bash
# Quick setup for new contributors
just quick-start     # Runs init + test

# Full CI pipeline locally
just ci              # Runs fmt + lint + test + build

# Fix all auto-fixable issues
just fix             # Runs fmt + clippy-fix

# Development mode with auto-reload
just dev             # Starts Docker and watches API
```

### Test Commands

```bash
# Run all tests
just test

# Run unit tests only (faster)
just test-unit

# Run integration tests
just test-integration

# Run specific crate tests
cargo test -p codetriever-indexer
cargo test -p codetriever-data
```

## Testing Infrastructure

- **Unit Tests** - Comprehensive coverage with mocks
- **Integration Tests** - Full stack testing with real components
- **Token Counter Tests** - Accuracy and performance validation
- **Byte Offset Tests** - Proper position tracking

## Key Design Decisions

- **Trait-based abstractions** - `VectorStorage`, `EmbeddingService`, `ContentParser`, `TokenCounter`
- **Token-aware chunking** - Respects model context limits
- **Deterministic chunk IDs** - UUID v5 based on content for stability
- **Incremental indexing** - Git-aware change detection
- **Modular architecture** - Clean separation of concerns

## Status

🚧 **Alpha** - Core functionality is working, API stabilizing.

### Completed
- [x] Modular crate structure
- [x] Tree-sitter parsing for 25+ languages
- [x] Vector storage with Qdrant
- [x] PostgreSQL state management
- [x] Token counting abstractions (Tiktoken, Heuristic)
- [x] Smart chunking service
- [x] REST API with Axum
- [x] Incremental indexing
- [x] Comprehensive test suite

### In Progress
- [ ] Performance optimization
- [ ] CLI improvements
- [ ] Documentation
- [ ] MCP server implementation

### Planned
- [ ] Git integration for history
- [ ] Multiple embedding models
- [ ] Web UI
- [ ] Language-specific improvements

## Architecture Documentation

See [docs/architecture/current-architecture.md](docs/architecture/current-architecture.md) for detailed system design.

## 🤝 Contributing - We Need You!

**The search endpoint is literally broken.** Want to be the hero who fixes it? 🦸

### Quick Wins for First Contributors
- [ ] Fix the search endpoint to return actual results
- [ ] Add CLI commands for search/similar/context
- [ ] **Upgrade to NEW Jina code models** (released Sept 3, 2025! 0.5b/1.5b/GGUF versions)
- [ ] Improve error messages
- [ ] Add more language tests
- [ ] Write documentation

### How to Contribute
1. **Fork and clone** the repository
2. **Pick a TODO** from the codebase (they're everywhere!)
3. **Write tests first** - We use TDD (Red/Green/Refactor)
4. **Run checks** - `just test && just clippy-fix`
5. **Submit a PR** - We review fast!

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup details.

**First PR merged gets a shoutout in the README!** 🎉

## 📖 The Origin Story

**Week 1**: Human architect brainstorms during dog walks, chatting with Claude mobile. "What if AI agents could semantically search codebases?"

**Friday Aug 30, 2025**: First commit at 2:36 PM EDT. Human designs, AI codes. Perfect pair programming.

**Labor Day Weekend**: Marathon coding session. Tree-sitter parsing, embeddings, vector storage. Human guides architecture, AI implements. No sleep, pure flow state.

**Week 2**: PostgreSQL state management, MCP server (via our Agenterra tool), comprehensive testing. Refactored everything twice because why not.

**Today**: Open sourcing as an alpha experiment. Search is broken, MCP untested, but the indexing is solid!

**2 weeks. 1 human architect. 1 AI developer. Pure collaboration.**

## Philosophy

- **Simple > Clever** - Boring tech that works
- **Fast > Perfect** - Ship iterations, not perfection
- **Local > Cloud** - Privacy and performance first
- **Open > Closed** - MIT licensed, no vendor lock-in

## License

MIT - Use it, fork it, sell it, build a company. We don't care. Just make AI coding better.

## 🙏 Credits

Built with:
- **[Rust](https://www.rust-lang.org/)** - The perfect systems language
- **[Varios Tree-sitters](https://crates.io/search?q=tree-sitter)** - Parse all the code
- **[Qdrant](https://github.com/qdrant/qdrant)** - Vector database that actually works
- **[Jina AI](https://huggingface.co/jinaai/jina-embeddings-v2-base-code)** - Using v2-base-code model
  - 🔥 **NEW:** [jina-code-embeddings](https://huggingface.co/collections/jinaai/jina-code-embeddings-68b0fbfbb0d639e515f82acd) released Sept 3, 2025! 0.5b/1.5b models trained on code generation - we should upgrade!
- **[Agenterra](https://github.com/clafollett/agenterra)** - Our MCP scaffolding tool that generated the server
- **[MAOS](https://github.com/clafollett/maos)** - Multi-agent orchestration system used in development
- **[Claude Code](https://docs.claude.com/en/docs/claude-code/overview)** - My AI pair programmer who never sleeps

Special thanks to the MCP team at Anthropic for creating the protocol that makes this possible.

---

*From dog walks to production in 14 days. This is what happens when humans and AI build together.* 🚀