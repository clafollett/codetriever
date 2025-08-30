# codetriever

MCP (Model Context Protocol) server implementation using Rust and Axum.

## Features

- **HTTP Server**: Axum-based HTTP server with SSE (Server-Sent Events) support
- **MCP Protocol**: Full Model Context Protocol implementation using rmcp
- **Tool Support**: Define and expose tools for AI model interaction
- **Resource Management**: Serve resources with proper URI handling
- **Prompt Templates**: Support for dynamic prompt templates
- **Real-time Communication**: SSE for real-time bidirectional communication
- **Signal Handling**: Graceful shutdown with proper signal handling
- **Security**: Built-in security features for production deployments

## Security Features

This server includes security features:

- **Transport Security**: Secure SSE mode for web deployments with configurable bind address
- **Input Sanitization**: All inputs are properly validated and sanitized
- **Structured Logging**: JSON logging prevents log injection attacks  
- **Graceful Shutdown**: Proper cleanup on termination signals
- **Error Handling**: Secure error messages that don't leak implementation details

## Usage

### Basic Usage

```bash
# Start the server in STDIO mode (default)
cargo run

# Start the server in SSE mode with HTTP endpoints
cargo run -- --transport sse --sse-addr 127.0.0.1:8080

# Start with custom configuration
cargo run -- --log-dir /var/log/mcp --api-url https://api.example.com

# Use a configuration file
cargo run -- --config-file server.toml

# Start with debug logging (standard Rust logging)
RUST_LOG=debug cargo run
```

### Configuration

All server configuration is done through command-line arguments:

```
codetriever [OPTIONS]

Options:
  -t, --transport <TRANSPORT>              Transport type to use [default: stdio] [possible values: stdio, sse]
      --sse-addr <SSE_ADDR>                SSE server bind address [default: 127.0.0.1:8080]
      --sse-keep-alive <SSE_KEEP_ALIVE>    SSE keep-alive interval in seconds [default: 30]
      --log-dir <LOG_DIR>                  Log directory path [default: logs]
      --api-url <API_URL>                  API URL for backend services [default: http://localhost:8080]
  -c, --config-file <CONFIG_FILE>          Optional configuration file path (TOML format)
  -h, --help                               Print help
  -V, --version                            Print version
```

### Transport Modes

**STDIO mode** (default):
- The server communicates via standard input/output
- Suitable for direct process communication
- Used by desktop MCP clients like Cursor/VS Code

**SSE mode**:
- The server exposes HTTP endpoints for SSE communication
- `/sse` - Server-Sent Events endpoint for real-time messages
- `/message` - POST endpoint for client messages
- Suitable for web-based clients and remote connections

### Configuration File

If you prefer to use a configuration file (specified with `--config-file`), create a TOML file:

```toml
log_dir = "logs"
api_url = "http://localhost:8080"
transport = "stdio"
sse_addr = "127.0.0.1:8080"
sse_keep_alive = 30
```

Note: Command-line arguments always override configuration file settings.

## API Endpoints

### MCP Protocol Endpoints

- `POST /mcp/sse` - Server-Sent Events endpoint for MCP communication
- `GET /health` - Health check endpoint
- `GET /schema` - OpenAPI schema endpoint

### Development Endpoints

- `GET /docs` - Swagger UI documentation (development only)

## Implementation

### Adding Tools

Tools are defined in `src/handlers/` and registered in `src/main.rs`:

```rust
use agenterra_rmcp::prelude::*;

#[tool]
async fn my_tool(
    #[description("Input parameter")] input: String,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    Ok(format!("Processed: {}", input))
}
```

### Adding Resources

Resources are managed through the MCP protocol and can represent files, databases, or any accessible data:

```rust
use agenterra_rmcp::prelude::*;

async fn list_resources() -> Vec<Resource> {
    vec![
        Resource {
            uri: "file:///example.txt".to_string(),
            name: Some("Example File".to_string()),
            description: Some("An example resource".to_string()),
            mime_type: Some("text/plain".to_string()),
        }
    ]
}
```

### Adding Prompts

Prompt templates can be defined for dynamic content generation:

```rust
use agenterra_rmcp::prelude::*;

async fn get_prompt(name: &str, args: &serde_json::Value) -> Option<PromptMessage> {
    match name {
        "example" => Some(PromptMessage {
            role: MessageRole::User,
            content: MessageContent::Text("Example prompt".to_string()),
        }),
        _ => None,
    }
}
```

## Dependencies

- **rmcp**: MCP protocol implementation
- **axum**: HTTP web framework
- **tokio**: Async runtime
- **serde**: Serialization support
- **schemars**: JSON schema generation
- **tracing**: Logging and instrumentation
- **signal-hook**: Signal handling for graceful shutdown

## Publishing Limitation

⚠️ **Important**: This project uses git dependencies and cannot be published to crates.io in its current form.

The `rmcp` dependency is referenced directly from the official ModelContextProtocol GitHub repository to ensure access to the latest features (including authentication support). To publish this project to crates.io, you would need to:

1. Wait for an official `rmcp` release on crates.io that includes the `auth` feature
2. Or remove features that require git dependencies
3. Or vendor the dependencies locally

This limitation ensures you have access to the full, official MCP SDK functionality during development.

## Development

### Building

```bash
cargo build
```

### Running Tests

```bash
cargo test
```

### Formatting

```bash
cargo fmt
```

### Linting

```bash
cargo clippy
```

### Running with Hot Reload

For development, you can use `cargo watch`:

```bash
cargo install cargo-watch
cargo watch -x run
```

## Project Structure

```
codetriever/
├── Cargo.toml          # Rust project manifest
├── src/
│   ├── handlers/        # MCP request handlers
│   │   ├── mod.rs       # Handler module exports
│   │   └── {endpoint}.rs # Individual endpoint handlers
│   ├── schemas/         # JSON schema files (created during generation)
│   ├── common.rs        # Common utilities and error handling
│   ├── config.rs        # Server configuration
│   ├── server.rs        # MCP server implementation
│   ├── signal.rs        # Signal handling for graceful shutdown
│   ├── transport.rs     # Transport layer (STDIO/SSE)
│   └── main.rs          # Server entry point
├── .env                # Environment variables
└── README.md           # Project documentation
```

The structure is organized as follows:

- **`handlers/`** - Contains all MCP tool implementations, one file per endpoint
- **`schemas/`** - JSON schema files for tool parameters (auto-generated)
- **`common.rs`** - Shared utilities for API communication and error handling
- **`config.rs`** - Configuration management and command-line parsing
- **`server.rs`** - Core MCP server implementation with protocol handling
- **`signal.rs`** - Signal handling for graceful shutdown (SIGTERM, SIGINT)
- **`transport.rs`** - Transport layer supporting both STDIO and SSE modes
- **`main.rs`** - Application entry point and server initialization

## Production Deployment

### Building Release Binary

```bash
cargo build --release
```

### Docker Deployment

Create a `Dockerfile`:

```dockerfile
FROM rust:1.70 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/codetriever /usr/local/bin/server
EXPOSE 3000
CMD ["server"]
```

### Environment Configuration

For production, consider:

- Setting appropriate `RUST_LOG` levels
- Configuring proper error handling
- Setting up monitoring and metrics
- Implementing rate limiting
- Adding authentication middleware

## License

This project is licensed under the MIT License.