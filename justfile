# Codetriever Development Task Runner
# Install just: https://github.com/casey/just#installation
# Usage: just <recipe>

set dotenv-load := true
set export := true

# Default recipe (runs when you just type 'just')
default:
    @just --list

# Check if environment is properly configured
check-env:
    #!/usr/bin/env bash
    echo "🔧 Checking environment configuration..."
    test -f stack.env || (echo "❌ stack.env file not found. Ensure it exists and is properly sourced." && exit 1)
    source stack.env
    test -n "${RUST_TOOLCHAIN:-}" || (echo "❌ RUST_TOOLCHAIN not set. Run: source stack.env" && exit 1)
    test -n "${BUILD_FLAGS:-}" || (echo "❌ BUILD_FLAGS not set. Run: source stack.env" && exit 1)
    test -n "${MIN_MACOS_VERSION:-}" || (echo "❌ Platform variables not set. Run: source stack.env" && exit 1)
    echo "✅ Environment properly configured"

# Show current test configuration  
test-config:
    @echo "📋 Current Test Configuration:"
    @echo "   Profile: ${TEST_PROFILE:-fast}"
    @echo "   Test timeout: ${TEST_TIMEOUT_MS:-5000}ms"
    @echo "   Test iterations: ${TEST_ITERATIONS:-10}"
    @echo ""
    @echo "   Embedding backend: ${EMBEDDING_BACKEND:-native}"
    @echo "   Embedding model: ${EMBEDDING_MODEL:-jina-embeddings-v2-base-code}"
    @echo "   Qdrant URL: ${QDRANT_URL:-http://localhost:6334}"
    @echo "   Use Metal: ${USE_METAL:-true}"
    @echo ""
    @echo "💡 To use different profiles:"
    @echo "   source stack.env                          # fast mode (default)"
    @echo "   TEST_PROFILE=thorough source stack.env && just test"
    @echo "   TEST_PROFILE=ci source stack.env && just test"

# Development setup and validation
dev-setup:
    @echo "🚀 Setting up Codetriever development environment..."
    @just check-env
    @just validate-stack
    @just install-deps
    @just setup-git-hooks
    @just format
    @just lint
    @just test
    @echo "✅ Development environment ready!"

# Validate stack versions match stack.env
validate-stack:
    #!/usr/bin/env bash
    source stack.env
    echo "🔍 Validating development stack..."
    
    # Required files check
    echo "Required files check:"
    test -f rust-toolchain.toml || (echo "❌ rust-toolchain.toml missing" && exit 1)
    test -f clippy.toml || (echo "❌ clippy.toml missing" && exit 1)
    test -f rustfmt.toml || (echo "❌ rustfmt.toml missing" && exit 1)
    echo "✅ All required files present"
    
    # Platform validation
    echo "Platform validation:"
    case "$(uname -s)" in
        Darwin)
            # macOS version check
            macos_version=$(sw_vers -productVersion | cut -d. -f1,2)
            if [[ $(echo "$macos_version >= ${MIN_MACOS_VERSION}" | bc -l) -eq 1 ]]; then
                echo "✅ macOS $macos_version (>= ${MIN_MACOS_VERSION} required)"
            else
                echo "❌ macOS $macos_version is below minimum ${MIN_MACOS_VERSION}"
                exit 1
            fi
            ;;
        Linux)
            # Basic Linux validation
            echo "✅ Linux platform detected"
            if command -v lsb_release >/dev/null 2>&1; then
                distro=$(lsb_release -si)
                version=$(lsb_release -sr)
                echo "📋 Detected: $distro $version"
            fi
            ;;
        MINGW*|CYGWIN*|MSYS*)
            echo "✅ Windows with Unix-like environment detected"
            ;;
        *)
            echo "⚠️  Unknown platform: $(uname -s)"
            ;;
    esac
    
    # Toolchain versions
    echo "Toolchain versions:"
    rustc --version
    cargo --version
    just --version
    
    # Environment variables
    echo "Environment variables:"
    echo "RUST_TOOLCHAIN: ${RUST_TOOLCHAIN}"
    echo "BUILD_FLAGS: ${BUILD_FLAGS}"
    echo "JUST_VERSION: ${JUST_VERSION}"
    echo "CLIPPY_VERSION: ${CLIPPY_VERSION}"
    echo "📋 Stack validation complete"

