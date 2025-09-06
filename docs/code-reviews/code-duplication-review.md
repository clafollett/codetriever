# Code Duplication Review & DRY Refactoring Suggestions

**Project**: Codetriever  
**Reviewer**: Code Review Agent  
**Date**: 2025-09-06  
**Review Type**: Comprehensive duplication analysis across all crates

## Executive Summary

Found several significant duplication patterns that would benefit from DRY refactoring. The most impactful opportunities are:

- **Database Repository Pattern** (High Impact): 90% similar logic between mock and real implementations
- **Auto-Generated Handler Boilerplate** (High Impact): Massive duplication across 9+ handlers  
- **Error Handling Patterns** (Medium Impact): Similar error types and conversions across crates
- **Test Setup Utilities** (Medium Impact): Repeated test initialization patterns
- **Configuration Structs** (Low Impact): Similar config patterns but context-specific

## Critical Duplication Findings 🚨

### 1. Database Repository Pattern Duplication

**Location**: `codetriever-data/src/repository.rs` vs `codetriever-data/src/mock.rs`  
**Severity**: HIGH - 90% logic duplication  
**Lines Affected**: ~700 lines total

#### Issues Found:
- Near-identical method implementations between `DbFileRepository` and `MockFileRepository`
- Same parameter validation, error handling, and return mapping
- Duplicated async trait implementations
- Identical test patterns for both implementations

#### Current Duplication Example:
```rust
// DbFileRepository::check_file_state (lines 59-100)
async fn check_file_state(&self, repo_id: &str, branch: &str, path: &str, hash: &str) -> Result<FileState> {
    let existing = sqlx::query("SELECT content_hash, generation FROM indexed_files WHERE repository_id = $1 AND branch = $2 AND file_path = $3")
        .bind(repo_id).bind(branch).bind(path).fetch_optional(&self.pool).await.context("Failed to check file state")?;
    
    match existing {
        None => Ok(FileState::New { generation: 1 }),
        Some(row) => {
            let existing_hash: String = row.get("content_hash");
            if existing_hash == hash { Ok(FileState::Unchanged) }
            else { Ok(FileState::Updated { old_generation: row.get("generation"), new_generation: row.get("generation") + 1 }) }
        }
    }
}

// MockFileRepository::check_file_state (lines 90-114)  
async fn check_file_state(&self, repo_id: &str, branch: &str, path: &str, hash: &str) -> Result<FileState> {
    self.check_fail()?;
    let key = (repo_id.to_string(), branch.to_string(), path.to_string());
    let files = self.indexed_files.lock().unwrap();
    
    match files.get(&key) {
        None => Ok(FileState::New { generation: 1 }),
        Some(file) if file.content_hash == hash => Ok(FileState::Unchanged),
        Some(file) => Ok(FileState::Updated { old_generation: file.generation, new_generation: file.generation + 1 }),
    }
}
```

#### DRY Refactoring Approach:
**Strategy**: Introduce a trait-based Repository Pattern with shared business logic

```rust
// New approach - shared business logic
pub trait FileRepositoryBackend: Send + Sync {
    async fn query_file(&self, repo_id: &str, branch: &str, path: &str) -> Result<Option<FileRecord>>;
    async fn store_file(&self, file: &FileRecord) -> Result<()>;
    // ... other storage primitives
}

pub struct FileRepository<B: FileRepositoryBackend> {
    backend: B,
}

impl<B: FileRepositoryBackend> FileRepository<B> {
    // Shared business logic - eliminate 90% of duplication
    async fn check_file_state(&self, repo_id: &str, branch: &str, path: &str, hash: &str) -> Result<FileState> {
        match self.backend.query_file(repo_id, branch, path).await? {
            None => Ok(FileState::New { generation: 1 }),
            Some(file) if file.content_hash == hash => Ok(FileState::Unchanged),
            Some(file) => Ok(FileState::Updated {
                old_generation: file.generation,
                new_generation: file.generation + 1,
            }),
        }
    }
}

// Concrete backends only implement storage primitives
pub struct DbBackend { pool: PgPool }
pub struct MockBackend { data: Arc<Mutex<MockData>> }
```

**Impact**: Reduces 700 lines to ~200 lines, eliminates logic drift between implementations

---

### 2. Auto-Generated Handler Boilerplate

**Location**: `codetriever/src/handlers/` (9 handlers)  
**Severity**: HIGH - 85% identical boilerplate  
**Lines Affected**: ~1,200 lines total

#### Issues Found:
- Nearly identical handler function structures across all endpoints
- Duplicated import blocks (15+ identical imports per file)
- Copy-paste logging patterns with slight variations
- Identical test patterns for every handler
- Same derive macros and trait implementations

