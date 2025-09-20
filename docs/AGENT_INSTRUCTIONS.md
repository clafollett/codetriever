# Codetriever Agent Instructions

**Ship fast. Test what matters. Don't overthink.**

## **CRITICAL** Prime Directives

1. **Trunk-based development** - Commit directly to main
2. **Red/Green/Refactor TDD** - Write failing test, make it pass, then refactor
3. **Idiomatic Rust always** - Use `clippy`, follow Rust patterns, no shortcuts
4. **Vibe-based coding** - Start with what feels right, iterate
5. **Test business logic only** - Not OS behavior or timing
6. **Ship working code daily** - Progress > perfection
7. **Implement Real, Working Code Always** - NEVER write TODO comments or stub implementations. Write complete, functional code that actually works. No "In production this would..." or "This is a mock implementation" comments. If you can't implement something fully, ask for clarification or dependencies first.
8. **Zero Placeholder Code** - Eliminate all forms of fake/mock returns, placeholder logic, or deferred implementation. Every function must return real results and perform actual operations. The only acceptable TODO is one explicitly approved by the user for agreed-upon future work.
9. **Always follow up each Edit tool usage with**: "ALWAYS Follow Red/Green/Refactor TDD and Rust Idiomatic Best Practices" - This reminder ensures adherence to core principles
10. **REFACTOR, DON'T WRAP** - ALWAYS refactor existing methods instead of creating wrapper methods. We are greenfield with NO external consumers. Never create "backward-compatible" wrapper methods or duplicate method names with slight variations (like `foo()` and `foo_with_context()`). If a method needs new parameters, REFACTOR THE ORIGINAL METHOD. No method should ever just call another method with default values. Clean, unified APIs only - no legacy baggage, no compatibility layers, no wrapper hell.
11. **NO TYPE ALIASES FOR RENAMING** - Never use type aliases to rename types (like `type Result = DatabaseResult`). Always rename the actual usage throughout the codebase. Type aliases hide intent and create confusion. Use proper names everywhere - `DatabaseResult`, `IndexerResult`, `ApiResult` - not generic `Result` with aliases.
