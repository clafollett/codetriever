# Contributing to Codetriever

Welcome to Codetriever! This guide will help you set up your development environment and contribute effectively to our semantic code search engine.

## 🚀 Quick Start

### 1. Development Environment Setup

```bash
# Clone the repository
git clone https://github.com/clafollett/codetriever.git
cd codetriever

# Source the development stack environment
source stack.env

# Set up development environment (installs dependencies and starts services)
just dev-setup
```

### 2. Install Just Command Runner

Codetriever uses [`just`](https://github.com/casey/just) for development task automation:

```bash
# macOS
brew install just

# Linux
curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to ~/bin

# Windows (PowerShell)
winget install --id Casey.Just --exact

# Or via Cargo (all platforms)
cargo install just
```

### 3. Essential Commands

```bash
# See all available commands
just

# Initialize Docker services and database
just init

# Development workflow
just fmt           # Format code
just lint          # Run clippy
just test          # Run all tests
just build         # Build project
just fix           # Fix all auto-fixable issues

# Running the API
just api           # Start API server
```

## 📋 Development Stack

Our development environment is defined in `stack.env` to ensure consistency across all contributors.

### Required Tools

- **Rust**: Stable toolchain (1.70+)
- **Just**: Task runner for development workflows
- **Docker**: For PostgreSQL and Qdrant services
- **Git 2.5+**: For version control

### Rust Toolchain Setup

If you don't have Rust installed:

```bash
# Install Rust (stable toolchain)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Verify installation
rustc --version
cargo --version
```

### Stack Validation

```bash
# Load environment and run tests
source stack.env
just test
```

## 🛠️ Development Workflow

### 1. Before You Start

1. Check the [GitHub Issues](https://github.com/clafollett/codetriever/issues) for available tasks
2. Pick an issue that interests you
3. Comment on the issue to let others know you're working on it

### 2. Branch Naming Convention

```
<type>/<brief-description>

Examples:
- feature/add-cli-search-command
- fix/qdrant-connection-bug
- docs/update-api-examples
```

### 3. Development Process

```bash
# 1. Create and switch to feature branch
git checkout -b feature/your-feature-name

# 2. Make your changes following TDD principles
just test          # Write failing tests first
# ... implement code ...
just test          # Make tests pass
just fix           # Format code and fix linting issues

# 3. Run full checks
just check         # Runs format, lint, and test

# 4. Commit your changes
git add .
git commit -m "feat: add your feature description"

# 5. Push and create PR
git push -u origin feature/your-feature-name
# Then create a pull request on GitHub
```

### 4. Code Quality Standards

All code must pass these quality gates:

- ✅ **Formatting**: `just fmt` (rustfmt with project config)
- ✅ **Linting**: `just lint` (clippy with zero warnings policy)
- ✅ **Tests**: `just test` (all tests pass)
- ✅ **Compilation**: `just build` (clean build)

## 🏗️ Architecture Overview

Codetriever follows a modular multi-crate workspace architecture:

```
crates/
├── codetriever/           # Main CLI application
├── codetriever-api/       # REST API server
├── codetriever-config/    # Configuration management
├── codetriever-indexing/  # Code indexing pipeline
├── codetriever-parsing/   # Tree-sitter parsing and chunking
├── codetriever-search/    # Semantic search functionality
├── codetriever-embeddings/# Embedding generation
├── codetriever-vector-data/# Vector storage (Qdrant)
├── codetriever-meta-data/ # PostgreSQL metadata storage
└── codetriever-common/    # Shared utilities
```

### Benefits of Multi-Crate Architecture

- **Parallel Compilation**: Crates build independently
- **Clear Separation**: Each crate has focused responsibility
- **Modular Testing**: Test individual components in isolation
- **Type Safety**: Strong compile-time guarantees across boundaries

### Coding Standards

1. **Test-First Development**: Write failing tests before implementation (TDD)
2. **Modern Rust**: Use latest stable features and idioms
3. **Documentation**: Document public APIs with examples
4. **Error Handling**: Use proper Result types and structured errors

## 🧪 Testing Strategy

### Test Organization

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semantic_search() {
        // Test semantic search functionality
        let query = "function that calculates fibonacci";
        let results = search_service.search(query, 10).await.unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_code_parsing() {
        // Test tree-sitter parsing
        let code = "fn main() { println!(\"Hello\"); }";
        let chunks = parser.parse(code, "rust").unwrap();
        assert_eq!(chunks.len(), 1);
    }
}
```

### Testing Types

1. **Unit Tests**: Each crate has comprehensive unit tests
2. **Integration Tests**: Full stack testing with real components
3. **API Tests**: REST API endpoint testing

### Running Tests

```bash
just test              # All tests across workspace
just test-unit         # Unit tests only (fast)
just test-integration  # Integration tests
cargo test -p codetriever-parsing  # Single crate tests
```

## 📝 Commit Standards

Follow Conventional Commits:

```
<type>: <description>

Types:
- feat: New features
- fix: Bug fixes
- docs: Documentation
- refactor: Code refactoring
- test: Adding/updating tests
- chore: Maintenance

Examples:
- feat: add semantic search API endpoint
- fix: resolve Qdrant connection timeout
- docs: update API documentation
```

## 🔄 Pull Request Process

### 1. PR Requirements

- [ ] All quality gates pass (`just check`)
- [ ] Tests added/updated for new functionality
- [ ] Documentation updated if needed
- [ ] Code follows project conventions

### 2. Review Process

1. **Automated Checks**: CI runs all quality gates
2. **Code Review**: Maintainer review
3. **Merge**: Squash and merge to main after approval

## 🆘 Getting Help

- **Questions**: Create a [GitHub Discussion](https://github.com/clafollett/codetriever/discussions)
- **Bugs**: Create an [issue](https://github.com/clafollett/codetriever/issues) with reproduction steps
- **Features**: Create an [issue](https://github.com/clafollett/codetriever/issues) with detailed requirements

## 💡 What to Contribute

### Quick Wins for New Contributors

- [ ] Add CLI commands for search/similar/context
- [ ] **Upgrade to NEW Jina code models** (released Sept 3, 2025!)
- [ ] Improve error messages and user experience
- [ ] Add more language parsing tests
- [ ] Write documentation and examples
- [ ] Test MCP server integration

### Larger Projects

- [ ] Web UI for the search interface
- [ ] Multiple embedding model support
- [ ] Git integration for code history
- [ ] Performance optimizations
- [ ] Language-specific parsing improvements

## 🔧 IDE Setup

### VS Code (Recommended)

Install recommended extensions:

```bash
code --install-extension rust-lang.rust-analyzer
code --install-extension skellock.just
code --install-extension tamasfe.even-better-toml
```

### Configuration

All editors should be configured to:
1. Use rust-analyzer as the language server
2. Run rustfmt on save
3. Show clippy lints inline
4. Exclude target/ directory from file watching

## 🚀 API Development

### Running the API

```bash
# Start all services
just init

# Start API server
just api

# Test endpoints
curl -X POST http://localhost:8080/index \
  -H "Content-Type: application/json" \
  -d '{"project_id": "test", "files": [{"path": "test.rs", "content": "fn main() {}"}]}'
```

### API Testing

```bash
# Run API-specific tests
cargo test -p codetriever-api

# Test with real services
just test-integration
```

## 📚 Additional Resources

- [Architecture Documentation](docs/architecture/)
- [API Documentation](docs/api/)
- [Deployment Guide](docs/deployment/)

---

Thank you for contributing to Codetriever! 🚀 Together we're building better tools for AI-powered code search.