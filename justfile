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
    @just setup-env
    @just setup-git-hooks
    @echo "✅ Development environment ready!"

# Setup environment file if not exists
setup-env:
    @if [ ! -f .env ]; then \
        echo "📝 Creating .env file from template..."; \
        cp .env.sample .env; \
        echo "⚠️  Please edit .env with your database credentials"; \
        echo "   Default development credentials are provided for local use only"; \
        echo "   NEVER use default credentials in production!"; \
    else \
        echo "✅ .env file already exists"; \
    fi

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

# Environment selection (defaults to 'data' for local development)
# Valid values: data, dev, prod
ENV := env_var_or_default("CODETRIEVER_ENV", "data")

# Start all services (PostgreSQL + Qdrant)
docker-up:
    @echo "🚀 Starting Docker services ({{ENV}} environment)..."
    @docker-compose -f docker/docker-compose.{{ENV}}.yml up -d
    @sleep 3
    @echo "✅ Services started for {{ENV}} environment"
    @if [ "{{ENV}}" = "data" ]; then \
        echo "✅ PostgreSQL ready on port 5433"; \
        echo "✅ Qdrant ready on http://localhost:6334"; \
    fi

# Stop all services
docker-down:
    @echo "🛑 Stopping Docker services ({{ENV}} environment)..."
    @docker-compose -f docker/docker-compose.{{ENV}}.yml stop
    @echo "✅ Services stopped"

# Remove containers and volumes (full reset)
docker-reset:
    @echo "🗑️ Resetting Docker environment ({{ENV}} environment)..."
    @docker-compose -f docker/docker-compose.{{ENV}}.yml down -v
    @echo "✅ Docker environment reset"

# View service logs
docker-logs:
    @docker-compose -f docker/docker-compose.{{ENV}}.yml logs -f

# ========================
# Database Management
# ========================

# Initialize database schema
db-setup: docker-up
    @echo "🔧 Setting up database..."
    @if [ ! -f .env ]; then \
        echo "❌ No .env file found. Please create one:"; \
        echo "   cp .env.sample .env"; \
        echo "   Then edit .env with your database credentials"; \
        exit 1; \
    fi
    @echo "📋 Loading database configuration from .env..."
    @echo "   DB_HOST=${DB_HOST}"
    @echo "   DB_PORT=${DB_PORT}"
    @echo "   DB_NAME=${DB_NAME}"
    @echo "   DB_USER=${DB_USER}"
    @echo "🔧 Running database migrations..."
    cargo run -p codetriever-meta-data --example run_migrations
    @echo "✅ Database setup complete"

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

# Run tests with GPU acceleration
test-metal:
    @echo "🍎 Running tests with Metal GPU support..."
    cargo test-metal --workspace

test-cuda:
    @echo "🐧 Running tests with CUDA GPU support..."
    cargo test-cuda --workspace

# Format code

# Fix clippy warnings
clippy-fix:
    @echo "🔧 Fixing clippy issues..."
    cargo clippy --all-targets --fix --allow-dirty --allow-staged -- -D warnings -W clippy::uninlined_format_args
    @echo "✅ Applied clippy fixes"

fmt:
    @echo "🎨 Formatting code..."
    cargo fmt --all

# Run clippy lints (matches CI environment)
lint:
    @echo "🔍 Running clippy..."
    RUSTFLAGS="-D warnings" cargo clippy --all-targets -- -W clippy::uninlined_format_args

# Fix all auto-fixable issues
fix: fmt clippy-fix
    @echo "✅ All auto-fixes applied!"

# Run CI checks locally before pushing
ci-check:
    @echo "🔍 Running CI checks locally..."
    @echo "1️⃣ Formatting check..."
    @cargo fmt --all -- --check || (echo "❌ Format check failed. Run 'just fmt' to fix." && exit 1)
    @echo "2️⃣ Clippy check..."
    @RUSTFLAGS="-D warnings" cargo clippy --all-targets -- -W clippy::uninlined_format_args || (echo "❌ Clippy check failed. Run 'just fix' to auto-fix." && exit 1)
    @echo "3️⃣ Build check..."
    @cargo build --workspace || (echo "❌ Build failed." && exit 1)
    @echo "✅ All CI checks passed!"

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

# Build debug version (CPU-only by default)
build:
    @echo "🔨 Building..."
    cargo build --workspace --all-targets

# Build with GPU acceleration
build-metal:
    @echo "🍎 Building with Metal GPU support..."
    cargo build-metal --workspace --all-targets

build-cuda:
    @echo "🚀 Building with CUDA GPU support..."
    cargo build-cuda --workspace --all-targets

# Build release version
build-release:
    @echo "🚀 Building release..."
    cargo build --workspace --release --all-targets

# Run CLI
run *args:
    cargo run --bin codetriever -- {{args}}

# Run API server (CPU-only by default)
api:
    @echo "🚀 Starting API server..."
    cargo run --bin codetriever-api

