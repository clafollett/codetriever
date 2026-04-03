//! Chunking service for token-aware text splitting

use super::traits::TokenCounterRef;
use crate::ParsingResult;
use crate::parsing::code_parser::CodeChunk;

/// Token budget configuration for chunking
#[derive(Debug, Clone, Copy)]
pub struct TokenBudget {
    /// Absolute maximum tokens (model limit)
    pub hard: usize,
    /// Target tokens (usually 90% of hard limit)
    pub soft: usize,
    /// Number of tokens to overlap between chunks
    pub overlap: usize,
}

impl TokenBudget {
    /// Create a new token budget
    pub fn new(max_tokens: usize, overlap_tokens: usize) -> Self {
        Self {
            hard: max_tokens,
            soft: (max_tokens as f64 * 0.9) as usize, // 90% target
            overlap: overlap_tokens,
        }
    }
}

/// Code span with both line and byte tracking
#[derive(Debug, Clone)]
pub struct CodeSpan {
    /// The actual code content
    pub content: String,
    /// Starting line number (1-indexed)
    pub start_line: usize,
    /// Ending line number (1-indexed)
    pub end_line: usize,
    /// Byte offset from start of file
    pub byte_start: usize,
    /// Byte offset of end (exclusive)
    pub byte_end: usize,
    /// Optional type/kind (e.g., "function", "class")
    pub kind: Option<String>,
    /// Optional name (e.g., function or class name)
    pub name: Option<String>,
    /// Language of the code
    pub language: String,
}

/// Service for chunking code based on token counts
pub struct ChunkingService {
    counter: TokenCounterRef,
    budget: TokenBudget,
}

impl ChunkingService {
    /// Create a new chunking service
    pub fn new(counter: TokenCounterRef, budget: TokenBudget) -> Self {
        Self { counter, budget }
    }

    /// Chunk a list of code spans into token-limited chunks
    pub fn chunk_spans(
        &self,
        file_path: &str,
        spans: Vec<CodeSpan>,
    ) -> ParsingResult<Vec<CodeChunk>> {
        let mut chunks = Vec::new();
        let mut current_content = String::new();
        let mut current_start_line = 0;
        let mut current_end_line = 0;
        let mut current_byte_start = 0;
        let mut current_byte_end = 0;
        let mut current_tokens = 0;
        let mut current_kind: Option<String> = None;
        let mut current_name: Option<String> = None;
        let mut current_language = String::new();

        for span in spans.into_iter() {
            let span_tokens = self.counter.count(&span.content);

            // If this single span exceeds hard limit, split it
            if span_tokens > self.budget.hard {
                // First, flush any accumulated content
                if !current_content.is_empty() {
                    chunks.push(CodeChunk {
                        file_path: file_path.to_string(),
                        content: current_content.clone(),
                        start_line: current_start_line,
                        end_line: current_end_line,
                        byte_start: current_byte_start,
                        byte_end: current_byte_end,
                        kind: current_kind.clone(),
                        language: current_language.clone(),
                        name: current_name.clone(),
                        token_count: Some(current_tokens),
                        embedding: None,
                    });
                    current_content.clear();
                    current_tokens = 0;
                }

                // Split the large span
                let split_chunks = self.split_large_span(file_path, span)?;
                chunks.extend(split_chunks);
                continue;
            }

            // Determine whether this span and the accumulated content are each
            // top-level definitions (function, class, struct, etc.).
            let definition_kinds = [
                "function",
                "method",
                "class",
                "struct",
                "enum",
                "trait",
                "impl",
                "interface",
            ];
            let is_new_definition = span
                .kind
                .as_deref()
                .is_some_and(|k| definition_kinds.contains(&k));
            let current_has_definition = current_kind
                .as_deref()
                .is_some_and(|k| definition_kinds.contains(&k));

            // Prefer separate chunks when BOTH spans are substantive (≥ 25% of
            // soft limit) top-level definitions with different names.  Tiny
            // helpers below the threshold are packed with the next span because
            // they are almost certainly related context.
            let substantive_threshold = self.budget.soft / 4;
            let should_split_at_ast_boundary = is_new_definition
                && current_has_definition
                && !current_content.is_empty()
                && span.name != current_name
                && current_tokens >= substantive_threshold;

            // Check if adding this span would exceed soft limit OR if an AST
            // semantic boundary demands a split.
            if (current_tokens + span_tokens > self.budget.soft || should_split_at_ast_boundary)
                && !current_content.is_empty()
            {
                // Create chunk from accumulated content
                chunks.push(CodeChunk {
                    file_path: file_path.to_string(),
                    content: current_content.clone(),
                    start_line: current_start_line,
                    end_line: current_end_line,
                    byte_start: current_byte_start,
                    byte_end: current_byte_end,
                    kind: current_kind.clone(),
                    language: current_language.clone(),
                    name: current_name.clone(),
                    token_count: Some(current_tokens),
                    embedding: None,
                });

                // Start new chunk - take ownership since we're consuming the span
                current_content = span.content;
                current_start_line = span.start_line;
                current_end_line = span.end_line;
                current_byte_start = span.byte_start;
                current_byte_end = span.byte_end;
                current_tokens = span_tokens;
                current_kind = span.kind;
                current_name = span.name;
                current_language = span.language;
            } else {
                // Accumulate span
                if current_content.is_empty() {
                    // First span - take ownership
                    current_content = span.content;
                    current_start_line = span.start_line;
                    current_byte_start = span.byte_start;
                    current_kind = span.kind;
                    current_name = span.name;
                    current_language = span.language;
                } else {
                    // Subsequent spans - we must append so we need the content
                    current_content.push('\n');
                    current_content.push_str(&span.content);
                }
                current_end_line = span.end_line;
                current_byte_end = span.byte_end;
                current_tokens += span_tokens;
            }
        }

        // Flush any remaining content
        if !current_content.is_empty() {
            chunks.push(CodeChunk {
                file_path: file_path.to_string(),
                content: current_content,
                start_line: current_start_line,
                end_line: current_end_line,
                byte_start: current_byte_start,
                byte_end: current_byte_end,
                kind: current_kind,
                language: current_language,
                name: current_name,
                token_count: Some(current_tokens),
                embedding: None,
            });
        }

        Ok(chunks)
    }

