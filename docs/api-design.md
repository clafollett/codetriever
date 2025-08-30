# Codetriever API Design

**Every CLI command is an MCP tool. Async for MCP, sync for CLI.**

## Design Principles

1. **Tool/CLI Parity**: Every operation available in both interfaces
2. **Async by Default (MCP)**: Never block Claude Code
3. **Sync by Default (CLI)**: Developers expect to wait
4. **Observable Operations**: Always know what's happening
5. **Smart Defaults**: Works without configuration

## Endpoints / Tools

### Search Operations

#### `search` - Semantic Code Search
```typescript
// Request
{
  query: string,          // Natural language query
  limit?: number,         // Max results (default: 10)
  threshold?: number      // Min similarity (default: 0.7)
}

// Response
{
  chunks: [{
    file: string,         // "src/auth.rs"
    line_start: number,   // 45
    line_end: number,     // 67
    content: string,      // Actual code
    similarity: number,   // 0.95
    symbols: string[]     // ["authenticate", "Token"]
  }],
  query_time_ms: number
}
```

#### `similar` - Find Similar Code
```typescript
// Request
{
  code: string,           // Code snippet to match
  limit?: number,         // Max results (default: 10)
  exclude_file?: string   // Don't include this file
}

// Response (same as search)
```

#### `context` - Get Surrounding Code
```typescript
// Request
{
  file: string,           // File path
  line: number,           // Center line
  radius?: number         // Lines before/after (default: 20)
}

// Response
{
  file: string,
  content: string,        // Full context
  line_start: number,
  line_end: number,
  symbols: string[]       // Symbols in context
}
```

#### `usages` - Find Symbol Usages
```typescript
// Request
{
  symbol: string,         // Function/class/variable name
  type?: "all" | "definitions" | "references"
}

// Response
{
  usages: [{
    file: string,
    line: number,
    type: "definition" | "reference",
    content: string       // Line content
  }]
}
```

### Index Operations

#### `index` - Build/Rebuild Index
```typescript
// Request
{
  mode?: "full" | "incremental",  // Default: incremental
  async?: boolean,                // MCP: true, CLI: false
  paths?: string[],               // Specific paths
  timeout_ms?: number             // Max wait time (MCP only)
}

// Response (async)
{
  job_id: string,
  status: "queued" | "started",
  message: string
}

// Response (sync)
{
  files_processed: number,
  chunks_created: number,
  duration_ms: number,
  errors: string[]
}
```

### Status Operations

#### `status` - Server and Index Status
```typescript
// Request
{} // No parameters

// Response
{
  server: {
    version: string,
    uptime_seconds: number,
    pid: number
  },
  index: {
    jobs: [{
      id: string,
      type: "full" | "incremental",
      status: "queued" | "processing" | "completed" | "failed",
      progress: {
        current: number,
        total: number,
        percent: number,
        eta_seconds: number
      },
      started: string,      // ISO timestamp
      completed?: string
    }],
    stats: {
      total_files: number,
      indexed_files: number,
      stale_files: number,
      total_chunks: number,
      index_size_mb: number,
      last_update: string
    }
  },
  watcher: {
    enabled: boolean,
    watching_paths: string[],
    events_last_minute: number,
    queue_size: number
  },
  performance: {
    avg_search_ms: number,
    avg_index_ms_per_file: number,
    memory_usage_mb: number
  }
}
```

#### `stats` - Quick Statistics
```typescript
// Request
{} // No parameters

// Response
{
  files: number,
  chunks: number,
  vectors: number,
  db_size_mb: number,
  last_indexed: string
}
```

### Management Operations

#### `clean` - Remove Stale Entries
```typescript
// Request
{
  older_than?: string,    // "7d", "1h", etc.
  missing_files?: boolean // Remove entries for deleted files
}

// Response
{
  removed_chunks: number,
  freed_space_mb: number
}
```

#### `compact` - Optimize Database
```typescript
// Request
{} // No parameters

// Response
{
  before_size_mb: number,
  after_size_mb: number,
  duration_ms: number
}
```

#### `config` - Get/Set Configuration
```typescript
// Request (GET)
{
  key?: string  // Specific key or all
}

// Request (SET)
{
  key: string,
  value: any
}

// Response
{
  config: {
    chunk_size: number,
    debounce_ms: number,
    ignore_patterns: string[],
    // etc...
  }
}
```

## Async Job Management

### Job States
```
QUEUED → PROCESSING → COMPLETED
                   ↘ FAILED
```

### Job Priority Queue
```typescript
enum Priority {
  HIGH = 0,    // User-initiated
  NORMAL = 1,  // File watcher
  LOW = 2,     // Background cleanup
  IDLE = 3     // Optimization
}
```

### Example: Async Index Flow
```typescript
// 1. Start index
const job = await tools.index({ async: true, paths: ["src/"] });
// Returns immediately: { job_id: "abc-123", status: "queued" }

// 2. Check progress
const status = await tools.status();
// Shows: jobs[0].progress.percent = 45

// 3. Job completes
const status2 = await tools.status();
// Shows: jobs[0].status = "completed"
```

## Error Handling

All endpoints return consistent errors:

```typescript
// Error Response
{
  error: {
    code: "INDEX_IN_PROGRESS" | "INVALID_QUERY" | "TIMEOUT" | etc,
    message: string,
    details?: any
  }
}
```

## Rate Limiting

- **Search operations**: Unlimited (fast, read-only)
- **Index operations**: One active job at a time
- **Status operations**: Unlimited
- **Management operations**: Queued behind index jobs

## CLI vs MCP Behavior

| Operation | CLI | MCP |
|-----------|-----|-----|
| search | Prints to stdout | Returns JSON |
| index | Blocks with progress bar | Returns job_id |
| status | Pretty prints table | Returns JSON |
| clean | Asks for confirmation | No confirmation |

## Authentication

None required - local-only by design.

## Future Extensions

- **explain**: Explain what code does
- **refactor**: Suggest improvements
- **dependencies**: Show dependency graph
- **diff**: Compare implementations
- **metrics**: Code complexity scores