# Run API with GPU acceleration
api-metal:
    @echo "🍎 Starting API with Metal GPU acceleration..."
    cargo api-metal

api-cuda:
    @echo "🐧 Starting API with CUDA GPU acceleration..."
    cargo api-cuda

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
# Deployment Commands
# ========================

# Check if any codetriever containers are running
check-running:
    @if docker ps --format '{{ "{{.Names}}" }}' | grep -q '^codetriever-'; then \
        echo "⚠️  WARNING: Codetriever containers are already running:"; \
        docker ps --filter "name=codetriever-" --format 'table {{ "{{.Names}}" }}\t{{ "{{.Status}}" }}'; \
        echo ""; \
        echo "❌ Please stop existing environment first with:"; \
        echo "   just docker-down  (for data environment)"; \
        echo "   just stop-dev     (for dev environment)"; \
        echo "   just stop-prod    (for prod environment)"; \
        exit 1; \
    fi

# Deploy development environment
deploy-dev: check-running
    @echo "🚀 Deploying development environment..."
    @docker-compose -f docker/docker-compose.dev.yml up -d
    @sleep 3
    @echo "✅ Development environment deployed"

# Build Docker image for API
build-docker:
    @echo "🔨 Building Docker image for API..."
    @docker build -f docker/Dockerfile.api -t codetriever/api:latest .
    @echo "✅ Docker image built: codetriever/api:latest"

# Deploy production environment
deploy-prod: check-running build-docker
    @echo "🚀 Deploying production environment..."
    @echo "⚠️  WARNING: Using production configuration"
    @echo "⚠️  Ensure all environment variables are properly set!"
    @docker-compose -f docker/docker-compose.prod.yml up -d
    @sleep 3
    @echo "✅ Production environment deployed"

# Stop development environment
stop-dev:
    @echo "🛑 Stopping development environment..."
    @docker-compose -f docker/docker-compose.dev.yml stop

# Stop production environment  
stop-prod:
    @echo "⚠️  Stopping production environment..."
    @docker-compose -f docker/docker-compose.prod.yml stop

# Stop all Codetriever containers regardless of environment
stop-all:
    @echo "🛑 Stopping all Codetriever containers..."
    @docker ps -q --filter "name=codetriever-" | xargs -r docker stop 2>/dev/null || true
    @echo "✅ All containers stopped"

# Show status of all environments
status:
    @echo "📊 Container Status:"
    @docker ps --filter "name=codetriever-" --format 'table {{ "{{.Names}}" }}\t{{ "{{.Status}}" }}\t{{ "{{.Ports}}" }}'

# Switch environments (stops current, starts new)
switch env:
    @echo "🔄 Switching to {{env}} environment..."
    @echo "Stopping current containers..."
    @docker stop $(docker ps -q --filter "name=codetriever-") 2>/dev/null || true
    @echo "Starting {{env}} environment..."
    @if [ "{{env}}" = "dev" ]; then \
        just deploy-dev; \
    elif [ "{{env}}" = "prod" ]; then \
        just deploy-prod; \
    elif [ "{{env}}" = "data" ]; then \
        just docker-up; \
    else \
        echo "❌ Unknown environment: {{env}}"; \
        echo "   Valid options: data, dev, prod"; \
        exit 1; \
    fi

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
dev: deploy-dev
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
    # Configuration in .cargo/audit.toml (includes allowlisted advisories)
    cargo audit || echo "⚠️ Run 'cargo install cargo-audit' if not installed"

# Check Qdrant status
qdrant-status:
    @echo "🔍 Checking Qdrant status..."
    @curl -s http://localhost:6333/ | python3 -m json.tool 2>/dev/null || echo "Qdrant not responding on port 6333"

# Check PostgreSQL test data
db-check-test-data:
    @echo "🔍 Checking for test data in PostgreSQL..."
    @echo "Tables with 'test' in repository_id:"
    @echo ""
    @echo "project_branches:"
    @PGPASSWORD=${DB_PASSWORD} psql -h localhost -p 5433 -U ${DB_USER} -d ${DB_NAME} -c "SELECT repository_id, branch FROM project_branches WHERE repository_id LIKE '%test%';" 2>/dev/null || echo "No test data or connection error"
    @echo ""
    @echo "indexed_files:"
    @PGPASSWORD=${DB_PASSWORD} psql -h localhost -p 5433 -U ${DB_USER} -d ${DB_NAME} -c "SELECT repository_id, branch, file_path FROM indexed_files WHERE repository_id LIKE '%test%';" 2>/dev/null || echo "No test data or connection error"
    @echo ""
    @echo "chunk_metadata (count):"
    @PGPASSWORD=${DB_PASSWORD} psql -h localhost -p 5433 -U ${DB_USER} -d ${DB_NAME} -c "SELECT repository_id, COUNT(*) as chunk_count FROM chunk_metadata WHERE repository_id LIKE '%test%' GROUP BY repository_id;" 2>/dev/null || echo "No test data or connection error"

