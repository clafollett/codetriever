# Codetriever Architecture

**Local-first semantic code search with embedded file watching and async indexing.**

## Core Concepts

### 1. Tree-sitter = Parser, Not Watcher
Tree-sitter parses code into structured syntax trees. It understands code semantics:

```rust
// Your code
fn authenticate(user: &str, pass: &str) -> Result<Token> {
    let hash = bcrypt::hash(pass)?;
    database::verify(user, hash)
}

// Tree-sitter sees
FunctionDef {
    name: "authenticate",
    params: ["user: &str", "pass: &str"],
    return_type: "Result<Token>",
    body: BlockExpr { ... }
}
```

### 2. Processing Pipeline

```
FILE CHANGE → PARSE → CHUNK → EMBED → STORE → SEARCH
     ↓          ↓        ↓       ↓       ↓        ↓
  fsnotify  Tree-sitter  Smart  Vector  SQLite  Cosine
             parsing    splits  embed    -vec   similarity
```

### 3. Embeddings = Semantic Fingerprints

```
"user authentication" → [0.21, -0.43, 0.65, ...]
"fn authenticate(...)" → [0.23, -0.45, 0.67, ...]
Cosine similarity: 0.98 (very similar!)
```

## Architecture Decisions

### Unified CLI/MCP Interface
Every CLI command is also an MCP tool. One codebase, two interfaces:

```rust
// Core function
pub async fn search(query: &str, limit: usize) -> Result<Vec<CodeChunk>> {
    vector_db.search(query, limit)
}

// CLI wrapper
codetriever search "auth logic"

// MCP wrapper (same function!)
tools.search({ query: "auth logic" })
```

### Async Indexing (Like SQL Server)
MCP returns immediately, indexes in background. CLI waits for completion.

```rust
// MCP (async, non-blocking)
async fn mcp_index(args: IndexArgs) -> IndexJob {
    let job_id = spawn_background_index(args);
    IndexJob { id: job_id, status: "started" }
}

// CLI (sync, blocking)
fn cli_index(args: IndexArgs) -> Result<IndexStats> {
    do_index_sync(args)  // Waits for completion
}
```

### Embedded File Watcher
MCP server includes file watcher - no separate process needed:

```rust
fn main() {
    // 1. Load vector DB
    let db = load_vectors("./codetriever.db");
    
    // 2. Start watcher in background
    thread::spawn(|| {
        watch_files(|path| {
            if is_code_file(path) {
                queue_incremental_index(path);
            }
        });
    });
    
    // 3. Serve MCP requests
    mcp_server.listen();
}
```

### Incremental Updates
Only re-parse and re-embed what changed:

```rust
on_file_change(path) {
    let old_chunks = db.get_chunks(path);
    let new_chunks = tree_sitter.parse(path);
    
    for chunk in diff_chunks(old_chunks, new_chunks) {
        if chunk.changed() {
            let embedding = embed(chunk);
            db.update(chunk.id, embedding);
        }
    }
}
```

## Component Breakdown

### Parser (Tree-sitter)
- Language-aware parsing (Rust, Python, JS, etc.)
- Extracts functions, classes, modules
- Preserves semantic structure

### Chunker
- Smart splitting at semantic boundaries
- Preserves context (imports, class membership)
- Configurable chunk size (50-200 tokens typical)

### Embedder
- Local: Candle with CodeBERT
- Remote: OpenAI text-embedding-ada-002
- Caches embeddings for unchanged code

### Vector Store (Qdrant)
- Native Rust vector database
- Production-ready performance
- Rich features: filtering, payloads, snapshots
- Runs as Docker service for easy deployment

### File Watcher
- fsnotify for cross-platform monitoring
- Debounced updates (500ms default)
- Ignores non-code files (.git, node_modules, etc.)

### MCP Server
- Runs continuously when editor starts
- Handles queries + background indexing
- Returns JSON responses in <100ms

## Performance Targets

- **Initial index**: < 1 minute for 100K LOC
- **Incremental update**: < 100ms per file
- **Search response**: < 10ms for top-10 results
- **Memory usage**: < 500MB for 1M LOC
- **DB size**: ~100MB per 1M LOC

## Data Flow Examples

### Search Query
```
1. User: "authentication logic"
2. Embed query → [0.21, -0.43, ...]
3. Find similar vectors (cosine distance)
4. Return top-K chunks with metadata
5. Response time: <10ms
```

### File Change
```
1. Developer saves src/auth.rs
2. Watcher detects change (debounced)
3. Parse file with Tree-sitter
4. Diff against stored chunks
5. Re-embed changed functions only
6. Update vector DB
7. Ready for next query
```

### Background Index
```
1. MCP: tools.index({ async: true })
2. Spawn background job, return job_id
3. Job processes files in priority order
4. Status endpoint shows progress
5. Queries use latest indexed data
6. Job completes, status updated
```

## Docker-based Multi-Service Architecture

Codetriever uses a hybrid architecture with components split between host and Docker:

### Service Topology
```
Docker Compose Stack
├── codetriever-api (Rust HTTP Server)
│   ├── Tree-sitter parsing
│   ├── Vector embedding
│   ├── Search logic
│   └── Qdrant client
│
├── qdrant (Official Docker image)
│   └── Vector storage & search
│
└── Host Machine
    └── codetriever (MCP/CLI binary)
        ├── File watching (native FS access)
        ├── MCP server (stdio/SSE)
        ├── CLI commands
        └── HTTP client → API
```

### Component Separation Rationale

**Host Binary (MCP/CLI):**
- File watching requires native OS file system events
- Docker volumes for watching are slow and problematic
- MCP needs persistent connection to Claude Code
- Direct file system access for reading code

**Docker API Service:**
- Heavy compute operations (parsing, embedding)
- Stateless and horizontally scalable
- Clean HTTP interface for future SaaS
- Isolated from host file system

**Docker Qdrant Service:**
- Persistent vector storage
- Managed as standard Docker service
- Easy backup and migration
- Production-ready deployment

### File Watching Strategy

The host binary watches files and sends changes to the API:

```rust
// Host binary detects change
on_file_change(path) {
    let content = fs::read_to_string(path)?;
    
    // Send to API for processing
    api_client.post("/index", IndexRequest {
        path: path,
        content: content,
        operation: "update"
    });
}
```

This avoids Docker volume mounting issues while keeping the heavy processing containerized.

## Why This Architecture?

1. **Local-first**: Privacy, speed, no cloud costs
2. **Unified interface**: Same tools everywhere
3. **Always fresh**: Native file watching = auto-updates
4. **Non-blocking**: Async indexing = no wait
5. **Incremental**: Only process changes
6. **Observable**: Status shows what's happening
7. **Production-ready**: Docker deployment from day one
8. **Scalable**: Clean separation enables future SaaS