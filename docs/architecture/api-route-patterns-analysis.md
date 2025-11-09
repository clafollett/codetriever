# API Route Handler Patterns Analysis

**Date**: 2025-10-25  
**Status**: Current State Documentation  
**Purpose**: Analyze inconsistencies in route handler patterns to establish unified architecture  

---

## Executive Summary

### The Problem

Codetriever API currently has **5 HTTP endpoints implemented with 5 completely different architectural patterns**. Each endpoint handles routing, dependency injection, error handling, and data access differently, creating:

- **Maintenance burden**: New developers must learn 5 different patterns
- **Testing complexity**: Each endpoint requires different mocking strategies
- **Code duplication**: Common concerns (correlation IDs, logging, validation) reimplemented differently
- **Architectural drift**: No clear pattern to follow for future endpoints

### Impact

| Concern | Current State | Desired State |
|---------|---------------|---------------|
| **Consistency** | 5 different patterns | 1 unified pattern |
| **Testability** | Mixed (some mockable, some not) | All endpoints easily testable |
| **Onboarding** | High cognitive load | Clear, documented standard |
| **Maintainability** | Scattered logic | Centralized, reusable |

---

## Detailed Pattern Analysis

### 1. Health Endpoint (`/health`)

**File**: `routes/health.rs`

#### A. Route Registration Pattern
```rust
pub fn routes() -> Router {
    Router::new().route("/health", get(health_check))
}
```
- **No dependencies**: Stateless function
- **No DI**: Self-contained
- **Simplest possible pattern**

#### B. Handler Function Pattern
```rust
async fn health_check() -> Json<serde_json::Value> {
    Json(json!({
        "status": "healthy",
        "service": "codetriever-api"
    }))
}
```
- **No State extraction**
- **Direct return**: `Json<Value>`
- **No error handling**: Cannot fail
- **No middleware**: No correlation ID, no tracing

#### C. Request/Response Handling
- **No request body**
- **Static response**: Always 200 OK
- **No validation**: Nothing to validate

#### D. Data Access Pattern
- **None**: No database/service access
- **Testability**: Trivial (no deps)

#### E. Observability Pattern
- **No logging**
- **No correlation ID**
- **No tracing instrumentation**
- **No metrics**

#### F. OpenAPI Documentation
- **Missing**: No `#[utoipa::path]` annotation
- **Not in OpenAPI spec**

#### Summary
| Aspect | Pattern |
|--------|---------|
| DI | None |
| Return Type | `Json<Value>` |
| Error Handling | None |
| State | None |
| Correlation ID | ❌ |
| Tracing | ❌ |
| OpenAPI | ❌ |

---

### 2. Search Endpoint (`POST /search`)

**File**: `routes/search.rs`

#### A. Route Registration Pattern
```rust
// Variant 1: Lazy initialization (default)
pub fn routes() -> Router {
    let search_wrapper = Arc::new(tokio::sync::Mutex::new(LazySearchService::new()));
    routes_with_lazy_search(search_wrapper)
}

// Variant 2: Dependency injection (clean)
pub fn routes_with_search_service(search_service: Arc<dyn SearchProvider>) -> Router {
    Router::new()
        .route("/search", post(search_handler))
        .with_state(search_service)
}

// Variant 3: Internal use
pub fn routes_with_lazy_search(
    search_service: Arc<tokio::sync::Mutex<LazySearchService>>,
) -> Router {
    Router::new()
        .route("/search", post(lazy_search_handler))
        .with_state(search_service)
}
```
- **3 different registration functions**
- **Trait object DI**: `Arc<dyn SearchProvider>`
- **Supports lazy initialization** for convenience
- **Proper abstraction** through traits

#### B. Handler Function Pattern
```rust
#[utoipa::path(
    post,
    path = "/search",
    tag = "search",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Search results", body = SearchResponse),
        (status = 500, description = "Internal server error")
    )
)]
#[instrument(skip(search_service), fields(correlation_id))]
pub async fn search_handler(
    State(search_service): State<SearchServiceHandle>,
    context: Option<Extension<RequestContext>>,
    Json(req): Json<SearchRequest>,
) -> ApiResult<Json<SearchResponse>> {
    search_handler_impl(search_service, context, req).await
}
```
- **State extraction**: `Arc<dyn SearchProvider>`
- **Middleware integration**: `Option<Extension<RequestContext>>`
- **JSON deserialization**: `Json<SearchRequest>`
- **Return type**: `ApiResult<Json<SearchResponse>>` (custom error type)
- **Tracing**: `#[instrument]` macro
- **OpenAPI**: `#[utoipa::path]` macro
- **Delegation pattern**: Calls `search_handler_impl()` for testability

#### C. Request/Response Handling
```rust
// Correlation ID handling
let correlation_id = context
    .as_ref()
    .map_or_else(CorrelationId::new, |ctx| ctx.correlation_id.clone());

tracing::Span::current().record("correlation_id", correlation_id.to_string());

// Request validation
if req.query.trim().is_empty() {
    return Err(ApiError::invalid_query(
        req.query,
        "Query cannot be empty".to_string(),
        correlation_id,
    ));
}

// Timeout handling
match tokio::time::timeout(Duration::from_secs(30), search_service.search(...)).await {
    Ok(Ok(results)) => { /* success */ },
    Ok(Err(search_error)) => { /* service error */ },
    Err(_timeout) => { /* timeout */ },
}

// Structured response
Ok(Json(SearchResponse {
    matches,
    metadata: SearchMetadata { /* ... */ },
}))
```
- **Explicit correlation ID extraction**
- **Comprehensive validation**
- **Timeout protection**
- **Structured errors** via `ApiError`
- **Rich logging** at each step

