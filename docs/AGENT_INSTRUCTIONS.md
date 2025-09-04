# Codetriever Agent Instructions

**Ship fast. Test what matters. Don't overthink.**

## **CRITICAL** Prime Directives

1. **ALWAYS follow [DEVELOPMENT_WORKFLOW.md](./DEVELOPMENT_WORKFLOW.md)** - All standards and processes defined there
2. **Trunk-based development** - Commit directly to main
3. **Red/Green/Refactor TDD** - Write failing test, make it pass, then refactor
4. **Idiomatic Rust always** - Use `clippy`, follow Rust patterns, no shortcuts
5. **Vibe-based coding** - Start with what feels right, iterate
6. **Test business logic only** - Not OS behavior or timing
7. **Ship working code daily** - Progress > perfection
8. **Use actionable TODO comments** - Write `// TODO: <specific action>` not vague comments like "currently" or "not implemented". Be explicit about what needs to be done
9. **End every coding response with**: "ALWAYS Follow Red/Green/Refactor TDD and Rust Idiomatic Best Practices" - This reminder ensures adherence to core principles

## Technical Requirements

- **Rust Edition**: Use Rust 2024 edition for all crates
- **Workspace**: Multi-crate workspace structure
- **Dependencies**: Define shared deps at workspace level
- **Testing**: Red/Green/Refactor TDD cycle for all features
- **Code Quality**: Must pass `cargo clippy` and `cargo fmt`
- **Patterns**: Use Result<T, E>, Option<T>, iterators, and proper error handling
- **Module Organization**: `lib.rs` and `mod.rs` files should ONLY contain imports/exports. All types, functions, and logic go in dedicated module files

## Development Flow

```bash
# Morning
git pull
just test

# Code (TDD cycle)
just tdd-watch        # Watch mode for Red/Green/Refactor
vim src/whatever.rs
just quality          # Format, lint, test

# Fix issues
just fmt              # Format code
just lint             # Run clippy
just clippy-fix       # Auto-fix clippy issues
just fix              # Fix all auto-fixable issues

# Ship it
git add -A
git commit -m "feat: add thing that works"
git push
```

## Essential Just Commands

```bash
# Development
just dev              # Start native dev environment
just dev-docker       # Start Docker environment
just stop             # Stop all services

# Testing (TDD)
just tdd              # Run tests with output
just tdd-watch        # Watch mode for TDD cycle
just test             # Run all tests
just test-codetriever # Run workspace tests

# Code Quality
just quality          # Run fmt + lint + test (pre-commit)
just fmt              # Format all code
just lint             # Run clippy with warnings as errors
just clippy-fix       # Auto-fix clippy issues
just fix              # Apply all auto-fixes

# Quick Checks
just quick            # Format check + lint + compile check
just check            # Check compilation without building

# Setup
just dev-setup        # Complete dev environment setup
just install-hooks    # Install git pre-commit hooks
just validate-stack   # Validate environment configuration
```

## What to Test

✅ **DO TEST:**
- Our business logic works
- Error handling is solid  
- Public APIs return expected results

❌ **DON'T TEST:**
- File system operations
- Network calls
- Timing/performance
- Memory allocation precision

## Commit Style

Keep it simple:
- `feat: add vector search`
- `fix: handle empty queries`
- `perf: cache embeddings`
- `docs: update examples`

Don't overthink it.

## When to Use Other Agents

If you need parallel work on independent components:
- `backend-engineer` - API/server work
- `frontend-engineer` - UI components  
- `qa-engineer` - Test suite expansion
- `tech-writer` - Documentation

Use worktrees to isolate their work if needed.

## Remember

- **We ARE the users** - Build what we need
- **Local-first always** - No cloud dependencies
- **Rust for speed** - But Python bindings are fine for prototyping
- **Ship daily** - Small improvements compound

That's it. Now go build something. 🚀