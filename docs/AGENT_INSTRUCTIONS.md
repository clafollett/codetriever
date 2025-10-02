# Codetriever Agent Instructions

**Ship fast. Test what matters. Don't overthink.**

## **CRITICAL** Prime Directives

1. **Red/Green/Refactor TDD** - Write failing test, make it pass, then refactor
2. **Idiomatic Rust always** - Use `just fix`, `just check` often. Always follow idiomatic Rust patterns and Best Practices! NO SHORTCUTS!
3. **Vibe-based coding** - Start with what feels right, iterate
4. **ALWAYS Test business logic only** - Not OS behavior or timing
5. **Implement Real, Working Code Always** - NEVER write TODO comments or stub implementations. Write complete, functional code that actually works. No "In production this would..." or "This is a mock implementation" comments. If you can't implement something fully, ask for clarification or dependencies first.
6. **Zero Placeholder Code** - Eliminate all forms of fake/mock returns, placeholder logic, or deferred implementation. Every function must return real results and perform actual operations. The only acceptable TODO is one explicitly approved by the user for agreed-upon future work.
7. **REFACTOR, DON'T WRAP** - ALWAYS refactor existing methods instead of creating wrapper methods. We are greenfield with NO external consumers. Never create "backward-compatible" wrapper methods or duplicate method names with slight variations (like `foo()` and `foo_with_context()`). If a method needs new parameters, REFACTOR THE ORIGINAL METHOD. No method should ever just call another method with default values. Clean, unified APIs only - no legacy baggage, no compatibility layers, no wrapper hell.
8. **NO TYPE ALIASES FOR RENAMING** - Never use type aliases to rename types (like `type Result = DatabaseResult`). Always rename the actual usage throughout the codebase. Type aliases hide intent and create confusion. Use proper names everywhere - `DatabaseResult`, `IndexerResult`, `ApiResult` - not generic `Result` with aliases.
9. **USE PARAMETERS, DON'T HIDE THEM** - Never prefix parameters with underscores to silence warnings unless their usage is truly not needed. If a parameter exists, USE IT PROPERLY for logging, tracing, or functionality. Hiding parameters with `_` is often a sign of incomplete implementation. Ask if parameters should be used or removed entirely.
10. **ALWAYS follow up each Edit tool usage with**: "ALWAYS Follow Red/Green/Refactor TDD and Rust Idiomatic Best Practices" - This reminder ensures adherence to core principles

## GitHub Issue Workflow (Stay Focused & Organized)

### Starting New Work
1. **Pick an issue** from GitHub (prioritized by labels)
2. **Create feature branch**: `git checkout -b feature/issue-N-short-name`
3. **Update TodoWrite**: Track the issue as active task
4. **Reference in commits**: Always include `Ref #N` in commit messages

### Branch Naming Convention
```bash
# Format: feature/issue-NUMBER-short-description
git checkout -b feature/issue-28-status-endpoint
git checkout -b feature/issue-15-usages-endpoint
git checkout -b fix/issue-42-search-performance
```

### Commit Message Format (Template at `.gitmessage`)
```
feat|fix|docs|refactor|test|chore: [short description]

[Detailed description - what changed and why]

Ref #[issue-number]

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>
```

### Merging to Main (Fast Local Merge)
```bash
# When feature is done and tested
git checkout main
git merge feature/issue-N-short-name --no-ff
git push

# Branch reminds you which issue you're on
# Commits link back to GitHub automatically
# No PR overhead, no async review lag
```

### Why This Works
- ✅ Branch name = constant reminder of current issue
- ✅ Commit refs link to GitHub for tracking
- ✅ TodoWrite shows active work status
- ✅ Fast local merges (no PR wait)
- ✅ Clean history with issue references
- ✅ Can push branches for backup without PR process

### Trunk-Based Development Rules
1. **Small commits** - Ship incremental progress frequently
2. **Feature branches** - Use locally for organization, merge fast
3. **No long-lived branches** - Merge within 1-2 days max
4. **Pre-commit hooks** - Catch issues before they hit main
5. **Fast fixes** - If main breaks, fix forward immediately
6. **Issue refs** - Every commit links to GitHub issue for context

