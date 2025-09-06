# Codetriever Development Commands
# https://github.com/clafollett/codetriever

set dotenv-load := true
set export := true

# Default: Show available commands
default:
    @just --list

# ========================
# Development Setup
# ========================

# Initial setup - install all dependencies and configure environment
dev-setup:
    @echo "🚀 Setting up Codetriever development environment..."
    @just install-deps
    @just setup-git-hooks
    @echo "✅ Development environment ready!"

# Install required dependencies
install-deps:
    @echo "📦 Installing Rust components..."
    rustup component add rustfmt clippy rust-src
    @echo "✅ Dependencies installed"

# Setup git hooks for quality checks
setup-git-hooks:
    @echo "🪝 Setting up git hooks..."
    @mkdir -p .git/hooks
    @echo '#!/bin/sh' > .git/hooks/pre-commit
    @echo 'echo "🪝 Running pre-commit checks..."' >> .git/hooks/pre-commit
    @echo 'echo "🎨 Formatting code..."' >> .git/hooks/pre-commit
    @echo 'cargo fmt --all' >> .git/hooks/pre-commit
    @echo 'echo "🔍 Running clippy lints..."' >> .git/hooks/pre-commit
    @echo 'cargo clippy --all-targets --all-features -- -D warnings -W clippy::uninlined_format_args' >> .git/hooks/pre-commit
    @echo 'echo "⚡ Running unit tests and doc tests..."' >> .git/hooks/pre-commit
    @echo 'cargo test --workspace --lib --bins' >> .git/hooks/pre-commit
    @echo 'cargo test --workspace --doc' >> .git/hooks/pre-commit
    @echo 'echo "✅ Pre-commit checks passed!"' >> .git/hooks/pre-commit
    @chmod +x .git/hooks/pre-commit
    @echo "✅ Git hooks installed!"

# ========================
# Docker Infrastructure
# ========================

# Start all services (PostgreSQL + Qdrant)
docker-up:
    @echo "🚀 Starting Docker services..."
    @docker-compose -f docker/docker-compose.data.yml up -d
    @sleep 3
    @echo "✅ PostgreSQL ready on port 5433"
    @echo "✅ Qdrant ready on http://localhost:6334"

# Stop all services
docker-down:
    @echo "🛑 Stopping Docker services..."
    @docker-compose -f docker/docker-compose.data.yml stop
    @echo "✅ Services stopped"

# Remove containers and volumes (full reset)
docker-reset:
    @echo "🗑️ Resetting Docker environment..."
    @docker-compose -f docker/docker-compose.data.yml down -v
    @echo "✅ Docker environment reset"

# View service logs
docker-logs:
    @docker-compose -f docker/docker-compose.data.yml logs -f

# ========================
# Database Management
# ========================

# Initialize database schema
db-setup: docker-up
    @echo "🔧 Setting up database..."
    @DATABASE_URL="${DATABASE_URL:-postgresql://codetriever:codetriever@localhost:5433/codetriever?sslmode=disable}" \
        cargo run -p codetriever-data --example run_migrations
    @echo "✅ Database ready"

# Run migrations
db-migrate: db-setup

# Reset database (drop and recreate)
db-reset: docker-reset docker-up
    @sleep 3
    @just db-setup

# ========================
# Development Workflow
# ========================

# Run all tests
test:
    @echo "🧪 Running all tests..."
    cargo test --workspace

# Run unit tests only (fast)
test-unit:
    @echo "⚡ Running unit tests..."
    cargo test --workspace --lib --bins
    cargo test --workspace --doc

# Run integration tests only
test-integration:
    @echo "🔧 Running integration tests..."
    cargo test --workspace --tests

# Format code

# Fix clippy warnings
clippy-fix:
    @echo "🔧 Fixing clippy issues..."
    cargo clippy --all-targets --all-features --fix --allow-dirty --allow-staged -- -D warnings -W clippy::uninlined_format_args
    @echo "✅ Applied clippy fixes"

fmt:
    @echo "🎨 Formatting code..."
    cargo fmt --all

# Run clippy lints
lint:
    @echo "🔍 Running clippy..."
    cargo clippy --all-targets --all-features -- -D warnings -W clippy::uninlined_format_args

# Fix all auto-fixable issues
fix: fmt clippy-fix
    @echo "✅ All auto-fixes applied!"

# Run all quality checks
check: fmt lint test-unit
    @echo "✅ All checks passed!"

# Watch for changes and run tests
watch:
    @echo "👀 Watching for changes..."
    cargo watch -x check -x test

# ========================
# Building & Running
# ========================

# Build debug version
build:
    @echo "🔨 Building debug..."
    cargo build --workspace --all-targets

# Build release version
build-release:
    @echo "🚀 Building release..."
    cargo build --workspace --release --all-targets

# Run CLI
run *args:
    cargo run --bin codetriever -- {{args}}

# Run API server
api:
    cargo run --bin codetriever-api

# Clean build artifacts
clean:
    @echo "🧹 Cleaning..."
    cargo clean

# ========================
# Documentation
# ========================

# Generate and open documentation
docs:
    @echo "📚 Generating documentation..."
    cargo doc --all-features --open

# Show project statistics
stats:
    @echo "📊 Project Statistics:"
    @echo "Lines of Rust code:"
    @find crates -name "*.rs" -type f | xargs wc -l | tail -1
    @echo "\nNumber of crates:"
    @ls -1 crates/ 2>/dev/null | wc -l || echo "0"
    @echo "\nNumber of tests:"
    @grep -r "#\[test\]" --include="*.rs" crates | wc -l || echo "0"

# ========================
# Common Workflows
# ========================

# Initialize everything (Docker + Database)
init: docker-up db-setup
    @echo "🎉 Codetriever environment initialized!"

# Quick setup and test (for new contributors)
quick-start: init test
    @echo "✅ Codetriever is ready to use!"

# Full CI pipeline locally
ci: fmt lint test build
    @echo "✅ CI pipeline passed!"

# Development mode with auto-reload
dev: docker-up
    @echo "🚀 Starting development mode..."
    cargo watch -x "run --bin codetriever-api"

# ========================
# Utility Commands
# ========================

# Update dependencies
update:
    @echo "📦 Updating dependencies..."
    cargo update

# Security audit
audit:
    @echo "🔒 Running security audit..."
    cargo audit || echo "⚠️ Run 'cargo install cargo-audit' if not installed"

# Clean Qdrant test collections
clean-test-data:
    @echo "🧹 Cleaning test collections..."
    @curl -s http://localhost:6334/collections | \
        jq -r '.result.collections[].name' | \
        grep '^test_' | \
        xargs -I {} curl -X DELETE "http://localhost:6334/collections/{}" 2>/dev/null || true
    @echo "✅ Test data cleaned"