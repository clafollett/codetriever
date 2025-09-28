# Agenterra Code Generation Issues to Fix

## Overview
The following issues were found in generated MCP handler code that should be fixed in the Agenterra project's code generation templates.

## Issue 1: Unsafe JSON Serialization with .expect()

**Location**: All generated MCP handlers (`/src/handlers/*.rs`)
**Pattern**:
```rust
impl IntoContents for CleanResponse {
    fn into_contents(self) -> Vec<Content> {
        vec![Content::json(self).expect("Failed to serialize CleanResponse to Content")]
    }
}
```

**Problem**:
- Uses `.expect()` which can panic in production
- No graceful error handling for serialization failures

**Suggested Fix**:
```rust
impl IntoContents for CleanResponse {
    fn into_contents(self) -> Vec<Content> {
        match Content::json(self) {
            Ok(content) => vec![content],
            Err(e) => {
                tracing::error!("JSON serialization failed: {e}");
                vec![Content::Text(format!("Serialization error: {e}"))]
            }
        }
    }
}
```

**Files Affected**:
- clean.rs, compact.rs, find_similar.rs, find_usages.rs
- get_context.rs, get_stats.rs, get_status.rs, index.rs, search.rs

## Issue 2: Signal Handler .expect() Calls

**Location**: `src/signal.rs` (generated)
**Pattern**:
```rust
let mut sighup = signal(SignalKind::hangup()).expect("Failed to register SIGHUP");
let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM");
let mut sigint = signal(SignalKind::interrupt()).expect("Failed to register SIGINT");
```

**Problem**:
- Signal registration can fail on some systems
- Should handle gracefully rather than panic

**Suggested Fix**:
```rust
let mut sighup = match signal(SignalKind::hangup()) {
    Ok(s) => s,
    Err(e) => {
        error!("Failed to register SIGHUP handler: {e}");
        return;
    }
};
```

## Issue 3: Hardcoded Magic Numbers

**Location**: Generated handlers
**Pattern**: Comments mention specific dimensions/limits that should be configurable

**Suggested Fix**: Generate these from model configuration rather than hardcoding

## Impact
- **Security**: Prevents potential panics in production MCP handlers
- **Reliability**: Better error handling and user experience
- **Maintainability**: Cleaner generated code

## Priority
**Medium** - These are in MCP handlers used for local development, but should be fixed for production readiness.