# Code Review: /usages Endpoint (Issue #15)

**Branch**: `feature/issue-15-usages-endpoint`  
**Date**: 2026-04-03  
**Reviewer**: Code Reviewer Agent  
**Files Reviewed**:
- `crates/codetriever-api/src/routes/search.rs`
- `crates/codetriever-api/src/openapi.rs`
- `crates/codetriever-search/src/lib.rs`
- `crates/codetriever-search/src/searching/test_utils.rs`

---

## Summary

The core implementation is solid — the handler works, validation exists, tests are meaningful, and the OpenAPI registration is complete. There are no security vulnerabilities and no critical functional defects. The issues below are all **Major** or lower and should be addressed before merge.

---

## Issues

### 1. `extract_repo_commit` Duplicates Inline Logic from `search_handler_impl`

**Category**: Major  
**Location**: `search.rs:932-957` vs `search.rs:462-491` and `search.rs:1309-1331`

The logic to extract repository name and commit info from `RepositoryMetadata` appears **three times**:

1. Inline in `search_handler_impl` (lines 462-491)
2. Extracted into `extract_repo_commit` (lines 932-957) — used only by `usages_handler`
3. Duplicated again inline in `test_search_results_include_repository_and_commit_info` (lines 1309-1331)

Per `AGENT_INSTRUCTIONS.md` directive 7 ("REFACTOR, DON'T WRAP"), `search_handler_impl` should be refactored to call `extract_repo_commit` instead of repeating the logic. The test at line 1309 should also be rewritten to test `extract_repo_commit` directly rather than re-implementing the extraction.

**Recommendation**: Refactor `search_handler_impl` to call `extract_repo_commit`. The function already exists — use it.

---

### 2. Case Sensitivity Mismatch Between `is_definition` and `is_reference`

**Category**: Major  
**Location**: `search.rs:915-926`

`is_definition` uses `eq_ignore_ascii_case` for the name comparison. `is_reference` calls `chunk.content.contains(symbol)` which is **case-sensitive**.

```rust
fn is_definition(chunk: &CodeChunk, symbol: &str) -> bool {
    chunk.name.as_ref().is_some_and(|n| n.eq_ignore_ascii_case(symbol)) && chunk.kind.is_some()
}

fn is_reference(chunk: &CodeChunk, symbol: &str) -> bool {
    !is_definition(chunk, symbol) && chunk.content.contains(symbol)  // case-sensitive
}
```