# Install development dependencies
install-deps:
    @echo "📦 Installing Rust components..."
    rustup component add rustfmt clippy rust-src
    @echo "📦 Installing cargo tools..."
    cargo install cargo-audit --quiet || echo "⚠️  cargo-audit already installed"
    @echo "✅ Dependencies installed"

# Code formatting
format:
    @echo "🎨 Formatting code..."
    cargo fmt --all

# Check formatting without applying changes
format-check:
    @echo "🔍 Checking code formatting..."
    cargo fmt --all -- --check

# Linting with clippy
lint:
    @echo "🔍 Running clippy lints..."
    cargo clippy --all-targets --all-features -- -D warnings -W clippy::uninlined_format_args
    @echo "🔍 Cross-checking for Windows compatibility..."
    @rustup target add x86_64-pc-windows-msvc > /dev/null 2>&1 || true
    @cargo clippy --target x86_64-pc-windows-msvc --all-targets --all-features -- -D warnings || true

# Fix auto-fixable clippy issues
clippy-fix:
    @echo "🔧 Fixing clippy issues..."
    cargo clippy --all-targets --all-features --fix --allow-dirty --allow-staged
    @echo "✅ Applied clippy fixes"


# Run all tests (uses MAOS_TEST_PROFILE from stack.env)
test:
    #!/usr/bin/env bash
    source stack.env
    echo "🧪 Running tests (profile: ${MAOS_TEST_PROFILE})..."
    echo "   Proptest cases: ${MAOS_TEST_SECURITY_PROPTEST_CASES}"
    cargo test --workspace

# Run thorough tests (includes ignored tests)
test-thorough:
    #!/usr/bin/env bash
    export MAOS_TEST_PROFILE=thorough
    source stack.env
    echo "🧪 Running thorough tests..."
    echo "   Proptest cases: ${MAOS_TEST_SECURITY_PROPTEST_CASES}"
    cargo test --workspace -- --include-ignored

# Run only security fuzzing tests with CI-level thoroughness
test-security:
    #!/usr/bin/env bash
    export MAOS_TEST_PROFILE=ci
    source stack.env
    echo "🔒 Running security fuzzing tests (CI mode)..."
    echo "   Proptest cases: ${MAOS_TEST_SECURITY_PROPTEST_CASES}"
    cargo test --workspace --test security_unit

# Run unit tests only (fastest)
test-unit:
    @echo "⚡ Running unit tests only..."
    cargo test --workspace --lib

# Run integration tests only
test-integration:
    @echo "🔧 Running integration tests..."
    cargo test --workspace --tests

# Run tests with coverage (requires cargo-tarpaulin)
test-coverage:
    @echo "📊 Running tests with coverage..."
    cargo tarpaulin --all-features --out Html

# Security audit
audit:
    @echo "🔒 Running security audit..."
    cargo audit

# Build debug version
build:
    @echo "🔨 Building debug version..."
    cargo build --all-targets

# Build release version
build-release:
    @echo "🚀 Building release version..."
    cargo build --release --all-targets

# Check compilation without building
check:
    @echo "✅ Checking compilation..."
    cargo check --all-targets

# Pre-commit checks (all quality gates)
pre-commit: check-env format-check lint test audit
    @echo "✅ All pre-commit checks passed!"

# Clean build artifacts
clean:
    @echo "🧹 Cleaning build artifacts..."
    cargo clean

# Update dependencies
update:
    @echo "📦 Updating dependencies..."
    cargo update

# Run the MAOS CLI
run *args:
    @echo "🤖 Running MAOS..."
    cargo run -- {{args}}

# Development watch mode (requires cargo-watch)
watch:
    @echo "👀 Watching for changes..."
    cargo watch -x check -x test

