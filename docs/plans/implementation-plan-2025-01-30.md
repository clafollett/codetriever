# Codetriever Implementation Plan
**Date:** 2025-01-30
**Author:** Marvin (with Claude)
**Scope:** Complete CLI/MCP/API Integration
**Methodology:** Test-Driven Development (Red/Green/Refactor)

## Executive Summary

This plan details the implementation of the missing pieces to complete the Codetriever system. The core indexing engine is production-ready, but the user interfaces (CLI and API endpoints) need to be completed. We'll use TDD methodology and git worktrees for parallel development.

## Current State Assessment

### ✅ What's Complete and Working
- **MCP Server**: 9 tools implemented via Agenterra scaffolding
- **Indexing Pipeline**: Full parsing, chunking, embeddings pipeline
- **Storage Layer**: PostgreSQL metadata + Qdrant vector storage
- **Docker Infrastructure**: Complete multi-service architecture
- **API Framework**: Axum server with routing structure
- **Index Endpoint**: Fully connected to indexer service

### ❌ What's Missing
- **Search API**: Returns empty stub `{"results": []}`
- **Other API Endpoints**: Not implemented (similar, context, usages, stats, clean, compact)
- **CLI Interface**: No CLI commands (only MCP server exists)
- **File Watching**: Complete TODO with no implementation

### 🎯 What This Plan Addresses
1. Complete all API endpoints with proper business logic
2. Add full CLI interface mirroring MCP tools
3. Ensure end-to-end functionality
4. Update documentation to match reality

## Implementation Phases

### Phase 0: Documentation Consolidation (Day 1 - Today)
**Goal:** Align documentation with reality before coding

#### Task 0.1: Update Architecture Documentation
- **File:** `docs/plans/architecture.md`
- **Actions:**
  - Mark "Embedded File Watcher" as TODO/future work
  - Update CLI/MCP section to reflect current state
  - Keep all Mermaid diagrams
  - Add note about Agenterra scaffolding

#### Task 0.2: Merge Current Architecture
- **Source:** `docs/architecture/current-architecture.md`
- **Target:** `docs/plans/architecture.md`
- **Keep:**
  - Component diagrams
  - Actual trait implementations
  - Storage architecture details

#### Task 0.3: Create Implementation Status
- **File:** `docs/IMPLEMENTATION_STATUS.md`
- **Content:**
  - Clear matrix of implemented vs planned
  - Links to relevant code
  - Progress tracking

### Phase 1: Search Endpoint Implementation (Day 2)
**Methodology:** Strict TDD - Red → Green → Refactor

#### Task 1.1: Write Failing Tests (RED Phase)
```rust
// crates/codetriever-api/src/routes/search.rs
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_search_returns_actual_results() {
        // Arrange: Mock indexer with known results
        // Act: Call search endpoint
        // Assert: Returns expected chunks, not empty
    }

    #[tokio::test]
    async fn test_search_handles_empty_query() { ... }

    #[tokio::test]
    async fn test_search_respects_limit() { ... }

    #[tokio::test]
    async fn test_search_handles_indexer_errors() { ... }
}
```

#### Task 1.2: Implement Minimum Code (GREEN Phase)
```rust
async fn search_handler(
    State(indexer): State<IndexerServiceHandle>,
    Json(req): Json<SearchRequest>,
) -> Json<SearchResponse> {
    // Minimum code to pass tests
}
```

#### Task 1.3: Refactor for Quality (REFACTOR Phase)
- Extract helper functions
- Add proper error handling
- Optimize performance
- Add logging/telemetry

### Phase 2: Git Worktrees Setup (Day 3)
**Goal:** Enable parallel development of remaining endpoints

#### Task 2.1: Create Feature Branches
```bash
git worktree add ../codetriever-similar feature/api-similar
git worktree add ../codetriever-context feature/api-context
git worktree add ../codetriever-usages feature/api-usages
git worktree add ../codetriever-status feature/api-status
git worktree add ../codetriever-cli feature/cli-implementation
```

#### Task 2.2: Parallel Agent Assignment
Each worktree can be developed in parallel:
- **Agent 1:** Similar endpoint
- **Agent 2:** Context endpoint
- **Agent 3:** Usages endpoint
- **Agent 4:** Status/Stats endpoints
- **Agent 5:** CLI implementation

### Phase 3: API Endpoints Implementation (Day 4-5)
**Each endpoint follows TDD cycle**

#### Task 3.1: `/similar` Endpoint
- **Request:** Code snippet to find similar
- **Logic:** Generate embedding → Search → Filter
- **Response:** Ranked similar chunks

#### Task 3.2: `/context` Endpoint
- **Request:** File path + line number
- **Logic:** Read file → Extract surrounding lines
- **Response:** Context with symbols

#### Task 3.3: `/usages` Endpoint
- **Request:** Symbol name
- **Logic:** Search for symbol in chunks
- **Response:** Definitions and references

#### Task 3.4: `/status` Endpoint
- **Request:** None
- **Logic:** Gather system metrics
- **Response:** Detailed status object

