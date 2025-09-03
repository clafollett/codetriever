# Codetriever Installation Strategy

**Single installer for binary + Docker services = seamless experience**

## The Challenge

We need to install:
1. A native binary (MCP/CLI) on the host
2. Docker containers (API + Qdrant)
3. Configuration and data directories

All with ONE installation mechanism that "just works".

## The Smart Installer Approach

### Installation Methods

#### Primary: Shell Script Installer
```bash
curl -sSL https://get.codetriever.dev | sh
```

What it does:
1. Detects OS and architecture
2. Downloads appropriate binary
3. Creates directory structure
4. Pulls Docker images
5. Sets up configuration
6. Optionally creates system service

#### Package Managers (Phase 2)
```bash
# macOS
brew install codetriever

# Linux
snap install codetriever
apt install codetriever  # Debian/Ubuntu
yum install codetriever  # RHEL/Fedora

# Windows
winget install codetriever
choco install codetriever
```

### Directory Structure

After installation:
```
~/.codetriever/
├── bin/
│   └── codetriever              # CLI/MCP binary
├── docker/
│   ├── docker-compose.yml       # Service definitions
│   └── .env                     # Docker environment
├── data/
│   ├── qdrant/                  # Vector DB persistence
│   └── indexes/                 # Cached indexes
├── logs/
│   ├── api.log                  # API server logs
│   └── mcp.log                  # MCP server logs
└── config.toml                  # User configuration
```

### The Smart Binary

The `codetriever` binary is intelligent about service management:

```rust
// Built-in service commands
codetriever start     // Start Docker services
codetriever stop      // Stop Docker services
codetriever status    // Check service health
codetriever logs      // View service logs
codetriever upgrade   // Update containers

// Regular commands auto-start services if needed
codetriever search "auth"  // Starts services if not running
```

### Auto-Start Logic

```rust
fn ensure_services_running() -> Result<()> {
    if !docker_services_healthy()? {
        if auto_start_enabled() {
            println!("Starting Codetriever services...");
            start_docker_services()?;
            wait_for_healthy(Duration::from_secs(30))?;
        } else {
            return Err("Services not running. Run 'codetriever start'");
        }
    }
    Ok(())
}
```

## Configuration Management

### Config File (`~/.codetriever/config.toml`)

```toml
[general]
auto_start = true               # Auto-start Docker services
telemetry = false               # No phone-home

[api]
endpoint = "http://localhost:8080"  # API server location
timeout = 30                        # Request timeout in seconds

[docker]
compose_file = "~/.codetriever/docker/docker-compose.yml"
auto_pull = true                # Auto-update images

[index]
paths = [                       # Paths to index
    "~/projects",
    "~/work"
]
ignore = [                      # Global ignore patterns
    "node_modules",
    "target",
    ".git",
    "*.pyc"
]
watch = true                    # Auto-index on file changes

[mcp]
transport = "stdio"             # MCP transport mode
log_level = "info"              # Logging verbosity
```

### Environment Variables

Override config with environment variables:
```bash
CODETRIEVER_API_ENDPOINT=http://remote:8080
CODETRIEVER_AUTO_START=false
CODETRIEVER_LOG_LEVEL=debug
```

## Claude Code Integration

### Automatic Setup

The installer adds to Claude Code settings:
```json
{
  "mcpServers": {
    "codetriever": {
      "command": "codetriever",
      "args": ["serve", "--mcp"],
      "env": {
        "CODETRIEVER_AUTO_START": "true"
      }
    }
  }
}
```

### First Launch Experience

1. User installs: `curl -sSL https://get.codetriever.dev | sh`
2. Opens Claude Code
3. Claude Code starts MCP server
4. MCP server auto-starts Docker services
5. Ready to search! No manual setup needed

## Installation Phases

### Phase 1: MVP (Week 1)
- Shell script installer
- Manual Claude Code config
- Basic service management
- GitHub releases

### Phase 2: Package Managers (Week 2-3)
- Homebrew formula
- Snap package
- Cargo crate
- Automated Claude Code setup

### Phase 3: Production (Week 4+)
- Native OS packages (deb, rpm, msi)
- Auto-updater
- System service integration
- Uninstaller

## Docker Compose Configuration

```yaml
version: '3.8'

services:
  api:
    image: codetriever/api:latest
    ports:
      - "8080:8080"
    environment:
      - QDRANT_URL=http://qdrant:6333
      - LOG_LEVEL=info
    depends_on:
      - qdrant
    restart: unless-stopped
    
  qdrant:
    image: qdrant/qdrant:latest
    ports:
      - "6333:6333"
    volumes:
      - ~/.codetriever/data/qdrant:/qdrant/storage
    restart: unless-stopped

networks:
  default:
    name: codetriever_net
```

## Upgrade Process

```bash
# Single command upgrade
codetriever upgrade

# What it does:
# 1. Downloads new binary
# 2. Pulls latest Docker images
# 3. Migrates configuration if needed
# 4. Restarts services
# 5. Verifies health
```

## Uninstall Process

```bash
# Clean uninstall
codetriever uninstall

# What it does:
# 1. Stops Docker services
# 2. Optionally removes Docker images
# 3. Optionally backs up data
# 4. Removes binary and directories
# 5. Cleans up PATH
```

## Platform-Specific Notes

### macOS
- Use `launchd` for service management
- Handle Docker Desktop requirements
- Code sign binary for Gatekeeper

### Linux
- Use `systemd` for service management
- Handle Docker permissions (docker group)
- Support both Docker and Podman

### Windows
- Use Windows Service for management
- Handle Docker Desktop or WSL2
- Provide PowerShell installer option

## Success Metrics

- **Install time**: < 2 minutes including Docker pulls
- **Time to first search**: < 30 seconds after install
- **Failure rate**: < 1% on supported platforms
- **Uninstall cleanliness**: Zero leftover files

## Why This Works

1. **Single entry point**: One installer for everything
2. **Smart defaults**: Works out-of-box for 90% of users
3. **Progressive disclosure**: Advanced config when needed
4. **Graceful degradation**: Falls back to manual mode
5. **Future-proof**: Ready for SaaS transition