#### D. Data Access Pattern
- **Service dependency**: `Arc<dyn SearchProvider>` trait
- **Indirect database access**: Search service uses DataClient internally
- **Trait allows mocking**: `MockSearchService` for tests
- **Clean separation**: Handler doesn't know about DB implementation

#### E. Observability Pattern
- **Full instrumentation**: `#[instrument]` macro
- **Correlation ID**: Extracted, logged, included in errors
- **Timing**: Manual `Instant::now()` for query timing
- **Structured logging**: `info!`, `warn!`, `error!` with context
- **Span recording**: Correlation ID added to tracing span

#### F. OpenAPI Documentation
- **Complete**: `#[utoipa::path]` with full details
- **Request/Response schemas**: All types annotated with `#[derive(ToSchema)]`
- **Status codes**: Documented (200, 500)
- **Tags**: Organized under "search"

#### Summary
| Aspect | Pattern |
|--------|---------|
| DI | `Arc<dyn SearchProvider>` (trait) |
| Return Type | `ApiResult<Json<T>>` |
| Error Handling | `ApiError` enum (structured) |
| State | Trait object |
| Correlation ID | ✅ (explicit extraction) |
| Tracing | ✅ (`#[instrument]`) |
| OpenAPI | ✅ (complete) |

---

### 3. Index Endpoint (`POST /index`)

**File**: `routes/index.rs`

#### A. Route Registration Pattern
```rust
type IndexerServiceHandle = Arc<Mutex<dyn IndexerService>>;

pub fn routes_with_indexer(indexer: IndexerServiceHandle) -> Router {
    Router::new()
        .route("/index", post(index_handler))
        .with_state(indexer)
}
```
- **Single registration function**
- **Type alias**: `IndexerServiceHandle` = `Arc<Mutex<dyn IndexerService>>`
- **Trait object with Mutex**: Requires mutable access
- **Clear naming**: `routes_with_*` pattern

#### B. Handler Function Pattern
```rust
#[utoipa::path(
    post,
    path = "/index",
    tag = "index",
    request_body = IndexRequest,
    responses(
        (status = 200, description = "Files indexed successfully", body = IndexResponse),
        (status = 500, description = "Internal server error", body = IndexResponse)
    )
)]
pub async fn index_handler(
    State(indexer): State<IndexerServiceHandle>,
    Json(request): Json<IndexRequest>,
) -> impl IntoResponse {
    // ... implementation
}
```
- **State extraction**: `Arc<Mutex<dyn IndexerService>>`
- **NO correlation ID**: Missing `RequestContext`
- **JSON deserialization**: `Json<IndexRequest>`
- **Return type**: `impl IntoResponse` (NOT `ApiResult`)
- **NO tracing macro**: No `#[instrument]`
- **OpenAPI**: `#[utoipa::path]` present

#### C. Request/Response Handling
```rust
// Lock the mutex
let mut indexer = indexer.lock().await;

// Call service
match indexer.index_file_content(&request.project_id, files).await {
    Ok(result) => {
        (StatusCode::OK, Json(IndexResponse::success(...))).into_response()
    }
    Err(e) => {
        tracing::error!("Indexing failed: {}", e);

        // Manual error categorization
        let is_infrastructure_error = error_msg.contains("Pool closed")
            || error_msg.contains("database") || /* ... */;

        if is_infrastructure_error {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(IndexResponse::error()))
        } else {
            (StatusCode::OK, Json(IndexResponse::error()))  // Business error as 200!
        }
    }
}
```
- **NO correlation ID handling**
- **Manual error categorization**: String matching on error messages
- **Inconsistent status codes**: Business errors return 200 OK (wrong!)
- **Manual response construction**: Tuple `(StatusCode, Json<T>)`
- **NO timeout protection**
- **Limited validation**

#### D. Data Access Pattern
- **Service dependency**: `Arc<Mutex<dyn IndexerService>>` trait
- **Indirect database access**: Indexer uses Repository internally
- **Mutex required**: Service needs mutable access
- **Mockable**: `MockIndexerService` exists

#### E. Observability Pattern
- **NO instrumentation**: Missing `#[instrument]`
- **NO correlation ID**: Can't track requests
- **Manual logging**: Only `tracing::error!` on failure
- **NO timing metrics**
- **NO span recording**

#### F. OpenAPI Documentation
- **Present**: `#[utoipa::path]`
- **Schemas**: `#[derive(ToSchema)]` on types
- **Status codes**: 200 and 500 (but implementation is wrong!)

#### Summary
| Aspect | Pattern |
|--------|---------|
| DI | `Arc<Mutex<dyn IndexerService>>` (trait) |
| Return Type | `impl IntoResponse` |
| Error Handling | Manual StatusCode construction |
| State | Trait object (mutable) |
| Correlation ID | ❌ |
| Tracing | ❌ (manual error logging only) |
| OpenAPI | ✅ |

