# 🔍 Codetriever

**Local-first semantic code search for massive codebases. Built in Rust. Works with Claude Code.**

## What It Does

Codetriever gives AI agents semantic understanding of your entire codebase without sending code to the cloud.

```bash
# Index your codebase
codetriever index

# Search semantically from CLI
codetriever search "database connection pooling logic"

# Run as MCP server for Claude Code
codetriever serve --mcp
```

## Why It Exists

- **Context windows are limited** - Even 200k tokens can't fit enterprise codebases
- **Current tools miss context** - 65% of devs report AI misses critical patterns
- **Cloud solutions are expensive** - $200/month for Cursor? No thanks
- **Privacy matters** - Your code stays on your machine

## Installation

```bash
# From source (for now)
git clone https://github.com/clafollett/codetriever
cd codetriever
cargo install --path .

# Coming soon
cargo install codetriever
```

## Quick Start

```bash
# Initialize in your repo
cd /path/to/your/codebase
codetriever init

# Index everything
codetriever index

# Search for patterns
codetriever search "error handling"
codetriever search "authentication flow"

# Use with Claude Code (add to settings.json)
{
  "mcpServers": {
    "codetriever": {
      "command": "codetriever",
      "args": ["serve", "--mcp"]
    }
  }
}
```

## Features

- 🦀 **Pure Rust** - Fast, safe, single binary
- 🏠 **Local-first** - No cloud, no telemetry, your data
- 🚀 **Sub-10ms search** - Instant semantic results
- 🌳 **Git-aware** - Understands branches and history
- 🔌 **MCP native** - First-class Claude Code support
- 📦 **Zero dependencies** - Just works™

## Architecture

Codetriever runs as a persistent MCP server with embedded file watching and async indexing.

```
Your Code → Tree-sitter Parser → Semantic Chunks → Vector Embeddings → SQLite
     ↑                                                                   ↓
File Watcher                                                             ↓
(auto-index)                                                             ↓
     ↓                                                                   ↓
Claude Code ← MCP Protocol ← Search Results ← Similarity Search ←────────┘
```

**Key Design Decisions:**
- **Unified CLI/MCP interface** - Same tools everywhere
- **Embedded file watcher** - No separate process needed
- **Async indexing** - Never blocks queries (like SQL Server)
- **Incremental updates** - Only re-parse what changed

See [Architecture](docs/architecture.md) and [API Design](docs/api-design.md) for details.

## Development

```bash
# Setup
source stack.env
just dev-setup

# Run tests
cargo test

# Build
cargo build --release

# Watch mode
cargo watch -x test -x run
```

## Status

🚧 **Pre-Alpha** - We're building this in public. Expect rough edges.

- [x] Basic project structure
- [ ] MCP scaffolding with Agenterra
- [ ] Tree-sitter parsing
- [ ] Vector embeddings
- [ ] SQLite-vec storage
- [ ] Git integration
- [ ] Performance optimization

## Contributing

This is vibe-based development. We ship fast and iterate.

1. **No PRs initially** - Trunk-based development
2. **Commit early and often** - Small, focused changes
3. **Test the happy path** - Don't overthink edge cases yet
4. **Document what works** - Not what might work

## Philosophy

- **Simple > Clever** - Boring tech where possible
- **Fast > Perfect** - Ship iterations, not perfection  
- **Local > Cloud** - Privacy and performance
- **Open > Closed** - MIT licensed, no vendor lock-in

## Links

- [Initial Concept Discussion](docs/initial-claude-chat/codetriever-concept.md)
- [Market Research](docs/initial-claude-chat/code-intelligence-platforms-race-towards-ai-native-development.md)

## License

MIT - Use it, fork it, sell it. We don't care.

---

*Built with 🦀 and ☕ by developers who are tired of slow, expensive, cloud-dependent tools.*