# Clean up PostgreSQL test data
db-clean-test-data:
    @echo "🧹 Cleaning test data from PostgreSQL..."
    @PGPASSWORD=${DB_PASSWORD} psql -h localhost -p 5433 -U ${DB_USER} -d ${DB_NAME} -c "DELETE FROM project_branches WHERE repository_id LIKE '%test%';" 2>/dev/null || echo "No test data to clean or connection error"
    @echo "✅ Test data cleaned from PostgreSQL"

# List Qdrant collections
qdrant-list:
    @echo "📋 Listing Qdrant collections..."
    @curl -H "api-key: ${QDRANT_API_KEY}" http://localhost:6333/collections

# Clean Qdrant test collections
clean-test-data:
    @echo "🧹 Cleaning test collections..."
    @curl -s -H "api-key: ${QDRANT_API_KEY}" http://localhost:6334/collections | \
        python3 -c "import sys, json; data = json.load(sys.stdin); [print(c['name']) for c in data.get('result', {}).get('collections', []) if c['name'].startswith('test_')]" | \
        xargs -I {} curl -X DELETE -H "api-key: ${QDRANT_API_KEY}" "http://localhost:6334/collections/{}" 2>/dev/null || true
    @echo "✅ Test data cleaned"
# ========================
# GitHub Issue Workflow
# ========================

# Start work on a GitHub issue (creates feature branch + updates todo)
start-issue ISSUE_NUMBER DESCRIPTION="":
    #!/usr/bin/env bash
    set -euo pipefail
    BRANCH="feature/issue-{{ISSUE_NUMBER}}-{{DESCRIPTION}}"
    if [ -z "{{DESCRIPTION}}" ]; then
        # Fetch issue title from GitHub and slugify it
        TITLE=$(gh issue view {{ISSUE_NUMBER}} --json title -q .title | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]/-/g' | sed 's/--*/-/g' | sed 's/^-//' | sed 's/-$//' | cut -c1-40)
        BRANCH="feature/issue-{{ISSUE_NUMBER}}-${TITLE}"
    fi
    echo "🚀 Starting issue #{{ISSUE_NUMBER}}: $(gh issue view {{ISSUE_NUMBER}} --json title -q .title)"
    git checkout -b "$BRANCH"
    echo "✅ Branch: $BRANCH"
    echo "💡 Commit with: 'Ref #{{ISSUE_NUMBER}}' | Finish: just finish-issue {{ISSUE_NUMBER}}"

# Finish work on an issue (squash merge to main)
finish-issue ISSUE_NUMBER:
    #!/usr/bin/env bash
    set -euo pipefail
    CURRENT_BRANCH=$(git branch --show-current)
    if [[ ! "$CURRENT_BRANCH" =~ issue-{{ISSUE_NUMBER}} ]]; then
        echo "⚠️  Warning: Current branch doesn't match issue #{{ISSUE_NUMBER}}"
        echo "   Current: $CURRENT_BRANCH"
        read -p "Continue anyway? (y/N) " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            exit 1
        fi
    fi
    echo "🔍 Running final checks..."
    just check
    echo "🎯 Squash merging to main..."
    git checkout main
    git pull --rebase
    git merge --squash "$CURRENT_BRANCH"
    echo ""
    echo "✅ Changes staged for commit. Commit message will reference #{{ISSUE_NUMBER}}"
    echo ""
    echo "💡 Next: Review staged changes, then commit and push"

# Show current issue (from branch name)
current-issue:
    #!/usr/bin/env bash
    BRANCH=$(git branch --show-current)
    if [[ "$BRANCH" =~ issue-([0-9]+) ]]; then
        ISSUE="${BASH_REMATCH[1]}"
        echo "📋 Currently working on issue #$ISSUE"
        echo ""
        gh issue view "$ISSUE"
    else
        echo "ℹ️  Not on an issue branch (current: $BRANCH)"
    fi

# List issues by label (defaults to all open)
issues LABEL="":
    #!/usr/bin/env bash
    if [ -z "{{LABEL}}" ]; then
        echo "📋 All Open Issues:"
        gh issue list --limit 30
    else
        echo "🎯 Issues labeled '{{LABEL}}':"
        gh issue list --label "{{LABEL}}" --limit 20
    fi

# List all open issues by priority
issues-all:
    @echo "📊 All Open Issues by Priority:"
    @echo ""
    @echo "🔥 HIGH PRIORITY:"
    @gh issue list --label "high-priority" --limit 20
    @echo ""
    @echo "📌 MEDIUM PRIORITY:"
    @gh issue list --label "medium-priority" --limit 20
    @echo ""
    @echo "💭 DISCUSSION/FUTURE:"
    @gh issue list --label "discussion" --limit 20