#### Current Duplication Example:
```rust
// EVERY handler has this exact pattern:
pub async fn {endpoint}_handler(config: &Config, params: &{Endpoint}Params) -> Result<CallToolResult, agenterra_rmcp::Error> {
    info!(target = "handler", event = "incoming_request", endpoint = "{endpoint}", 
          method = "GET", path = "/{endpoint}", params = serde_json::to_string(params)...);
    debug!(target = "handler", event = "before_api_call", endpoint = "{endpoint}");
    let resp = get_endpoint_response::<_, {Endpoint}Response>(config, params).await;
    match &resp {
        Ok(r) => info!(target = "handler", event = "api_response", endpoint = "{endpoint}", response = ?r),
        Err(e) => error!(target = "handler", event = "api_error", endpoint = "{endpoint}", error = ?e),
    }
    resp.and_then(|r| r.into_call_tool_result())
}

// Plus identical test modules in every file
#[cfg(test)]
mod tests {
    #[test] fn test_parameters_struct_serialization() { /* identical */ }
    #[test] fn test_properties_struct_serialization() { /* identical */ }
}
```

#### DRY Refactoring Approach:
**Strategy**: Generic handler macro + trait-based endpoint definitions

```rust
// Eliminate 1000+ lines with a single macro
macro_rules! generate_handler {
    ($endpoint:ident, $params:ty, $response:ty, $path:expr, $description:expr) => {
        pub async fn paste!([<$endpoint _handler>])(
            config: &Config,
            params: &$params,
        ) -> Result<CallToolResult, agenterra_rmcp::Error> {
            handle_endpoint::<$params, $response>(config, params, stringify!($endpoint), $path).await
        }
    };
}

// Single generic handler function
async fn handle_endpoint<P, R>(config: &Config, params: &P, endpoint: &str, path: &str) -> Result<CallToolResult, agenterra_rmcp::Error>
where
    P: Serialize + fmt::Debug,
    R: DeserializeOwned + IntoContents,
{
    log_incoming_request(endpoint, path, params);
    let resp = get_endpoint_response::<P, R>(config, params).await;
    log_response(endpoint, &resp);
    resp.and_then(|r| r.into_call_tool_result())
}

// Usage - 1 line per handler instead of 130 lines
generate_handler!(search, SearchParams, SearchResponse, "/search", "Search code by meaning");
generate_handler!(index, IndexParams, IndexResponse, "/index", "Refresh the code index");
// ... 7 more handlers
```

**Impact**: Reduces 1,200 lines to ~50 lines, ensures consistent logging and error handling

---

### 3. Error Handling Pattern Duplication

**Location**: `codetriever-api/src/error.rs` vs `codetriever-indexer/src/error.rs`  
**Severity**: MEDIUM - 60% overlap  
**Lines Affected**: ~150 lines total

#### Issues Found:
- Similar error variants across crates (Io, Configuration, Storage, etc.)
- Duplicated `From` trait implementations
- Similar but not identical error handling patterns

#### Current Duplication Example:
```rust
// codetriever-api/src/error.rs
#[error("IO error: {0}")]           Io(String),
#[error("Configuration error: {0}")] Configuration(String),
#[error("Not found: {0}")]          NotFound(String),

// codetriever-indexer/src/error.rs  
#[error("IO error: {0}")]           Io(String),
#[error("Configuration error: {0}")] Configuration(String),
#[error("Other error: {0}")]        Other(String),  // Similar to NotFound

// Plus identical From impls everywhere
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self { Error::Io(e.to_string()) }
}
```

#### DRY Refactoring Approach:
**Strategy**: Shared error crate with common patterns

```rust
// New codetriever-core crate
pub trait ErrorVariants {
    fn io_error(msg: String) -> Self;
    fn config_error(msg: String) -> Self;
    fn not_found(msg: String) -> Self;
}

// Macro for consistent error types
macro_rules! define_error_enum {
    ($name:ident, { $($variant:ident($type:ty) => $msg:expr),* }) => {
        #[derive(Debug, thiserror::Error)]
        pub enum $name {
            $(#[error($msg)] $variant($type),)*
            #[error(transparent)] Anyhow(#[from] anyhow::Error),
        }
        
        impl ErrorVariants for $name {
            fn io_error(msg: String) -> Self { Self::Io(msg) }
            fn config_error(msg: String) -> Self { Self::Configuration(msg) }
            fn not_found(msg: String) -> Self { Self::NotFound(msg) }
        }
    };
}

// Usage
define_error_enum!(ApiError, {
    Io(String) => "IO error: {0}",
    Configuration(String) => "Configuration error: {0}",
    NotFound(String) => "Not found: {0}",
    Parser(String) => "Parser error: {0}"
});

define_error_enum!(IndexerError, {
    Io(String) => "IO error: {0}",
    Configuration(String) => "Configuration error: {0}",
    Storage(String) => "Storage error: {0}",
    Embedding(String) => "Embedding error: {0}"
});
```

