# MAOS Development Workflow

This document defines the systematic development process for the Multi-Agent Orchestration System (MAOS), emphasizing thorough planning, detailed issue tracking, and comprehensive code review.

## Table of Contents

1. [Core Principles](#core-principles)
2. [Planning Phase](#planning-phase)
3. [GitHub Project Setup](#github-project-setup)
4. [Issue Creation Standards](#issue-creation-standards)
5. [Development Process](#development-process)
6. [Code Review Process](#code-review-process)
7. [Documentation Requirements](#documentation-requirements)
8. [Quality Gates](#quality-gates)

## Core Principles

1. **Plan Before Code** - No implementation without comprehensive planning
2. **Issue-Driven Development** - Every change tracked through detailed GitHub issues
3. **Specification First** - Complete specs before implementation
4. **Multi-Agent Review** - Primary development by one agent, review by another
5. **Documentation as Code** - All plans versioned in markdown

## Planning Phase

### 1. Architecture Decision Records (ADRs)

Before any major implementation:

```markdown
docs/architecture/decisions/
├── 001-use-hook-based-orchestration.md
├── 002-json-over-sqlite.md
├── 003-git-worktree-isolation.md
└── 004-session-based-coordination.md
```

**ADR Template:**
```markdown
# ADR-XXX: [Decision Title]

## Status
[Proposed | Accepted | Deprecated]

## Context
What is the issue we're addressing?

## Decision
What have we decided to do?

## Consequences
What are the positive and negative outcomes?

## Alternatives Considered
What other options did we evaluate?
```

### 2. Hook System Documentation

```markdown
docs/hooks/
├── lifecycle/
│   ├── pre_tool_use.md
│   ├── post_tool_use.md
│   └── session_management.md
├── patterns/
│   ├── tool-interception.md
│   ├── agent-isolation.md
│   └── error-handling.md
└── security/
    └── dangerous-operations.md
```

### 3. Technical Specifications

```markdown
docs/specifications/
├── hooks/
│   ├── hook-api.md
│   └── tool-handlers.md
├── coordination/
│   └── session-files.md
└── agents/
    └── agent-definitions.md
```

## GitHub Project Setup

### 1. Project Board Structure

**Columns:**
1. **📋 Backlog** - All planned work
2. **🎯 Ready** - Fully specified, ready for development
3. **🚧 In Progress** - Currently being implemented
4. **👀 In Review** - PR submitted, awaiting review
5. **✅ Done** - Merged to main

### 2. Labels System

**Type Labels:**
- `type:feature` - New functionality
- `type:bug` - Defect fix
- `type:refactor` - Code improvement
- `type:docs` - Documentation
- `type:test` - Test additions
- `type:chore` - Maintenance

**Component Labels:**
- `component:hooks` - Hook system
- `component:agents` - Agent definitions
- `component:orchestration` - Orchestration logic
- `component:testing` - Test infrastructure

**Priority Labels:**
- `priority:critical` - Must have
- `priority:high` - Should have
- `priority:medium` - Could have
- `priority:low` - Nice to have

**Status Labels:**
- `status:needs-spec` - Requires specification
- `status:ready` - Ready for development
- `status:blocked` - Waiting on dependency

### 3. Milestones

```
v0.1.0 - Hook Foundation
├── Basic hook system
├── Tool interception
├── Session management
└── Integration tests

v0.2.0 - Agent Orchestration
├── Worktree isolation
├── Multi-agent coordination
└── Lock management

v0.3.0 - Production Ready
├── Performance optimization
├── Comprehensive testing
├── Documentation
└── Error recovery
```

## Issue Creation Standards

### Issue Template

```markdown
---
name: Development Task
about: Standard template for development tasks
title: '[Component] Brief description'
labels: 'type:feature, component:hooks, priority:high'
assignees: ''
---

## Overview
Brief description of what needs to be implemented.

## Acceptance Criteria
- [ ] Specific, measurable outcome 1
- [ ] Specific, measurable outcome 2
- [ ] All tests pass
- [ ] Documentation updated

## Technical Specification

### Hook Implementation (if applicable)
```rust
use maos_security::SecurityValidator;
use serde_json::Value;

pub fn pre_tool_use(tool_name: &str, tool_args: &Value) -> Result<HookResponse> {
    // Intercept tool before execution
    match tool_name {
        "Task" => handle_task_spawn(tool_args),
        "Edit" | "Write" => enforce_workspace(tool_args),
        _ => Ok(HookResponse::allow()),
    }
}
```

### Coordination Files (if applicable)
```json
{
  "session_id": "sess-123",
  "agents": [],
  "locks": {}
}
```

### Test Scenarios
1. **Happy Path**
   - Test: Hook intercepts tool correctly
   - Expected: Tool modified as intended

2. **Error Cases**
   - Test: Invalid tool arguments
   - Expected: Graceful error handling

3. **Edge Cases**
   - Test: Concurrent hook execution
   - Expected: Thread-safe operation

## Implementation Notes
- Follow Rust best practices and idioms
- Use TDD approach with `cargo test --watch`
- Consider memory safety and ownership
- Handle errors with Result<T, E> types
- Use async/await for concurrent operations

## Dependencies
- Blocked by: #[issue number]
- Blocks: #[issue number]

## Definition of Done
- [ ] Implementation complete
- [ ] Unit tests written and passing
- [ ] Integration tests (if applicable)
- [ ] Documentation updated
- [ ] Code reviewed by another agent
- [ ] PR merged to main
```

### Epic Template

```markdown
---
name: Epic
about: Large feature spanning multiple issues
title: 'EPIC: [Feature Name]'
labels: 'epic'
---

## Epic Overview
High-level description of the feature.

## Business Value
Why are we building this?

## Technical Approach
How will we implement it?

## Child Issues
- [ ] #1 - Setup hook infrastructure
- [ ] #2 - Implement tool handlers
- [ ] #3 - Add session management
- [ ] #4 - Create integration tests
- [ ] #5 - Write documentation

## Acceptance Criteria
- [ ] All child issues complete
- [ ] End-to-end tests pass
- [ ] Performance benchmarks met
- [ ] Documentation complete
```

## Development Process

### 1. Pre-Development Checklist

Before starting any issue:
- [ ] Issue has complete specification
- [ ] Acceptance criteria are clear
- [ ] Dependencies resolved
- [ ] Technical approach reviewed

### 2. Development Flow

```mermaid
graph LR
    A[Pick Issue] --> B[Create Branch]
    B --> C[Write Failing Tests]
    C --> D[Implement Code]
    D --> E[Refactor]
    E --> F[Update Docs]
    F --> G[Create PR]
    G --> H[Code Review]
    H --> I[Merge]
```

### 3. Branch Naming

Format: `<type>/issue-<number>/<brief-description>`

Examples:
- `feature/issue-1/hook-interception`
- `fix/issue-23/worktree-cleanup`
- `docs/issue-45/api-specification`

### 4. Commit Standards

#### Conventional Commit Format
Use semantic commit messages with GitHub issue linking:

**Format:** `<type>: <description> (#<issue_number>)`

**Types:**
- `feat:` - New features
- `fix:` - Bug fixes
- `chore:` - Maintenance tasks (Cargo.toml updates, dependencies)
- `docs:` - Documentation updates
- `refactor:` - Code refactoring
- `test:` - Adding/updating tests
- `perf:` - Performance improvements
- `style:` - Code formatting (cargo fmt)

**Breaking Changes:** Add `BREAKING CHANGE:` in commit body for major changes

> **Note:** Only use automatic closing keywords (e.g., `Closes #57`) in the *final* commit or pull-request description when the issue is *fully resolved*. For intermediate work, reference the issue without closing it, e.g., `Relates to #57`, `Refs #57`, or simply `(#57)`.

**Examples:**
- `feat: implement pre_tool_use hook (#15)`
- `fix: resolve worktree creation race condition (#23)`
- `chore: update dependencies (#8)`
- `docs: add hook documentation (#42)`

### 5. Branch Protection Rules

**Repository Protection Settings:**
- **No direct pushes** - All changes via pull requests
- **Required status checks:**
  - `Rust Tests (cargo test)`
  - `Clippy Lints` (see [Linting Guide](features/linting.md))
  - `Format Check (cargo fmt)`
  
- **Required reviews** - At least 1 approving review
- **Dismiss stale reviews** - Re-approval required after new commits
- **Require conversation resolution** - All review comments must be resolved

## Code Review Process

### 1. Pull Request Template

```markdown
## Summary
Brief description of changes.

## Related Issue
Closes #[issue number]

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Refactoring
- [ ] Documentation

## Testing
- [ ] Unit tests pass
- [ ] Integration tests pass
- [ ] Manual testing completed

## Checklist
- [ ] Code follows Rust conventions
- [ ] Tests follow TDD approach
- [ ] Documentation updated
- [ ] No hardcoded paths
- [ ] Error handling uses Result<T, E>
- [ ] No unwrap() in production code

## Screenshots (if applicable)
[Add any relevant screenshots]
```

### 2. Review Criteria

**Automated Checks:**
- All CI/CD checks pass
- Test coverage maintained
- No linting errors

**Manual Review Points:**
- Clean code principles followed
- Error handling comprehensive
- Tests meaningful
- Documentation clear

### 3. Review Assignment

- Primary developer: Implements the issue
- Code reviewer: Different agent
- Final approval: Repository owner

## Documentation Requirements

### 1. Code Documentation

```rust
/// Register a new agent in the session.
///
/// # Arguments
///
/// * `session_id` - Unique session identifier
/// * `agent_name` - Name of the agent to register
/// * `worktree_path` - Path to agent's git worktree
///
/// # Returns
///
/// Agent registration details
///
/// # Errors
///
/// Returns an error if the agent is already registered
///
/// # Example
///
/// ```
/// let details = register_agent("sess-123", "backend-engineer", "/tmp/worktrees/backend")?;
/// assert_eq!(details.status, AgentStatus::Active);
/// ```
pub fn register_agent(
    session_id: &str,
    agent_name: &str,
    worktree_path: &Path,
) -> Result<AgentDetails> {
    // Implementation
}
```

### 2. Architecture Documentation

Each component must have:
- README.md explaining purpose
- Architecture diagram (if complex)
- Usage examples
- Configuration options

### 3. Decision Documentation

Major decisions recorded in:
- ADRs for architecture choices
- Comments for non-obvious code
- Issues for implementation decisions

## Quality Gates

### 1. Definition of Ready

Issue is ready for development when:
- [ ] Specification complete
- [ ] Acceptance criteria defined
- [ ] Technical approach documented
- [ ] Dependencies identified
- [ ] Estimated and prioritized

### 2. Definition of Done

Issue is complete when:
- [ ] All acceptance criteria met
- [ ] Tests written and passing
- [ ] Documentation updated
- [ ] Code reviewed and approved
- [ ] CI/CD checks pass
- [ ] Merged to main

### 3. Release Criteria

Version is ready for release when:
- [ ] All milestone issues complete
- [ ] Integration tests pass
- [ ] Performance benchmarks met
- [ ] Documentation complete
- [ ] CHANGELOG updated
- [ ] Release notes prepared

## Rust Development Standards

### Module Organization
```rust
// 1. External crates
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;

// 2. Standard library
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// 3. Local modules
use crate::config::Config;
use crate::hooks::HookHandler;
use crate::orchestration::WorkspaceManager;
```

### Testing Standards

#### Test Structure
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_tool_interception() {
        // Given
        let tool_name = "Task";
        let tool_args = json!({
            "subagent_type": "backend-engineer"
        });
        
        // When
        let result = pre_tool_use(tool_name, &tool_args).unwrap();
        
        // Then
        assert!(result.intercepted);
        assert!(result.worktree.is_some());
    }
    
    #[tokio::test]
    async fn test_async_operations() {
        // Test async hook processing
        let result = process_hook_async().await;
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_error_handling() -> Result<()> {
        // Test graceful error handling
        let result = risky_operation()?;
        Ok(())
    }
}
```

### String Interpolation & Formatting

- Prefer inline captured identifiers introduced in Rust 1.58+.
- Use `format!("{var}")` instead of `format!("{}", var)`.
- Applies to: `format!`, `println!`, logging macros (`info!`, `error!`, etc.), `panic!`, `anyhow!` messages, `assert!` messages, and `format_args!`.
- Enforce via Clippy: `cargo clippy -- -D clippy::uninlined-format-args`.

Good examples:
```rust
let user = "alice";
let s = format!("hello {user}");
info!("processing user: {user}");
println!("result: {user}");
```

Bad examples:
```rust
let user = "alice";
let s = format!("hello {}", user); // avoid
info!("processing user: {}", user); // avoid
println!("result: {}", user); // avoid
```

### Logging Standards
```rust
use log::{debug, error, info, warn};
use tracing::{event, instrument, Level};

// Using log crate
info!("Loading hook configuration for: {hook_name}");
error!("Failed to create worktree for agent: {agent_name}");

// Using tracing for structured logging
#[instrument]
fn process_hook(hook_name: &str) -> Result<()> {
    event!(Level::INFO, hook = hook_name, "Processing hook");
    // Implementation
    Ok(())
}

// Bad - using println!
println!("Loading configuration"); // Don't use this
```

## Development Commands

### Quick Commands Reference
```bash
# Run tests
cargo test
cargo test test_hooks -- --nocapture
cargo test --workspace

# Formatting and linting
cargo fmt
cargo clippy -- -D warnings
cargo clippy --fix -Z unstable-options  # Auto-fix clippy warnings
cargo check

# Build and run
cargo build --release
cargo run --bin maos -- pre-tool-use

# Documentation
cargo doc --open
cargo doc --no-deps

# Benchmarks
cargo bench
cargo bench -- --save-baseline main
```

## Getting Started

1. Review this workflow document
2. Check the GitHub project board
3. Pick an issue marked `status:ready`
4. Follow the development process
5. Submit PR for review

Remember: **Plan twice, code once!**

---

*This workflow ensures systematic, high-quality development of MAOS with minimal refactoring needs.*