    /// Split a large span that exceeds the hard token limit
    fn split_large_span(&self, file_path: &str, span: CodeSpan) -> ParsingResult<Vec<CodeChunk>> {
        let mut chunks = Vec::new();
        let lines: Vec<&str> = span.content.lines().collect();

        let mut current_lines: Vec<String> = Vec::new();
        let mut current_tokens = 0;
        let mut current_start_line = span.start_line;
        let mut current_byte_offset = span.byte_start;

        let mut line_buffer = String::with_capacity(256);
        for line in lines.iter() {
            line_buffer.clear();
            line_buffer.push_str(line);
            line_buffer.push('\n');
            let line_tokens = self.counter.count(&line_buffer);

            if current_tokens + line_tokens > self.budget.soft && !current_lines.is_empty() {
                // Create chunk
                let content = current_lines.join("\n");
                let byte_len = content.len();

                chunks.push(CodeChunk {
                    file_path: file_path.to_string(),
                    content,
                    start_line: current_start_line,
                    end_line: current_start_line + current_lines.len() - 1,
                    byte_start: current_byte_offset,
                    byte_end: current_byte_offset + byte_len,
                    kind: span.kind.clone(),
                    language: span.language.clone(),
                    name: span.name.clone(),
                    token_count: Some(current_tokens),
                    embedding: None,
                });

                // Carry trailing lines as overlap into the next chunk.
                // Walk backward through current_lines, accumulating token counts
                // until we've collected up to `self.budget.overlap` tokens.
                let mut overlap_token_count = 0usize;
                let overlap_lines: Vec<String> = if self.budget.overlap > 0 {
                    let mut overlap_count = 0usize;
                    let mut buf = String::with_capacity(256);
                    for ol in current_lines.iter().rev() {
                        buf.clear();
                        buf.push_str(ol);
                        buf.push('\n');
                        let t = self.counter.count(&buf);
                        if overlap_token_count + t > self.budget.overlap {
                            break;
                        }
                        overlap_token_count += t;
                        overlap_count += 1;
                    }
                    current_lines[current_lines.len() - overlap_count..].to_vec()
                } else {
                    Vec::new()
                };

                // Compute overlap byte length using the same join("\n") representation
                // as chunk content to avoid off-by-one drift at boundaries.
                let overlap_byte_len = if overlap_lines.is_empty() {
                    0
                } else {
                    overlap_lines.join("\n").len()
                };
                let advance_bytes = byte_len.saturating_sub(overlap_byte_len);
                current_byte_offset += advance_bytes;

                // The next chunk starts at the line number of the first overlap line.
                let non_overlap_count = current_lines.len() - overlap_lines.len();
                current_start_line += non_overlap_count;

                // Seed the next chunk with the overlap lines and the token count
                // already computed during the reverse walk.
                current_lines = overlap_lines;
                current_tokens = overlap_token_count;
            }

            current_lines.push(line.to_string());
            current_tokens += line_tokens;
        }

        // Flush remaining lines
        if !current_lines.is_empty() {
            let content = current_lines.join("\n");
            chunks.push(CodeChunk {
                file_path: file_path.to_string(),
                content,
                start_line: current_start_line,
                end_line: span.end_line,
                byte_start: current_byte_offset,
                byte_end: span.byte_end,
                kind: span.kind.clone(),
                language: span.language.clone(),
                name: span.name.clone(),
                token_count: Some(current_tokens),
                embedding: None,
            });
        }

        Ok(chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunking::traits::TokenCounter;
    use std::sync::Arc;

    /// Simple word-counting token counter for deterministic tests.
    struct WordCounter;

    impl TokenCounter for WordCounter {
        fn name(&self) -> &str {
            "word-counter"
        }

        fn max_tokens(&self) -> usize {
            1024
        }

        fn count(&self, text: &str) -> usize {
            text.split_whitespace().count()
        }
    }

    /// Build a CodeSpan from a list of lines (for overlap tests).
    fn make_span(lines: &[&str]) -> CodeSpan {
        let content = lines.join("\n");
        let byte_end = content.len();
        CodeSpan {
            content,
            start_line: 1,
            end_line: lines.len(),
            byte_start: 0,
            byte_end,
            kind: None,
            name: None,
            language: "rust".to_string(),
        }
    }

    /// Build a CodeSpan with a specific kind, name, and exact word count (for AST boundary tests).
    fn make_definition_span(kind: &str, name: &str, word_count: usize) -> CodeSpan {
        let words: Vec<String> = (0..word_count).map(|i| format!("word_{i}")).collect();
        let content = words.join(" ");
        let byte_end = content.len();
        CodeSpan {
            content,
            start_line: 1,
            end_line: 1,
            byte_start: 0,
            byte_end,
            kind: Some(kind.to_string()),
            name: Some(name.to_string()),
            language: "rust".to_string(),
        }
    }

    /// Build a ChunkingService with overlap support.
    fn make_service_with_overlap(hard: usize, overlap: usize) -> ChunkingService {
        let budget = TokenBudget::new(hard, overlap);
        ChunkingService::new(Arc::new(WordCounter), budget)
    }

    /// Build a ChunkingService without overlap (for AST boundary tests).
    fn make_service(hard: usize) -> ChunkingService {
        make_service_with_overlap(hard, 0)
    }

    // =========================================================================
    // Overlap tests (split_large_span)
    // =========================================================================

    #[test]
    fn test_split_large_span_has_overlap() {
        let lines: Vec<String> = (1..=10)
            .map(|i| format!("word1_{i} word2_{i} word3_{i} word4_{i}"))
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let span = make_span(&line_refs);

        let svc = make_service_with_overlap(20, 4);
        let chunks = svc
            .split_large_span("test.rs", span)
            .expect("split should succeed");

        assert!(chunks.len() > 1, "expected multiple chunks, got {}", chunks.len());

        for window in chunks.windows(2) {
            let prev_last = window[0].content.lines().last().expect("chunk must have content");
            let next_first = window[1].content.lines().next().expect("chunk must have content");
            assert_eq!(prev_last, next_first, "overlap missing between chunks");
        }
    }

    #[test]
    fn test_overlap_token_count_respected() {
        let lines: Vec<String> = (1..=10)
            .map(|i| format!("word1_{i} word2_{i} word3_{i} word4_{i}"))
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let span = make_span(&line_refs);

        let svc = make_service_with_overlap(20, 6);
        let chunks = svc
            .split_large_span("test.rs", span)
            .expect("split should succeed");

        assert!(chunks.len() > 1, "expected multiple chunks");

        let counter = WordCounter;
        for window in chunks.windows(2) {
            let prev_lines: Vec<&str> = window[0].content.lines().collect();
            let next_lines: Vec<&str> = window[1].content.lines().collect();

            let max_overlap = next_lines.len().min(prev_lines.len());
            let overlap_line_count = (1..=max_overlap)
                .rev()
                .find(|&k| next_lines[..k] == prev_lines[prev_lines.len() - k..])
                .unwrap_or(0);

            let overlap_text = next_lines[..overlap_line_count].join("\n");
            let overlap_tokens = counter.count(&overlap_text);
            assert!(overlap_tokens <= 6, "overlap tokens {overlap_tokens} exceeded budget of 6");
        }
    }

    #[test]
    fn test_single_chunk_no_overlap_needed() {
        let lines = ["hello world", "foo bar", "baz qux"];
        let span = make_span(&lines);

        let svc = make_service_with_overlap(20, 4);
        let chunks = svc.split_large_span("test.rs", span).expect("split should succeed");
        assert_eq!(chunks.len(), 1, "expected a single chunk");
    }

    #[test]
    fn test_zero_overlap_no_change() {
        let lines: Vec<String> = (1..=10)
            .map(|i| format!("word1_{i} word2_{i} word3_{i} word4_{i}"))
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let span = make_span(&line_refs);

        let svc = make_service_with_overlap(20, 0);
        let chunks = svc.split_large_span("test.rs", span).expect("split should succeed");
        assert!(chunks.len() > 1, "expected multiple chunks");

        for window in chunks.windows(2) {
            let prev_last = window[0].content.lines().last().expect("content");
            let next_first = window[1].content.lines().next().expect("content");
            assert_ne!(prev_last, next_first, "zero-overlap should not share lines");
        }
    }

    #[test]
    fn test_overlap_byte_offsets_consistent() {
        let lines: Vec<String> = (1..=10)
            .map(|i| format!("word1_{i} word2_{i} word3_{i} word4_{i}"))
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let span = make_span(&line_refs);

        let svc = make_service_with_overlap(20, 4);
        let chunks = svc.split_large_span("test.rs", span).expect("split should succeed");
        assert!(chunks.len() > 1, "expected multiple chunks");

        for (i, chunk) in chunks.iter().enumerate() {
            let expected_len = chunk.content.len();
            let actual_len = chunk.byte_end - chunk.byte_start;
            assert_eq!(
                actual_len, expected_len,
                "chunk {i}: byte range ({}-{} = {actual_len}) != content.len() ({expected_len})",
                chunk.byte_start, chunk.byte_end
            );
        }
    }

    // =========================================================================
    // AST boundary tests (chunk_spans)
    // =========================================================================

    #[test]
    fn test_separate_named_functions_get_own_chunks() {
        let svc = make_service(100);
        let span_a = make_definition_span("function", "foo", 40);
        let span_b = make_definition_span("function", "bar", 40);

        let chunks = svc.chunk_spans("test.rs", vec![span_a, span_b]).expect("should succeed");

        assert_eq!(chunks.len(), 2, "two substantive functions should produce 2 chunks");
        assert_eq!(chunks[0].name.as_deref(), Some("foo"));
        assert_eq!(chunks[1].name.as_deref(), Some("bar"));
    }

    #[test]
    fn test_small_helper_packed_with_next_function() {
        let svc = make_service(100);
        let tiny_helper = make_definition_span("function", "helper", 9);
        let large_fn = make_definition_span("function", "process_data", 60);

        let chunks = svc.chunk_spans("test.rs", vec![tiny_helper, large_fn]).expect("should succeed");
        assert_eq!(chunks.len(), 1, "small helper below 25% should pack with next function");
    }

    #[test]
    fn test_non_definition_spans_still_pack() {
        let svc = make_service(100);

        let make_import = |n: usize| {
            let words: Vec<String> = (0..n).map(|i| format!("use_mod_{i}")).collect();
            let content = words.join(" ");
            let byte_end = content.len();
            CodeSpan {
                content,
                start_line: 1,
                end_line: 1,
                byte_start: 0,
                byte_end,
                kind: Some("import".to_string()),
                name: None,
                language: "rust".to_string(),
            }
        };

        let chunks = svc
            .chunk_spans("test.rs", vec![make_import(30), make_import(30)])
            .expect("should succeed");
        assert_eq!(chunks.len(), 1, "non-definition spans should pack together");
    }

    #[test]
    fn test_same_name_definitions_pack() {
        let svc = make_service(100);
        let impl_a = make_definition_span("impl", "MyStruct", 35);
        let impl_b = make_definition_span("impl", "MyStruct", 35);

        let chunks = svc.chunk_spans("test.rs", vec![impl_a, impl_b]).expect("should succeed");
        assert_eq!(chunks.len(), 1, "same-named impl blocks should stay packed");
    }
}