**Concrete failure**: symbol `"ParseConfig"` (e.g., Go/C# PascalCase) — `is_definition` would match a chunk named `"parseconfig"`, but `is_reference` would miss `"parseConfig"` in content. The opposite also occurs: a language like Python where the caller spells the symbol differently in casing than the definition.

This is not just a style concern. For Go, C#, and Python codebases the mismatch will produce wrong classifications or missed references.

**Recommendation**: Make `is_reference` use a case-insensitive content check, or explicitly document that the endpoint is case-sensitive for content matching and add that to the API docs. If case-insensitive content search is chosen, be aware of the performance cost on large chunk content strings.

---

### 3. `UsagesRequest` Has `#[serde(skip_serializing_if)]` on a Deserialize-Only Struct

**Category**: Minor  
**Location**: `search.rs:846`, `search.rs:849`, `search.rs:851`

`UsagesRequest` is `#[derive(Debug, Deserialize, ToSchema)]` — it is never serialized. The `skip_serializing_if` attributes on its `repository_id` and `branch` fields are dead attributes that do nothing.

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub repository_id: Option<String>,
#[serde(skip_serializing_if = "Option::is_none")]
pub branch: Option<String>,
```

Same issue exists on `ContextRequest` (lines 556, 559, 564, 567). Both structs are Deserialize-only. Clippy may or may not catch this depending on the configured lints, but it is noise that adds confusion.

**Recommendation**: Remove all `skip_serializing_if` annotations from `UsagesRequest` and `ContextRequest`.

---

### 4. Performance: Client-Side Filter Against a Fixed-Cap Search

**Category**: Major  
**Location**: `search.rs:1035`, `search.rs:1083-1135`

The handler searches for `min(limit, 100)` chunks via semantic search, then filters the result set down to only chunks that contain the symbol string. For popular symbols with broad semantic relevance (e.g., `"config"`, `"error"`, `"new"`), the semantic search will return 100 results, most of which may not contain the literal string, leaving the response with very few usages.

Example: symbol = `"Config"`, limit = 50. Semantic search returns 50 results about configuration broadly. After `is_definition`/`is_reference` filtering, only 3 actually contain the string `"Config"`. The user gets 3 results but there may be 80 definitions in the index.

There is no overfetch compensation — no second search pass when filtered results are well below the requested limit.

**Recommendation**: Either (a) fetch a multiplier of the requested limit (e.g., `limit * 3`, capped at a reasonable absolute maximum) and return the first `limit` post-filter results, or (b) document this behavior clearly in the OpenAPI response description so callers know that `total_usages` may be lower than expected. At minimum, add a `// NOTE:` comment in the handler explaining the tradeoff.

---

### 5. `usage_type` Validated After Normalization but Error Message Reports Original

**Category**: Minor  
**Location**: `search.rs:1022-1032`

```rust
let usage_type = req.usage_type.to_lowercase();
if !["all", "definitions", "references"].contains(&usage_type.as_str()) {
    return Err(ApiError::invalid_query(
        req.symbol,                  // <-- uses symbol, not usage_type
        format!("Invalid usage_type '{}'. Must be...", req.usage_type),
        correlation_id,
    ));
}
```

Two problems:
1. The first argument to `invalid_query` passes `req.symbol` as the "query" field in the error. The invalid value is `req.usage_type` — that should be the reported field.
2. `usage_type` is missing a `warn!` tracing call before returning the error, inconsistent with how the empty-symbol and too-long-symbol validation paths are handled (lines 1004, 1013).

**Recommendation**: Pass `req.usage_type` as the first arg to `invalid_query`, and add a `warn!` log before the return.

---

### 6. Missing Test: Symbol That Is Both a Definition Name AND Appears in Its Own Content

**Category**: Minor  
**Location**: `search.rs` test module

The classification logic short-circuits: `is_definition` wins if name matches. There is no test verifying that a chunk where `name == symbol` AND `content.contains(symbol)` is classified as `"definition"` and NOT double-counted as both. Given the current logic this works correctly, but the invariant is untested. A future refactor of `is_reference` to not call `is_definition` internally (e.g., a match enum approach) could break this silently.

**Recommendation**: Add a test case with a chunk where `name == symbol` and the content also contains the symbol string, asserting `usage_type == "definition"` and `metadata.references == 0`.

---

### 7. Missing Test: Symbol with Whitespace-Only Padding (Trim Behavior)

**Category**: Minor  
**Location**: `search.rs:1003`

`test_usages_rejects_empty_symbol` tests `"   "` (whitespace-only). The validation does `req.symbol.trim().is_empty()` — correct. However, if a caller sends `"  parse_config  "` (padded but non-empty), the trimmed check passes and the raw padded symbol is used in the search and `is_reference` content comparison. There is no trimming of the symbol before use.

This is probably fine in practice (no real caller would pad a symbol), but the length check also uses the raw `req.symbol.len()` (line 1012) which counts the padding. A symbol of 498 spaces + 2 chars would pass as "500 chars", and the actual 2-char symbol used in search would be semantically meaningless.

**Recommendation**: Trim `req.symbol` at the start of the handler and use the trimmed value throughout. Or add a test documenting the current behavior as intentional.

---

### 8. `test_search_results_include_repository_and_commit_info` Doesn't Test the Handler

**Category**: Minor  
**Location**: `search.rs:1277-1339`

This test manually reimplements the repository/commit extraction logic instead of routing through the actual handler. It asserts against locally constructed values — it cannot detect a regression in `search_handler_impl` itself. It is testing dead code paths in the test body.

**Recommendation**: Either delete this test (the behavior is already covered by `test_search_response_includes_repository_fields` and `test_search_results_include_repository_and_commit_info` indirectly) or refactor it to go through the Axum router with a `MockSearch::with_matches` that carries `repository_metadata`, then assert the JSON response fields. The latter would be the high-value version of this test.

---

### 9. `MockSearch::with_results` Always Sets `name: None`

**Category**: Minor  
**Location**: `test_utils.rs:54-77`

`with_results` hardcodes `name: None` and `kind: Some("function")` for every result. This means any test using `with_results` that exercises the usages classification path would produce incorrect results (all chunks would be `is_reference` since `name` is always `None`). This is fine today because the usages tests use `with_matches`, but it's a hidden trap for future test authors.

**Recommendation**: Add a doc comment to `with_results` noting that it does not populate `name`, and is not suitable for usages classification testing. Or have it accept names as part of the tuple — but given the existing `with_matches` API, a comment is sufficient.

---

## Non-Issues (Verified Clean)

- OpenAPI registration in `openapi.rs` is complete and correct. All four new types (`UsagesRequest`, `UsagesResponse`, `UsagesMetadata`, `Usage`) are registered.
- `extract_repo_commit` type alias `RepoCommitInfo` is clear and doesn't violate the no-type-alias-for-renaming rule (it's a local structural alias, not a rename of an existing type).
- Sort logic (definitions first, then by similarity descending) is correct. The `i32::from(t != "definition")` trick is readable.
- Timeout and error handling in `usages_handler` mirrors `search_handler_impl` appropriately.
- `default_usage_type()` function is the correct serde pattern for default field values.
- `TestSearchMatch` struct and `MockSearch::with_matches` are clean additions. The `#[cfg(any(test, feature = "test-utils"))]` gate is correct.
- Symbol length cap at 500 is reasonable and documented.

---

## Required Before Merge — ALL RESOLVED

All issues below were fixed in subsequent commits on this branch.

| # | Issue | Status |
|---|-------|--------|
| 1 | Refactor `search_handler_impl` to use `extract_repo_commit` | ✅ Fixed |
| 2 | Case sensitivity mismatch in `is_definition`/`is_reference` | ✅ Fixed — `is_reference` now case-insensitive with precomputed `symbol_lower` |
| 3 | Client-side filter against fixed-cap fetch | ✅ Fixed — overfetch 3x, truncate after sort |
| 4 | `invalid_query` error passes wrong field + missing `warn!` log | ✅ Fixed |
| 5 | Dead `skip_serializing_if` on Deserialize-only structs | ✅ Removed |
| 6 | Missing test: definition not double-counted as reference | ✅ Added |
| 7 | Symbol whitespace not trimmed | ✅ Trimmed after validation |
| 8 | Meaningless repo/commit test | ✅ Replaced with `extract_repo_commit` unit test |
| 9 | `MockSearch::with_results` missing doc warning | ✅ Added |
| 10 | `SearchServiceUnavailable` mapping missing in usages handler | ✅ Added (Copilot follow-up) |
| 11 | Metadata counts inconsistent after truncation | ✅ Fixed — recomputed post-truncation (Copilot follow-up) |
| 12 | `is_reference` per-call allocation | ✅ Fixed — precompute `symbol_lower` (Copilot follow-up) |