**Issues**:
- Business errors returned as 200 OK
- No correlation ID tracking
- No request instrumentation
- Inconsistent with search endpoint pattern

---

### 4. Status Endpoint (`GET /api/status`)

**File**: `routes/status.rs`

#### A. Route Registration Pattern
```rust
pub fn routes(state: AppState) -> axum::Router {
    use axum::routing::get;
    axum::Router::new()
        .route("/api/status", get(status_handler))
        .with_state(state)
}
```
- **Takes entire AppState**: NOT just its dependencies
- **Different from other endpoints**: Only one that takes full state
- **State cloning**: AppState must be Clone

#### B. Handler Function Pattern
```rust
// NO #[utoipa::path] - excluded due to "impl Trait limitation"
pub async fn status_handler(State(state): State<AppState>) -> Json<StatusResponse> {
    let postgres_status = check_postgres_health_client(&state.db_client).await;
    let (total_files, total_chunks, db_size_mb, last_indexed_at) =
        get_index_stats_client(&state.db_client).await;
    let qdrant_status = check_qdrant_health(state.vector_storage.as_ref()).await;
    // ...
    Json(StatusResponse { /* ... */ })
}
```
- **State extraction**: Entire `AppState`
- **NO correlation ID**: Missing `RequestContext`
- **NO request body**: GET request
- **Return type**: `Json<StatusResponse>` (direct, no ApiResult)
- **NO tracing macro**
- **NO OpenAPI annotation**: Comment says "impl Trait limitation"

#### C. Request/Response Handling
```rust
// Helper functions for data access
async fn check_postgres_health_client(client: &DataClient) -> String {
    match client.count_project_branches().await {
        Ok(_) => "connected".to_string(),
        Err(_) => "disconnected".to_string(),
    }
}

async fn get_index_stats_client(client: &DataClient) -> (i64, i64, f64, Option<String>) {
    let files = client.count_indexed_files().await.unwrap_or(0);
    // ... more unwrap_or calls
    (files, chunks, db_size_mb, last_indexed_at)
}
```
- **NO validation**: Nothing to validate (GET)
- **Error swallowing**: All errors become "disconnected" or 0
- **Graceful degradation**: Never fails, returns partial data
- **NO explicit correlation ID**
- **Static response structure**

#### D. Data Access Pattern
- **Direct concrete type**: `Arc<DataClient>` (from AppState)
- **NO trait abstraction**: Can't mock easily
- **Multiple service access**: DB client AND vector storage
- **Requires full AppState** to access multiple services

#### E. Observability Pattern
- **NO instrumentation**
- **NO correlation ID**
- **NO logging**: Silent success/failure
- **NO timing**
- **Uptime tracking**: Uses `LazyLock<SystemTime>`

#### F. OpenAPI Documentation
- **Missing from paths**: Comment says excluded
- **Schemas documented**: `#[derive(ToSchema)]` on response types
- **Example provided**: `#[schema(example = json!(...))]`
- **NOT in OpenAPI routes**: Can't be tested via Swagger UI

#### Summary
| Aspect | Pattern |
|--------|---------|
| DI | Full `AppState` |
| Return Type | `Json<T>` (direct) |
| Error Handling | Silent swallowing (unwrap_or) |
| State | Entire AppState |
| Correlation ID | ❌ |
| Tracing | ❌ |
| OpenAPI | ⚠️ (schemas only, no path) |

**Issues**:
- Takes entire AppState instead of minimal dependencies
- No correlation ID tracking
- Silently swallows all errors
- Missing from OpenAPI paths
- Inconsistent with other endpoints

---

### 5. Context Endpoint (`POST /api/context`)

**File**: `routes/context.rs`

#### A. Route Registration Pattern
```rust
pub fn routes(db_client: Arc<DataClient>) -> Router {
    Router::new()
        .route("/api/context", post(context_handler))
        .with_state(db_client)
}
```
- **Concrete type injection**: `Arc<DataClient>` (NOT a trait!)
- **Broken pattern**: After removing `DatabaseClient` trait
- **Can't mock**: No trait abstraction
- **Different from search/index**: They use traits

#### B. Handler Function Pattern
```rust
#[utoipa::path(
    post,
    path = "/context",
    tag = "context",
    request_body = ContextRequest,
    responses(
        (status = 200, description = "Context retrieved successfully", body = ContextResponse),
        (status = 404, description = "File not found"),
        (status = 500, description = "Internal server error")
    )
)]
#[instrument(skip(db_client), fields(correlation_id))]
pub async fn context_handler(
    State(db_client): State<Arc<DataClient>>,
    context: Option<Extension<RequestContext>>,
    Json(req): Json<ContextRequest>,
) -> ApiResult<Json<ContextResponse>> {
    // ... implementation
}
```
- **State extraction**: `Arc<DataClient>` (concrete type)
- **Middleware integration**: `Option<Extension<RequestContext>>` ✅
- **JSON deserialization**: `Json<ContextRequest>` ✅
- **Return type**: `ApiResult<Json<ContextResponse>>` ✅
- **Tracing**: `#[instrument]` ✅
- **OpenAPI**: `#[utoipa::path]` ✅
- **Follows search pattern**: Except for concrete type DI