# Generate documentation
docs:
    @echo "📚 Generating documentation..."
    cargo doc --all-features --open

# Full CI pipeline locally
ci: format-check lint test audit build
    @echo "🎉 Full CI pipeline completed successfully!"

# Set up git hooks (pure Rust alternative to pre-commit)
setup-git-hooks:
    #!/usr/bin/env bash
    echo "🪝 Setting up git hooks..."
    mkdir -p .git/hooks
    cat > .git/hooks/pre-commit << 'HOOK_EOF'
    #!/bin/sh
    # MAOS Pre-commit Hook - Validates environment and runs quality checks
    
    set -e  # Exit on any error
    
    echo "🪝 MAOS Pre-commit validation starting..."
    
    # Validate development environment
    echo "📋 Sourcing stack.env..."
    # Git hooks run from the repository root, but let's be explicit
    REPO_ROOT="$(git rev-parse --show-toplevel)"
    STACK_ENV_PATH="$REPO_ROOT/stack.env"
    if [ ! -f "$STACK_ENV_PATH" ]; then
        echo "❌ stack.env file not found at $STACK_ENV_PATH"
        echo "💡 Ensure the file exists and is properly located in the project root directory"
        exit 1
    fi
    source "$STACK_ENV_PATH" || {
        echo "❌ Failed to source stack.env"
        echo "💡 Check the file for errors or permissions issues"
        exit 1
    }
    
    # Validate stack configuration
    echo "🔍 Validating development stack..."
    just validate-stack || {
        echo "❌ Stack validation failed"
        echo "💡 Run 'just dev-setup' to fix your environment"
        exit 1
    }
    
    # Run all quality checks
    echo "✅ Running pre-commit quality checks..."
    just pre-commit || {
        echo "❌ Pre-commit checks failed"
        echo "💡 Fix the issues above and try committing again"
        exit 1
    }
    
    echo "🎉 All pre-commit checks passed!"
    HOOK_EOF
    chmod +x .git/hooks/pre-commit
    echo "✅ Git hooks installed! All commits will validate environment and run quality checks"

# ========================
# Git & Worktree Commands
# ========================

# List all active worktrees
worktree-list:
    @echo "📋 Active worktrees:"
    @git worktree list

# Clean up stale worktrees
worktree-cleanup:
    @echo "🧹 Pruning stale worktrees..."
    @git worktree prune
    @echo "✅ Cleanup complete"

# Show git status across all worktrees
status-all:
    @echo "📊 Status of all worktrees:"
    @for worktree in $(git worktree list --porcelain | grep "worktree" | cut -d' ' -f2); do \
        echo "\n📁 $$worktree:"; \
        git -C "$$worktree" status -s || echo "  (no changes)"; \
    done

# ========================
# MAOS Coordination
# ========================

# Show current MAOS session info
session-info:
    @echo "🤖 MAOS Session Info:"
    @if [ -f .maos/session.json ]; then \
        cat .maos/session.json | python -m json.tool; \
    else \
        echo "No active session"; \
    fi

# Show active agents
agents:
    @echo "👥 Active Agents:"
    @if [ -f .maos/coordination/agents.json ]; then \
        cat .maos/coordination/agents.json | python -m json.tool; \
    else \
        echo "No active agents"; \
    fi

# Show file locks
locks:
    @echo "🔒 File Locks:"
    @if [ -f .maos/coordination/locks.json ]; then \
        cat .maos/coordination/locks.json | python -m json.tool; \
    else \
        echo "No active locks"; \
    fi

# Clean MAOS session data
clean-session:
    @echo "🧹 Cleaning MAOS session data..."
    @rm -rf .maos/session.json .maos/coordination/
    @echo "✅ Session cleaned"

# ========================
# Development Shortcuts
# ========================

# Quick test a specific module
test-module module:
    @echo "🧪 Testing module: {{module}}"
    @cargo test --package {{module}}

# Run with verbose output
run-verbose *args:
    @RUST_LOG=debug cargo run -- {{args}}

