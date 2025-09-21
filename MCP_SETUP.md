# MCP (Model Context Protocol) Setup Guide

⚠️ **WARNING: MCP integration is UNTESTED and may not work at all!** This is alpha software built in 2 weeks.

## What is MCP?

MCP is Anthropic's open protocol that lets AI assistants like Claude use tools. Codetriever has an MCP server that was scaffolded by Agenterra but hasn't been properly tested yet.

## Quick Setup for Claude Code

1. **Build and install Codetriever**:
```bash
cargo install --path crates/codetriever
```

2. **Edit Claude Code config**:

Open VS Code settings and add to MCP servers configuration. See [Claude Code MCP documentation](https://docs.claude.com/en/docs/claude-code/mcp) for details.

3. **Add Codetriever to MCP servers**:
```json
{
  "mcpServers": {
    "codetriever": {
      "command": "codetriever",
      "args": ["mcp"],
      "env": {
        "DATABASE_URL": "postgresql://codetriever:codetriever@localhost:5432/codetriever",
        "QDRANT_URL": "http://localhost:6334"
      }
    }
  }
}
```

4. **Start Docker services**:
```bash
just docker-up
```

5. **Reload VS Code window** or restart VS Code

6. **Use with Claude Code**:
```
Use the codetriever_index tool to index /path/to/my/project
```

## Available MCP Tools

Once configured, Claude can use these tools:

- `codetriever_index` - Index a repository
- `codetriever_search` - Search for code
- `codetriever_similar` - Find similar code chunks
- `codetriever_context` - Get context around a code location
- `codetriever_usages` - Find usages of a symbol
- `codetriever_status` - Check indexing status
- `codetriever_stats` - Get statistics
- `codetriever_clean` - Clean up old data
- `codetriever_compact` - Compact storage

## Other MCP Clients

### Cline (VS Code Extension)
Configure in VS Code settings under Cline MCP servers. Cline is another popular AI coding assistant for VS Code.

### Continue.dev
Add to `~/.continue/config.json`:
```json
{
  "mcpServers": [
    {
      "name": "codetriever",
      "command": "codetriever",
      "args": ["mcp"]
    }
  ]
}
```

### Zed Editor
Add to `~/.config/zed/settings.json`:
```json
{
  "language_models": {
    "mcp_servers": {
      "codetriever": {
        "command": "codetriever",
        "args": ["mcp"]
      }
    }
  }
}
```

### Claude Desktop
While less common for development, Claude Desktop also supports MCP. Config location:
- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`

## Troubleshooting

### "Connection refused" errors
- Ensure Docker is running: `docker ps`
- Start services: `just docker-up`
- Check logs: `just docker-logs`

### "Command not found"
- Ensure Rust bin directory is in PATH
- Add to shell profile: `export PATH="$HOME/.cargo/bin:$PATH"`

### Memory issues
- Increase Docker memory allocation (4GB minimum)
- Reduce batch size in indexing

## Environment Variables

Optional configuration:
```bash
# Custom database URL
export DATABASE_URL="postgresql://user:pass@host:5432/db"

# Custom Qdrant URL
export QDRANT_URL="http://localhost:6334"

# Logging level
export RUST_LOG="info"
```

## Testing Your Setup

1. Start Docker services:
```bash
just docker-up
```

2. Test MCP server directly:
```bash
codetriever mcp
```

3. Send a test request (in another terminal):
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | codetriever mcp
```

You should see a list of available tools in JSON format.

---

*Need help? Open an issue on GitHub!*