#### C. Request/Response Handling
```rust
// Correlation ID handling (same as search)
let correlation_id = context
    .as_ref()
    .map_or_else(CorrelationId::new, |ctx| ctx.correlation_id.clone());

tracing::Span::current().record("correlation_id", correlation_id.to_string());

// Validation
if req.file_path.trim().is_empty() {
    return Err(ApiError::invalid_query(...));
}

// Database access with error handling
let (repository_id, branch, file_content) = match db_client
    .get_file_content(req.repository_id.as_deref(), req.branch.as_deref(), &req.file_path)
    .await
{
    Ok(Some((repo_id, br, content))) => (repo_id, br, content),
    Ok(None) => return Err(ApiError::invalid_query(...)),  // File not found
    Err(e) => return Err(ApiError::InternalServerError { correlation_id }),
};

// Structured response
Ok(Json(ContextResponse { /* ... */ }))
```
- **Same correlation ID pattern as search** ✅
- **Validation present** ✅
- **Structured errors via ApiError** ✅
- **NO timeout protection** (unlike search)
- **Clean error handling**

#### D. Data Access Pattern
- **Concrete type**: `Arc<DataClient>` ⚠️
- **NO trait abstraction**: Can't inject mocks
- **Direct database calls**: No service layer
- **Broken after refactor**: Tests disabled (can't mock)

#### E. Observability Pattern
- **Full instrumentation**: `#[instrument]` ✅
- **Correlation ID**: Extracted and logged ✅
- **Timing**: Manual `Instant::now()` ✅
- **Structured logging**: `info!`, `warn!`, `error!` ✅
- **Span recording**: Correlation ID added ✅

#### F. OpenAPI Documentation
- **Complete**: `#[utoipa::path]` ✅
- **Request/Response schemas**: All types annotated ✅
- **Status codes**: 200, 404, 500 documented ✅
- **Tags**: Organized under "context" ✅

#### Summary
| Aspect | Pattern |
|--------|---------|
| DI | `Arc<DataClient>` (concrete type) ⚠️ |
| Return Type | `ApiResult<Json<T>>` ✅ |
| Error Handling | `ApiError` enum (structured) ✅ |
| State | Concrete type |
| Correlation ID | ✅ |
| Tracing | ✅ |
| OpenAPI | ✅ |

**Issues**:
- Uses concrete `DataClient` instead of trait
- Tests are broken/disabled (can't mock)
- Inconsistent with search/index (which use traits)
- Otherwise follows good patterns from search

---

## Cross-Cutting Concerns Matrix

| Endpoint | DI Pattern | Return Type | Error Handling | Correlation ID | Tracing | OpenAPI | Testability |
|----------|-----------|-------------|----------------|----------------|---------|---------|-------------|
| **health** | None | `Json<Value>` | None | ❌ | ❌ | ❌ | Easy (stateless) |
| **search** | `Arc<dyn SearchProvider>` | `ApiResult<Json<T>>` | `ApiError` enum | ✅ | ✅ | ✅ | Easy (trait mock) |
| **index** | `Arc<Mutex<dyn IndexerService>>` | `impl IntoResponse` | Manual StatusCode | ❌ | ❌ | ✅ | Easy (trait mock) |
| **status** | Full `AppState` | `Json<T>` | Silent swallowing | ❌ | ❌ | ⚠️ | Hard (needs AppState) |
| **context** | `Arc<DataClient>` (concrete) | `ApiResult<Json<T>>` | `ApiError` enum | ✅ | ✅ | ✅ | **Broken** (no mock) |

**Legend**:
- ✅ = Implemented correctly
- ❌ = Missing
- ⚠️ = Partial/incomplete

---

## Data Layer Architecture

### Current Structure

```
┌─────────────────────────────────────────┐
│         API Layer (routes)              │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐ │
│  │ search  │  │  index  │  │ context │ │
│  └────┬────┘  └────┬────┘  └────┬────┘ │
└───────┼────────────┼────────────┼───────┘
        │            │            │
        ▼            ▼            ▼
┌─────────────────────────────────────────┐
│      Service Layer (traits)             │
│  ┌──────────────┐  ┌──────────────┐    │
│  │SearchProvider│  │IndexerService│    │
│  │   (trait)    │  │   (trait)    │    │
│  └──────┬───────┘  └──────┬───────┘    │
└─────────┼──────────────────┼─────────────┘
          │                  │
          ▼                  ▼
┌─────────────────────────────────────────┐
│         Data Client Layer               │
│           ┌──────────────┐              │
│           │  DataClient  │              │ <-- Concrete type
│           │   (facade)   │              │     (status, context use directly)
│           └──────┬───────┘              │
└──────────────────┼──────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────┐
│       Repository Layer (traits)         │
│     ┌────────────────────────┐          │
│     │   FileRepository       │          │ <-- Trait exists!
│     │      (trait)           │          │     (underutilized)
│     └───────────┬────────────┘          │
└─────────────────┼──────────────────────┘
                  │
                  ▼
      ┌───────────────────────┐
      │    Repository Impl    │
      │   (SQL queries)       │
      └───────────────────────┘
```

### Key Components

#### 1. Repository (Concrete Implementation)
- **Location**: `codetriever-meta-data/src/repository.rs`
- **Purpose**: Executes actual SQL queries
- **State**: Holds connection pools
- **Methods**: 60+ database operations

#### 2. FileRepository Trait
- **Location**: `codetriever-meta-data/src/traits.rs`
- **Purpose**: Abstraction for testing
- **Status**: **EXISTS BUT UNDERUTILIZED!**
- **Implementation**: Repository implements this trait
- **Problem**: API layer doesn't use it

#### 3. DataClient (Facade)
- **Location**: `codetriever-meta-data/src/client.rs`
- **Purpose**: Simplify API for consumers
- **Wraps**: Repository instance
- **Methods**: Delegates to Repository
- **Type**: **Concrete struct** (not a trait)

#### 4. MockDataClient
- **Location**: `codetriever-meta-data/src/mock.rs`
- **Purpose**: Testing without database
- **Problem**: Different type than DataClient, can't substitute
- **Issue**: No common trait to unify them

#### 5. AppState
- **Location**: `codetriever-api/src/state.rs`
- **Contains**:
  - `db_client: Arc<DataClient>` (concrete)
  - `vector_storage: Arc<dyn VectorStorage>` (trait)
  - `search_service: Arc<dyn SearchProvider>` (trait)
  - `indexer_service: Arc<Mutex<dyn IndexerService>>` (trait)
- **Inconsistency**: Only `db_client` is concrete type!

### Problems

1. **Mixed abstraction levels**:
   - Search/Index use trait objects
   - Status/Context use concrete DataClient
   - No consistency

2. **FileRepository trait ignored**:
   - Exists in data layer
   - Not exposed to API layer
   - Could solve the abstraction problem

3. **DataClient not trait-based**:
   - Concrete type
   - Can't be mocked at API layer
   - Forces integration tests

4. **AppState inconsistency**:
   - 3 services use traits (`dyn Trait`)
   - 1 service uses concrete type (`DataClient`)
   - Why the difference?

---

## Identified Anti-Patterns

### 1. Inconsistent Dependency Injection

**Problem**: Each endpoint does DI differently

- Health: No DI
- Search: `Arc<dyn SearchProvider>` (clean trait)
- Index: `Arc<Mutex<dyn IndexerService>>` (trait + mutex)
- Status: Full `AppState` (too much coupling)
- Context: `Arc<DataClient>` (concrete type, no abstraction)

**Impact**: No standard pattern to follow for new endpoints

---

### 2. Mixed Return Types

**Problem**: Handlers return different types

- Health: `Json<Value>`
- Search: `ApiResult<Json<T>>`
- Index: `impl IntoResponse`
- Status: `Json<T>`
- Context: `ApiResult<Json<T>>`

**Impact**:
- Inconsistent error handling
- Different testing approaches
- Confusing to maintain

---

### 3. Inconsistent Error Handling

**Problem**: 3 different error strategies

| Strategy | Endpoints | Issues |
|----------|-----------|---------|
| **No errors** | Health | Can't communicate failures |
| **ApiError enum** | Search, Context | ✅ Good (structured) |
| **Manual StatusCode** | Index | Wrong codes (200 for errors!) |
| **Silent swallowing** | Status | Errors become "disconnected" |

**Impact**: No unified error experience for API consumers

---

### 4. Missing Correlation IDs

**Problem**: Only 2 of 5 endpoints track requests

| Endpoint | Correlation ID | Impact |
|----------|----------------|--------|
| Health | ❌ | Can't track health check requests |
| Search | ✅ | Full traceability |
| Index | ❌ | **Can't debug indexing issues** |
| Status | ❌ | Can't track status requests |
| Context | ✅ | Full traceability |

**Impact**: 60% of endpoints can't be traced in production logs

---

### 5. Inconsistent Observability

**Problem**: Mixed tracing/logging approaches

| Endpoint | Tracing Macro | Manual Logging | Timing |
|----------|---------------|----------------|--------|
| Health | ❌ | ❌ | ❌ |
| Search | ✅ `#[instrument]` | ✅ Structured | ✅ Manual |
| Index | ❌ | ⚠️ Error only | ❌ |
| Status | ❌ | ❌ | ✅ Uptime |
| Context | ✅ `#[instrument]` | ✅ Structured | ✅ Manual |

**Impact**: Can't monitor/debug 60% of endpoints effectively

---

### 6. Status Endpoint Taking Full AppState

**Problem**: Over-coupling to entire application state

```rust
pub fn routes(state: AppState) -> Router {
    // Takes EVERYTHING even though it only needs:
    // - db_client
    // - vector_storage
}
```

**Impact**:
- Can't test in isolation
- Requires constructing entire AppState
- Violates dependency inversion principle
- Harder to refactor

**Better approach**: Take only what it needs
```rust
pub fn routes(
    db_client: Arc<dyn DatabaseOperations>,
    vector_storage: Arc<dyn VectorStorage>,
) -> Router
```

---

### 7. Context Using Concrete DataClient

**Problem**: Recently refactored to remove trait, now untestable

**Before** (had trait abstraction):
```rust
pub fn routes(db_client: Arc<dyn DatabaseClient>) -> Router
```

**After** (broken - uses concrete type):
```rust
pub fn routes(db_client: Arc<DataClient>) -> Router
```

**Impact**:
- **Tests disabled** (can't inject mock)
- Can't test without real database
- Inconsistent with search/index pattern

---

### 8. FileRepository Trait Underutilized

**Problem**: Perfect abstraction exists but isn't used

**What exists**:
```rust
// codetriever-meta-data/src/traits.rs
pub trait FileRepository: Send + Sync {
    async fn get_file_content(...) -> DatabaseResult<Option<String>>;
    // ... 60+ methods
}

impl FileRepository for Repository { /* SQL queries */ }
```

**What's NOT happening**:
- DataClient doesn't expose this trait to API layer
- API routes don't use the trait
- AppState uses concrete `DataClient`

**Solution**: Expose trait to API layer for DI

---

## Proposed Unified Pattern

### Design Goals

1. **Consistency**: All endpoints follow the same pattern
2. **Testability**: Easy to mock all dependencies
3. **Observability**: Correlation IDs and tracing everywhere
4. **Error handling**: Unified ApiError across all endpoints
5. **Minimal coupling**: Inject only what's needed

### Recommended Pattern

#### A. Route Registration
```rust
pub fn routes(
    dependency: Arc<dyn ServiceTrait>,
    // ... other deps as needed
) -> Router {
    Router::new()
        .route("/endpoint", post(handler))
        .with_state(dependency)
}
```

**Principles**:
- Accept trait objects (`Arc<dyn Trait>`)
- Inject minimal dependencies
- Use `routes_with_*` naming for clarity

#### B. Handler Signature
```rust
#[utoipa::path(
    post,
    path = "/endpoint",
    tag = "category",
    request_body = RequestType,
    responses(
        (status = 200, description = "Success", body = ResponseType),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal error")
    )
)]
#[instrument(skip(service), fields(correlation_id))]
pub async fn handler(
    State(service): State<Arc<dyn ServiceTrait>>,
    context: Option<Extension<RequestContext>>,
    Json(req): Json<RequestType>,
) -> ApiResult<Json<ResponseType>> {
    handler_impl(service, context, req).await
}
```

**Principles**:
- Always use `#[utoipa::path]`
- Always use `#[instrument]`
- Always accept `Option<Extension<RequestContext>>`
- Always return `ApiResult<Json<T>>`
- Delegate to `*_impl()` for testability

#### C. Handler Implementation
```rust
async fn handler_impl(
    service: Arc<dyn ServiceTrait>,
    context: Option<Extension<RequestContext>>,
    req: RequestType,
) -> ApiResult<Json<ResponseType>> {
    let start = Instant::now();

    // 1. Extract correlation ID
    let correlation_id = context
        .as_ref()
        .map_or_else(CorrelationId::new, |ctx| ctx.correlation_id.clone());

    tracing::Span::current().record("correlation_id", correlation_id.to_string());

    // 2. Log request
    info!(
        correlation_id = %correlation_id,
        field = %req.field,
        "Processing request"
    );

    // 3. Validate request
    if req.field.is_empty() {
        warn!(correlation_id = %correlation_id, "Validation failed");
        return Err(ApiError::InvalidRequest { correlation_id, /* ... */ });
    }

    // 4. Call service with timeout
    let result = match tokio::time::timeout(
        Duration::from_secs(30),
        service.do_operation(req.field)
    ).await {
        Ok(Ok(data)) => data,
        Ok(Err(service_error)) => {
            error!(correlation_id = %correlation_id, error = %service_error, "Service failed");
            return Err(ApiError::ServiceUnavailable { correlation_id });
        }
        Err(_timeout) => {
            error!(correlation_id = %correlation_id, "Request timed out");
            return Err(ApiError::Timeout { correlation_id });
        }
    };

    // 5. Log success
    info!(
        correlation_id = %correlation_id,
        duration_ms = start.elapsed().as_millis(),
        "Request completed"
    );

    // 6. Return response
    Ok(Json(ResponseType { data, /* ... */ }))
}
```

**Principles**:
- Always extract and log correlation ID
- Always validate inputs
- Always use timeout protection
- Always log at info/warn/error levels
- Always measure and log timing
- Always use structured errors

#### D. Data Access Pattern

**Option 1: Expose FileRepository Trait (Recommended)**

```rust
// codetriever-meta-data/src/lib.rs
pub use traits::FileRepository;

// codetriever-api/src/state.rs
pub struct AppState {
    pub db_client: Arc<dyn FileRepository>,  // Trait, not concrete!
    pub vector_storage: Arc<dyn VectorStorage>,
    pub search_service: Arc<dyn SearchProvider>,
    pub indexer_service: Arc<Mutex<dyn IndexerService>>,
}

// Routes take trait objects
pub fn routes(db_client: Arc<dyn FileRepository>) -> Router { /* ... */ }
```

**Option 2: Create API-Layer Database Trait**

```rust
// codetriever-api/src/database.rs
#[async_trait]
pub trait DatabaseOperations: Send + Sync {
    async fn get_file_content(...) -> Result<Option<String>>;
    async fn count_files(...) -> Result<i64>;
    // Only the methods API needs
}

// Implement for DataClient
impl DatabaseOperations for DataClient { /* delegate to repository */ }
impl DatabaseOperations for MockDataClient { /* test data */ }
```

---

## Migration Path

### Phase 1: Fix Context Endpoint (Immediate)

**Goal**: Make context testable again

**Steps**:
1. Expose `FileRepository` trait from `codetriever-meta-data`
2. Update `context::routes()` to accept `Arc<dyn FileRepository>`
3. Update `AppState.db_client` to be `Arc<dyn FileRepository>`
4. Re-enable context endpoint tests

**Impact**: Low risk, fixes broken tests

---

### Phase 2: Standardize Error Handling (Short-term)

**Goal**: All endpoints use `ApiError`

**Steps**:
1. Update `index` endpoint to return `ApiResult<Json<T>>`
2. Fix 200 OK for business errors (should be 400/500)
3. Update `status` endpoint to return `ApiResult<Json<T>>`
4. Add proper error cases instead of silent swallowing

**Impact**: Better error consistency, breaking change for index endpoint

---

### Phase 3: Add Missing Observability (Short-term)

**Goal**: All endpoints have correlation IDs and tracing

**Steps**:
1. Add `Option<Extension<RequestContext>>` to all handlers
2. Add `#[instrument]` to all handlers
3. Extract correlation ID in all handlers
4. Add timing metrics to all handlers

**Endpoints to update**: `health`, `index`, `status`

**Impact**: Full request traceability in production

---

### Phase 4: Standardize Dependency Injection (Medium-term)

**Goal**: All endpoints use trait objects for DI

**Steps**:
1. Decide: Use `FileRepository` or create API-specific trait?
2. Update `AppState` to use traits consistently
3. Update `status` endpoint to NOT take full AppState
4. Ensure all services in AppState are trait objects

**Impact**: Consistent DI pattern, improved testability

---

### Phase 5: Add RouteHandler Trait (Long-term)

**Goal**: Enforce consistent pattern via compile-time checks

**Possible design**:
```rust
#[async_trait]
pub trait RouteHandler {
    type Request: DeserializeOwned;
    type Response: Serialize;
    type Service;

    async fn handle(
        service: Arc<Self::Service>,
        context: Option<Extension<RequestContext>>,
        request: Self::Request,
    ) -> ApiResult<Json<Self::Response>>;
}
```

**Impact**: Compile-time enforcement of pattern consistency

---

## Recommendations

### Immediate Actions (This Week)

1. **Fix context endpoint** (broken tests)
   - Expose `FileRepository` trait
   - Update context to use trait
   - Re-enable tests

2. **Document decision** (this document)
   - Share with team
   - Discuss preferred patterns
   - Get consensus

### Short-term Actions (This Sprint)

3. **Standardize error handling**
   - Migrate all to `ApiError`
   - Fix index endpoint 200 OK bug

4. **Add observability**
   - Correlation IDs everywhere
   - Tracing everywhere

### Medium-term Actions (Next Sprint)

5. **Unify DI pattern**
   - Decide on trait strategy
   - Update AppState
   - Refactor status endpoint

6. **Create pattern template**
   - Document "new endpoint checklist"
   - Create code generator/scaffold

### Long-term Actions (Backlog)

7. **RouteHandler trait**
   - Design and prototype
   - Migrate existing endpoints
   - Update documentation

---

## Decision Points

### 1. Database Trait Strategy

**Option A: Use existing FileRepository trait**
- ✅ Already exists and implemented
- ✅ Comprehensive (60+ methods)
- ❌ Exposes too much to API layer?
- ❌ Tied to data layer implementation

**Option B: Create API-specific DatabaseOperations trait**
- ✅ Only exposes what API needs
- ✅ Clean separation of concerns
- ❌ Another abstraction layer
- ❌ More code to maintain

**Option C: Keep DataClient concrete, use it everywhere**
- ✅ Simple, no abstraction
- ❌ **Can't mock for testing**
- ❌ Tight coupling
- ❌ Inconsistent with other services

**Recommendation**: **Option A** - Use FileRepository trait
- Already exists and is well-designed
- Matches pattern of SearchProvider/IndexerService
- Enables testing
- Can be refined later if needed

---

### 2. Error Handling Strategy

**Current state**: 3 different approaches

**Recommendation**: **Standardize on ApiError enum**
- Already used by 2 endpoints (search, context)
- Structured, correlation ID included
- Maps to HTTP status codes
- Works with `ApiResult<T>` type

**Migration**:
- Index: Return 400/500 instead of 200
- Status: Return errors instead of silent degradation
- Health: Add error cases (service checks?)

---

### 3. Status Endpoint Dependencies

**Current**: Takes full AppState

**Options**:
- Keep taking AppState (simple but coupled)
- Take individual dependencies (clean but more parameters)
- Create StatusDependencies struct (middle ground)

**Recommendation**: **Take individual dependencies**
```rust
pub fn routes(
    db_client: Arc<dyn FileRepository>,
    vector_storage: Arc<dyn VectorStorage>,
) -> Router
```

---

### 4. Return Type Strategy

**Options**:
- `ApiResult<Json<T>>` everywhere (recommended)
- `impl IntoResponse` everywhere (flexible but less type-safe)
- Mixed based on needs (current state - confusing)

**Recommendation**: **ApiResult<Json<T>>** everywhere
- Type-safe
- Consistent
- Forces proper error handling

---

## Conclusion

Codetriever API has **5 endpoints with 5 different architectural patterns**. This creates:
- High maintenance burden
- Testing complexity
- Onboarding challenges
- Architectural drift

**Root cause**: No established pattern when first endpoints were created.

**Solution**: Standardize on a unified pattern that emphasizes:
1. Trait-based dependency injection
2. Structured error handling (`ApiError`)
3. Observability (correlation IDs, tracing)
4. Consistent return types (`ApiResult<Json<T>>`)
5. Minimal coupling (inject only what's needed)

**Next steps**:
1. Fix context endpoint (broken tests)
2. Discuss and decide on database trait strategy
3. Create migration plan with team
4. Execute phased refactoring
5. Document standard pattern for new endpoints

---

## DECISIONS MADE (2025-10-25)

### 1. Health Check Enhancement
**Decision**: Health check should verify actual service connectivity
- PostgreSQL: `SELECT NOW()` (fast, proves connection)
- Qdrant: Collection exists check
- Return proper errors if checks fail
- Add correlation ID tracking

### 2. Search Routes Cleanup
**Decision**: Remove lazy initialization variants
- Keep ONLY `routes_with_search_service(Arc<dyn SearchService>)`
- Rename to `routes(search_service: Arc<dyn SearchService>)`
- Delete `routes()` and `routes_with_lazy_search()`
- Force explicit dependency injection

### 3. Correlation IDs Everywhere
**Decision**: **MANDATORY** correlation ID handling in ALL endpoints
- Every handler MUST accept `Option<Extension<RequestContext>>`
- Every handler MUST extract correlation ID
- Every handler MUST log correlation ID
- Every error MUST include correlation ID

### 4. Global Timeout Handling
**Decision**: All service calls MUST have timeouts
- Default: 30 seconds
- Configurable per endpoint if needed
- Consistent timeout error handling

### 5. Naming Convention: Service Over Provider
**Decision**: Use `*Service` suffix consistently
- ✅ `SearchService` (not SearchProvider)
- ✅ `IndexerService`
- ✅ `StatusService` (new)
- ✅ `ContextService` (new?)

### 6. Parameterless routes() Functions
**Decision**: All route handlers expose ONLY `routes()` with NO parameters
- Dependencies injected via service constructors
- Services passed to `routes()` internally (encapsulated)
- Clean, consistent API for route registration

**Example**:
```rust
// OLD (inconsistent)
pub fn routes_with_indexer(indexer: Arc<Mutex<dyn IndexerService>>) -> Router

// NEW (clean)
pub fn routes() -> Router  // Dependencies handled internally
```

### 7. Status Endpoint Needs StatusService
**Decision**: Create `StatusService` to encapsulate status logic
- Takes database + vector storage dependencies
- Exposes `get_status()` method
- Status handler just calls service
- Matches pattern of search/index

### 8. Context Endpoint Needs Complete Refactor
**Decision**: Context endpoint is broken and needs redesign
- Create `ContextService` (or use DataClient via trait)
- Fix dependency injection
- Re-enable tests
- Follow unified pattern

---

## Revised Unified Pattern

### Standard Route Handler Pattern

**Every endpoint MUST follow this pattern**:

```rust
// 1. Service creation (in service module)
pub struct MyService {
    dependency: Arc<dyn SomeTrait>,
}

impl MyService {
    pub fn new(dependency: Arc<dyn SomeTrait>) -> Self {
        Self { dependency }
    }
}

// 2. Route registration (parameterless!)
pub fn routes() -> Router {
    let service = Arc::new(MyService::new(/* initialized elsewhere */));
    Router::new()
        .route("/endpoint", post(handler))
        .with_state(service)
}

// 3. Handler signature (MANDATORY elements)
#[utoipa::path(/* ... */)]
#[instrument(skip(service), fields(correlation_id))]
pub async fn handler(
    State(service): State<Arc<MyService>>,
    context: Option<Extension<RequestContext>>,  // MANDATORY
    Json(req): Json<RequestType>,
) -> ApiResult<Json<ResponseType>> {
    // ALWAYS extract correlation ID
    let correlation_id = context
        .as_ref()
        .map_or_else(CorrelationId::new, |ctx| ctx.correlation_id.clone());

    // ALWAYS add to span
    tracing::Span::current().record("correlation_id", correlation_id.to_string());

    // ALWAYS use timeout
    match tokio::time::timeout(Duration::from_secs(30), service.do_thing()).await {
        Ok(Ok(result)) => Ok(Json(ResponseType { result })),
        Ok(Err(e)) => Err(ApiError::ServiceFailed { correlation_id }),
        Err(_) => Err(ApiError::Timeout { correlation_id }),
    }
}
```

**Non-negotiable requirements**:
1. ✅ `routes()` is parameterless
2. ✅ Handler accepts `Option<Extension<RequestContext>>`
3. ✅ Correlation ID extracted and logged
4. ✅ `#[instrument]` macro present
5. ✅ `#[utoipa::path]` macro present
6. ✅ Returns `ApiResult<Json<T>>`
7. ✅ Uses structured `ApiError`
8. ✅ Timeout protection on service calls

---

**Author**: Marvin (AI Agent)
**Date**: 2025-10-25
**Status**: **DECISIONS FINALIZED** - Ready for Implementation
**Approved By**: @clafollett
