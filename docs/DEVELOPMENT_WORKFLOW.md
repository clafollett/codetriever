# Codetriever Development Workflow

**Vibe-based, trunk-based, ship daily.**

## Philosophy

1. **Start building, not planning** - Code teaches us what we need
2. **Ship to main** - No branches, no PRs initially  
3. **Test the happy path** - Edge cases can wait
4. **Iterate in public** - Commit WIP, fix forward

## Daily Flow

### Morning
```bash
git pull
cargo test
# Fix any breaks from overnight
```

### Building
```bash
# Try something
cargo run

# Works? Ship it
git add -A && git commit -m "feat: tried something" && git push

# Broken? Ship it anyway with a fix coming
git add -A && git commit -m "wip: exploring approach" && git push
```

### Evening
```bash
# Tag working versions
git tag v0.0.x -m "Works for basic use case"
git push --tags
```

## GitHub Issues

Keep them simple:
```markdown
## Add semantic search
- Parse query
- Search vectors  
- Return results
```

Not novels. Bullet points.

## When to Branch

Only when you must:
- `spike/crazy-idea` - Experimental rewrites
- `break/v2` - Breaking API changes

Merge fast or delete.

## Code Standards

### Rust Basics
```rust
// Use Result everywhere
fn search(query: &str) -> Result<Vec<Match>> {
    // Happy path first
    let results = do_search(query)?;
    Ok(results)
}

// Tests for business logic only
#[test]
fn test_search_finds_matches() {
    let results = search("test").unwrap();
    assert!(!results.is_empty());
}
```

### Don't
- Write elaborate docs before coding
- Create detailed specifications
- Plan perfect architectures
- Wait for consensus

### Do
- Ship something that works
- Get feedback from usage
- Iterate based on pain points
- Document what exists

## Multi-Agent Coordination

When you need parallel work:

```bash
# Main agent continues on trunk
git checkout main

# Spawn specialized agent with worktree
git worktree add /tmp/codetriever-ui frontend-ui
# Let frontend-engineer work there

# They ship directly to main when ready
cd /tmp/codetriever-ui
git add -A && git commit -m "feat: add search UI" && git push
```

## Release Process

When it works well enough:

```bash
# Bump version
cargo bump patch

# Tag it
git tag v0.1.0 -m "First usable version"

# Push
git push && git push --tags

# Maybe write release notes, maybe not
```

## Metrics That Matter

- **Did we ship today?** ✓
- **Does it work for us?** ✓  
- **Is it faster than yesterday?** ✓

Not:
- Code coverage %
- Documentation completeness
- Perfect test suites

## Quick Commands

```bash
# Dev loop
cargo watch -x test -x run

# Format and ship
cargo fmt && git add -A && git commit -m "style: fmt" && git push

# Check benchmarks
cargo bench

# See what changed
git log --oneline -10
```

## Remember

We're building a tool we need RIGHT NOW at RealManage. Not a perfect platform for hypothetical future users.

**Ship. Learn. Iterate. Repeat.**

That's the workflow. 🚀