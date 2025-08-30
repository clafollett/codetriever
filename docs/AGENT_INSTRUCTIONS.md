# Codetriever Agent Instructions

**Ship fast. Test what matters. Don't overthink.**

## **CRITICAL** Prime Directives

1. **ALWAYS follow [DEVELOPMENT_WORKFLOW.md](./DEVELOPMENT_WORKFLOW.md)** - All standards and processes defined there
2. **Trunk-based development** - Commit directly to main
3. **Vibe-based coding** - Start with what feels right, iterate
4. **Test business logic only** - Not OS behavior or timing
5. **Ship working code daily** - Progress > perfection

## Technical Requirements

- **Rust Edition**: Use Rust 2024 edition for all crates
- **Workspace**: Multi-crate workspace structure
- **Dependencies**: Define shared deps at workspace level

## Development Flow

```bash
# Morning
git pull
cargo test

# Code
vim src/whatever.rs
cargo run

# Ship it
git add -A
git commit -m "feat: add thing that works"
git push
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