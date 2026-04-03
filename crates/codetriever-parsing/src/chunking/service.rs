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

            // Check if adding this span would exceed soft limit
            if current_tokens + span_tokens > self.budget.soft && !current_content.is_empty() {
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
                let overlap_lines: Vec<String> = if self.budget.overlap > 0 {
                    let mut overlap_tokens = 0usize;
                    let mut overlap_count = 0usize;
                    let mut buf = String::with_capacity(256);
                    for ol in current_lines.iter().rev() {
                        buf.clear();
                        buf.push_str(ol);
                        buf.push('\n');
                        let t = self.counter.count(&buf);
                        if overlap_tokens + t > self.budget.overlap {
                            break;
                        }
                        overlap_tokens += t;
                        overlap_count += 1;
                    }
                    // Take the last `overlap_count` lines as the seed for the next chunk.
                    current_lines[current_lines.len() - overlap_count..].to_vec()
                } else {
                    Vec::new()
                };

                // Advance the byte offset by only the non-overlap portion.
                // The overlap lines will be re-emitted in the next chunk so their
                // bytes are NOT consumed here.
                let overlap_byte_len: usize = overlap_lines
                    .iter()
                    .map(|l| l.len() + 1) // +1 for the '\n' separator
                    .sum();
                // Guard against underflow when overlap_byte_len > byte_len (shouldn't
                // happen in practice, but be safe).
                let advance_bytes = byte_len.saturating_sub(overlap_byte_len);
                current_byte_offset += advance_bytes;

                // The next chunk starts at the line number of the first overlap line.
                let non_overlap_count = current_lines.len() - overlap_lines.len();
                // current_start_line is 1-indexed; lines before overlap are consumed.
                current_start_line += non_overlap_count;

                // Seed the next chunk with the overlap lines and their token count.
                let overlap_token_sum: usize = overlap_lines
                    .iter()
                    .map(|l| {
                        let mut b = l.clone();
                        b.push('\n');
                        self.counter.count(&b)
                    })
                    .sum();
                current_lines = overlap_lines;
                current_tokens = overlap_token_sum;
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
    /// Counts whitespace-separated words as tokens.
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

    /// Build a CodeSpan from a list of lines.
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

    /// Build a ChunkingService with the WordCounter and a given budget.
    fn make_service(hard: usize, overlap: usize) -> ChunkingService {
        let budget = TokenBudget::new(hard, overlap);
        ChunkingService::new(Arc::new(WordCounter), budget)
    }

    // ---------------------------------------------------------------------------
    // Test 1: overlap lines from chunk[i] appear at start of chunk[i+1]
    // ---------------------------------------------------------------------------
    #[test]
    fn test_split_large_span_has_overlap() {
        // 10 lines × 4 words each = 40 words (tokens).
        // hard=20, soft=18, overlap=4.
        // WordCounter counts whitespace-separated words, so each line = 4 tokens.
        // Lines accumulate: 4→8→12→16 → at line 5: 16+4=20 > 18 → emit chunk
        // with lines 1–4 (16 tokens). Overlap: 4 tokens → last 1 line carries over.
        // Chunk[1] starts with the last line of chunk[0].
        let lines: Vec<String> = (1..=10)
            .map(|i| format!("word1_{i} word2_{i} word3_{i} word4_{i}"))
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let span = make_span(&line_refs);

        let svc = make_service(20, 4);
        let chunks = svc
            .split_large_span("test.rs", span)
            .expect("split should succeed");

        // Must produce more than one chunk (content is 50 tokens, hard=20).
        assert!(
            chunks.len() > 1,
            "expected multiple chunks, got {}",
            chunks.len()
        );

        // For every consecutive pair, the first line of chunk[i+1] must appear
        // as the last line of chunk[i].
        for window in chunks.windows(2) {
            let prev_last_line = window[0]
                .content
                .lines()
                .last()
                .expect("chunk must have content");
            let next_first_line = window[1]
                .content
                .lines()
                .next()
                .expect("chunk must have content");

            assert_eq!(
                prev_last_line, next_first_line,
                "overlap missing: last line of prev chunk should equal first line of next chunk"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // Test 2: overlap token budget is never exceeded
    // ---------------------------------------------------------------------------
    #[test]
    fn test_overlap_token_count_respected() {
        // 10 lines × 4 words each = 40 tokens total.
        // hard=20, soft=18, overlap=6.
        // Each line is 4 tokens. The overlap budget of 6 means at most 1 line
        // can be carried (4 ≤ 6, but 4+4=8 > 6). Overlap tokens per chunk
        // boundary must never exceed 6.
        let lines: Vec<String> = (1..=10)
            .map(|i| format!("word1_{i} word2_{i} word3_{i} word4_{i}"))
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let span = make_span(&line_refs);

        let svc = make_service(20, 6);
        let chunks = svc
            .split_large_span("test.rs", span)
            .expect("split should succeed");

        assert!(chunks.len() > 1, "expected multiple chunks");

        let counter = WordCounter;
        for window in chunks.windows(2) {
            // Collect overlap lines: lines from chunk[i+1] that also appear at
            // the end of chunk[i].
            let prev_lines: Vec<&str> = window[0].content.lines().collect();
            let next_lines: Vec<&str> = window[1].content.lines().collect();

            // Walk from the start of next chunk and count how many leading lines
            // match the tail of the previous chunk.
            let overlap_line_count = next_lines
                .iter()
                .zip(prev_lines.iter().rev())
                .take_while(|(n, p)| n == p)
                .count();

            let overlap_text = next_lines[..overlap_line_count].join("\n");
            let overlap_tokens = counter.count(&overlap_text);

            assert!(
                overlap_tokens <= 6,
                "overlap tokens {overlap_tokens} exceeded budget of 6"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // Test 3: content that fits in one chunk produces no split
    // ---------------------------------------------------------------------------
    #[test]
    fn test_single_chunk_no_overlap_needed() {
        // 3 lines × 2 words each = 6 tokens. hard=20 → fits comfortably.
        let lines = ["hello world", "foo bar", "baz qux"];
        let span = make_span(&lines);

        let svc = make_service(20, 4);
        let chunks = svc
            .split_large_span("test.rs", span)
            .expect("split should succeed");

        assert_eq!(chunks.len(), 1, "expected a single chunk");
    }

    // ---------------------------------------------------------------------------
    // Test 4: overlap=0 produces independent (non-overlapping) chunks
    // ---------------------------------------------------------------------------
    #[test]
    fn test_zero_overlap_no_change() {
        // 10 lines × 4 words each = 40 tokens. hard=20, overlap=0.
        // No line from chunk[i] should appear in chunk[i+1].
        let lines: Vec<String> = (1..=10)
            .map(|i| format!("word1_{i} word2_{i} word3_{i} word4_{i}"))
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let span = make_span(&line_refs);

        let svc = make_service(20, 0);
        let chunks = svc
            .split_large_span("test.rs", span)
            .expect("split should succeed");

        assert!(chunks.len() > 1, "expected multiple chunks");

        for window in chunks.windows(2) {
            let prev_last_line = window[0]
                .content
                .lines()
                .last()
                .expect("chunk must have content");
            let next_first_line = window[1]
                .content
                .lines()
                .next()
                .expect("chunk must have content");

            assert_ne!(
                prev_last_line, next_first_line,
                "zero-overlap: last line of prev chunk should NOT equal first line of next chunk"
            );
        }
    }
}
