# 🔍 Codetriever

**Local-first semantic code search for massive codebases. Built in Rust.**

## What It Does

Codetriever indexes your codebase using semantic embeddings, enabling intelligent code discovery through vector similarity search.

```bash
# Index your codebase
codetriever index /path/to/repo

# Search semantically
codetriever search "database connection pooling logic"

# Run as API server
codetriever-api
```

## Why It Exists

- **Context windows are limited** - Even 200k tokens can't fit enterprise codebases
- **Semantic understanding** - Find code by meaning, not just keywords
- **Privacy matters** - Your code stays on your machine
- **Performance** - Sub-second semantic search across large codebases

## Current Architecture

```
Your Code → Tree-sitter Parser → Semantic Chunks → Vector Embeddings → Qdrant
     ↑                                                                    ↓
File Tracking                                                             ↓
(PostgreSQL)                                                              ↓
     ↓                                                                    ↓
API Server ← HTTP/JSON ← Search Results ← Similarity Search ←─────────────┘
```

## Features

### Implemented ✅

- 🦀 **Pure Rust** - Fast, safe, modular crate architecture
- 🌳 **Tree-sitter parsing** - AST-based code understanding for 25+ languages
- 🧠 **Smart chunking** - Token-aware splitting with overlap preservation
- 🔢 **Multiple token counters** - Tiktoken (OpenAI), Heuristic fallback
- 💾 **Dual storage** - PostgreSQL for metadata, Qdrant for vectors
- 🎯 **Incremental indexing** - Only re-indexes changed files
- 📊 **Embedding generation** - Local BERT models (Jina v2)
- 🔌 **REST API** - Full-featured HTTP API for search and indexing
- 🧪 **Comprehensive testing** - Unit, integration, and doc tests

### Architecture Components

- **codetriever** - CLI tool for indexing and searching
- **codetriever-api** - REST API server (Axum-based)
- **codetriever-indexer** - Core indexing logic with trait abstractions
- **codetriever-data** - PostgreSQL state management and models

## Installation

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

## Quick Start

```bash
# Initialize Docker services and database
just init

# Index a repository
codetriever index /path/to/your/codebase --recursive

# Search for code
codetriever search "error handling in async functions"

# Start API server
codetriever-api

# Use API
curl -X POST http://localhost:3000/search \
  -H "Content-Type: application/json" \
  -d '{"query": "database migrations", "limit": 10}'
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

## Contributing

This project follows trunk-based development with continuous integration.

1. **Fork and clone** the repository
2. **Create a feature branch** for significant changes
3. **Write tests** - We use TDD (Red/Green/Refactor)
4. **Run quality checks** - `just test && just clippy-fix`
5. **Submit a PR** with clear description

See [CONTRIBUTING.md](CONTRIBUTING.md) for details.

## Philosophy

- **Simple > Clever** - Boring tech that works
- **Fast > Perfect** - Ship iterations, not perfection  
- **Local > Cloud** - Privacy and performance first
- **Open > Closed** - MIT licensed, no vendor lock-in

## License

MIT - Use it, fork it, sell it. We don't care.

---

*Built with 🦀 by developers who understand that code search should be fast, private, and intelligent.*