#### Task 3.5: `/stats` Endpoint
- **Request:** None
- **Logic:** Quick statistics query
- **Response:** File/chunk counts

#### Task 3.6: `/clean` Endpoint
- **Request:** Cleanup parameters
- **Logic:** Remove stale entries
- **Response:** Cleanup results

#### Task 3.7: `/compact` Endpoint
- **Request:** None
- **Logic:** Optimize storage
- **Response:** Compaction results

### Phase 4: CLI Implementation (Day 6)
**Goal:** Mirror all MCP tools as CLI commands

#### Task 4.1: Add Clap Framework
```rust
// crates/codetriever/src/main.rs
use clap::{Parser, Subcommand};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Mcp { ... },      // Existing
    Search { ... },   // New
    Index { ... },    // New
    Similar { ... },  // New
    // ... etc
}
```

#### Task 4.2: Implement Command Handlers
Each command calls the corresponding API endpoint:
```rust
async fn handle_search(query: &str, limit: usize) -> Result<()> {
    let response = api_client.search(query, limit).await?;
    print_search_results(response);
    Ok(())
}
```

### Phase 5: Integration Testing (Day 7)
**Goal:** Verify end-to-end functionality

#### Task 5.1: MCP → API Flow
- Start Docker services
- Run MCP server
- Execute each tool
- Verify responses

#### Task 5.2: CLI → API Flow
- Execute each CLI command
- Verify output formatting
- Test error scenarios

#### Task 5.3: Performance Testing
- Measure search latency
- Test concurrent requests
- Verify memory usage

### Phase 6: Documentation Finalization (Day 8)
**Goal:** Complete, accurate documentation

#### Task 6.1: Update README
- Installation instructions
- CLI usage examples
- MCP configuration

#### Task 6.2: API Documentation
- OpenAPI spec validation
- Example requests/responses
- Error codes

#### Task 6.3: Demo Creation
- Record video demo
- Create quickstart guide
- Publish examples

## Success Criteria

### Functional Requirements
- [ ] All 9 MCP tools return real data (not stubs)
- [ ] CLI commands mirror all MCP functionality
- [ ] Search returns actual results from Qdrant
- [ ] All API endpoints match OpenAPI spec
- [ ] Error handling is comprehensive

### Quality Requirements
- [ ] All code follows TDD (tests written first)
- [ ] Idiomatic Rust patterns used throughout
- [ ] No clippy warnings
- [ ] Test coverage > 80%
- [ ] Documentation complete and accurate

### Performance Requirements
- [ ] Search response < 100ms
- [ ] Index operation provides progress updates
- [ ] Memory usage < 500MB under load
- [ ] Concurrent request handling

## Risk Mitigation

### Risk 1: API/Indexer Integration Issues
- **Mitigation:** Start with search endpoint to validate pattern
- **Fallback:** Direct indexer calls if service trait issues

### Risk 2: Parallel Development Conflicts
- **Mitigation:** Clear interface boundaries per endpoint
- **Fallback:** Sequential development if conflicts arise

### Risk 3: Performance Degradation
- **Mitigation:** Benchmark each endpoint during development
- **Fallback:** Caching layer if needed

## Daily Checklist

### Day 1 (Today - Jan 30)
- [x] Create this planning document
- [ ] Update architecture.md
- [ ] Create IMPLEMENTATION_STATUS.md
- [ ] Review and commit documentation

### Day 2 (Jan 31)
- [ ] Write search endpoint tests (RED)
- [ ] Implement search endpoint (GREEN)
- [ ] Refactor search endpoint
- [ ] Test via MCP tool

### Day 3 (Feb 1)
- [ ] Set up git worktrees
- [ ] Create feature branches
- [ ] Assign parallel work
- [ ] Begin endpoint development

### Day 4-5 (Feb 2-3)
- [ ] Complete all API endpoints
- [ ] Run tests for each
- [ ] Merge feature branches

### Day 6 (Feb 4)
- [ ] Implement CLI framework
- [ ] Add all commands
- [ ] Test CLI/API integration

### Day 7 (Feb 5)
- [ ] Integration testing
- [ ] Performance testing
- [ ] Bug fixes

### Day 8 (Feb 6)
- [ ] Finalize documentation
- [ ] Create demo
- [ ] Final review

## Notes

### TDD Commitment
Every piece of functionality MUST follow:
1. **RED**: Write failing test first
2. **GREEN**: Write minimum code to pass
3. **REFACTOR**: Improve code quality

### Rust Best Practices
- Use `Result<T, Error>` everywhere
- Prefer `&str` over `String` for parameters
- Use `Arc<Mutex<>>` for shared state
- Follow clippy recommendations
- Document public APIs

### Git Workflow
- Feature branches for each endpoint
- Small, focused commits
- Descriptive commit messages
- PR reviews before merging

## Conclusion

This plan provides a structured approach to completing Codetriever's user interfaces. By following TDD and using parallel development, we can efficiently deliver a production-ready system while maintaining code quality.

**Let's build this right! 🚀**