**Impact**: Reduces error boilerplate by ~50 lines, ensures consistent error patterns

---

### 4. Test Setup Utility Duplication

**Location**: Multiple test files across `codetriever-indexer/tests/`  
**Severity**: MEDIUM - Repeated test setup patterns  
**Lines Affected**: ~200 lines scattered across tests

#### Issues Found:
- Similar test environment setup in multiple files
- Repeated HuggingFace token checking
- Duplicated storage creation patterns
- Copy-paste test data generation

#### Current Duplication Example:
```rust
// In content_indexing_tests.rs (lines 17-28)
let config = test_config();
let storage = create_test_storage("content_indexing").await.expect("Failed to create storage");
let mut indexer = Indexer::with_config_and_storage(&config, storage);

// In qdrant_integration.rs (similar pattern)
let storage = create_test_storage("qdrant_chunks").await.expect("Failed to create storage");

// Repeated HF token check in every test
if skip_without_hf_token().is_none() { return; }
```

#### DRY Refactoring Approach:
**Strategy**: Test fixture builder pattern

```rust
// Enhanced test_utils.rs
pub struct TestFixture {
    storage: QdrantStorage,
    indexer: Indexer,
    config: Config,
}

impl TestFixture {
    pub async fn new(test_name: &str) -> Result<Self, String> {
        skip_without_hf_token().ok_or("Missing HF token")?;
        let config = test_config();
        let storage = create_test_storage(test_name).await?;
        let indexer = Indexer::with_config_and_storage(&config, storage.clone());
        Ok(Self { storage, indexer, config })
    }
    
    pub fn with_test_files(mut self, files: Vec<(&str, &str)>) -> Self {
        // Pre-populate with common test data
        self
    }
}

// Usage - eliminates 10+ lines per test
#[tokio::test]
async fn test_index_content() {
    let fixture = TestFixture::new("content_test").await.expect("Setup failed");
    // Test logic only...
}
```

**Impact**: Reduces test setup from 10+ lines to 2 lines per test, ensures consistency

---

## Minor Duplication Patterns

### 5. Configuration Struct Similarities

**Locations**: Multiple `Config` structs across crates  
**Severity**: LOW - Context-specific but similar patterns  
**Assessment**: These are likely appropriate given different domain contexts

### 6. Repeated Context Patterns

**Location**: Throughout `repository.rs`  
**Severity**: LOW - Standard Rust error handling  
**Pattern**: `.context("Failed to ...")` everywhere  
**Assessment**: This is idiomatic Rust - consider keeping as-is

## Refactoring Priority & Impact Assessment

### High Priority (Immediate Action Recommended) 🔥

1. **Handler Boilerplate** - Saves ~1,000 lines, improves maintainability
2. **Repository Pattern** - Saves ~500 lines, eliminates logic drift risk

### Medium Priority (Next Sprint) 🎯

3. **Error Handling** - Saves ~100 lines, improves consistency  
4. **Test Utilities** - Saves ~150 lines, improves test reliability

### Low Priority (Consider Later) 💭

5. **Configuration Patterns** - Domain-specific, may be appropriate as-is

## Implementation Recommendations

### Phase 1: Handler Refactoring (High Impact, Low Risk)
- Create generic handler macro
- Migrate one handler at a time
- Maintain backward compatibility during transition

### Phase 2: Repository Pattern (High Impact, Medium Risk)  
- Extract shared business logic into trait
- Implement backend pattern for storage abstraction
- Add comprehensive tests for new pattern

### Phase 3: Error & Test Utilities (Medium Impact, Low Risk)
- Create shared error patterns
- Enhance test utilities with fixture pattern
- Gradual migration across crates

## Code Quality Metrics

### Before Refactoring:
- **Total Lines**: ~2,250 lines with duplication
- **Maintenance Burden**: HIGH (changes require updates in multiple places)
- **Bug Risk**: HIGH (logic can drift between mock/real implementations)

### After Refactoring (Estimated):
- **Total Lines**: ~1,100 lines (51% reduction)
- **Maintenance Burden**: LOW (single source of truth)
- **Bug Risk**: LOW (shared logic, comprehensive tests)

## Conclusion

The codetriever codebase shows classic symptoms of rapid development - functional duplication that emerged organically. The refactoring opportunities identified here would significantly improve maintainability while reducing the risk of bugs from logic drift between implementations.

The handler boilerplate and repository pattern duplication are the most impactful to address, offering substantial line reduction and improved consistency. The error handling and test utilities represent good secondary targets for cleanup.

All refactoring should be done incrementally with comprehensive test coverage to ensure no regression in functionality. 🚀

---

**Next Steps**: 
1. Prioritize handler macro implementation (quick win)
2. Design repository trait pattern (architecture review)
3. Create migration plan with rollback strategy
4. Implement comprehensive test suite for new patterns

*Remember: The best code is code you only have to write once! 💯*