# Format and lint in one command
fmt: format lint

# Fix all auto-fixable issues (format + clippy)
fix: format clippy-fix
    @echo "🎯 All auto-fixes applied!"

# Quick check without tests
quick: format-check lint check

# ========================
# Codetriever Commands
# ========================

# Run native development environment (Mac with Metal)
dev:
    #!/usr/bin/env bash
    set -e
    echo "🚀 Starting Codetriever native development..."
    
    # Start Qdrant in Docker
    just qdrant-start
    
    # Wait for Qdrant to be ready
    sleep 2
    
    # Run MCP server
    echo "Starting MCP server..."
    cargo run --bin codetriever -- serve --mcp

# Run Docker environment
dev-docker:
    docker-compose up --build

# Stop all Codetriever services
stop:
    @just qdrant-stop
    @pkill codetriever || true
    @docker-compose down 2>/dev/null || true
    @echo "✅ All services stopped"

# Create new API crate
create-api:
    cargo new --lib crates/codetriever-api
    @echo "✅ Created codetriever-api crate"

# Run tests with TDD output
tdd:
    cargo test --all -- --nocapture

# Watch and test (Red/Green/Refactor cycle)
tdd-watch:
    cargo watch -x "test --all -- --nocapture"

# Full quality check (format, lint, test)
quality: fmt lint test
    @echo "✅ Quality checks passed!"

# Run Codetriever-specific tests
test-codetriever:
    cargo test --workspace --all-features

# Build Codetriever crates
build-codetriever:
    cargo build --workspace --all-targets

# Clean and rebuild
rebuild: clean build
    @echo "✅ Clean rebuild complete"

# Install git hooks for quality checks
install-hooks:
    #!/usr/bin/env bash
    echo "🪝 Installing Codetriever git hooks..."
    mkdir -p .git/hooks
    cat > .git/hooks/pre-commit << 'EOF'
    #!/bin/sh
    echo "🪝 Running pre-commit checks..."
    just quality || {
        echo "❌ Pre-commit checks failed"
        echo "💡 Fix issues and try again"
        exit 1
    }
    echo "✅ Pre-commit checks passed!"
    EOF
    chmod +x .git/hooks/pre-commit
    echo "✅ Git hooks installed!"

# Remove git hooks
uninstall-hooks:
    rm -f .git/hooks/pre-commit
    @echo "✅ Git hooks removed"

# Benchmark embeddings performance
bench-embeddings:
    cargo bench -p codetriever-api --bench embeddings

# Show project stats
stats:
    @echo "📊 Codetriever Statistics:"
    @echo "Lines of Rust code:"
    @find crates -name "*.rs" -type f | xargs wc -l | tail -1
    @echo "\nNumber of crates:"
    @ls -1 crates/ 2>/dev/null | wc -l || echo "0"
    @echo "\nNumber of tests:"
    @grep -r "#\[test\]" --include="*.rs" crates | wc -l || echo "0"

# === Qdrant Docker Commands ===

# Start Qdrant in Docker
qdrant-start:
    @echo "🚀 Starting Qdrant in Docker..."
    @docker run -d \
        --name qdrant \
        -p 6333:6333 \
        -p 6334:6334 \
        -v $(PWD)/qdrant_storage:/qdrant/storage:z \
        qdrant/qdrant 2>/dev/null || docker start qdrant
    @sleep 2
    @curl -s http://localhost:6333/health >/dev/null && echo "✅ Qdrant ready on http://localhost:6333" || echo "⚠️  Qdrant starting..."

# Stop Qdrant
qdrant-stop:
    @docker stop qdrant 2>/dev/null || true
    @echo "✅ Qdrant stopped"

# Remove Qdrant container
qdrant-clean:
    @docker rm -f qdrant 2>/dev/null || true
    @echo "✅ Qdrant container removed"

# Show Qdrant logs
qdrant-logs:
    @docker logs -f qdrant

# Check Qdrant health
qdrant-health:
    @curl -s http://localhost:6333 | jq '.' || echo "❌ Qdrant not responding"