//! Chunking service for token-aware text splitting

use super::traits::TokenCounterRef;
use crate::Result;
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
    pub fn chunk_spans(&self, file_path: &str, spans: Vec<CodeSpan>) -> Result<Vec<CodeChunk>> {
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
    fn split_large_span(&self, file_path: &str, span: CodeSpan) -> Result<Vec<CodeChunk>> {
        let mut chunks = Vec::new();
        let lines: Vec<&str> = span.content.lines().collect();

        let mut current_lines: Vec<String> = Vec::new();
        let mut current_tokens = 0;
        let mut current_start_line = span.start_line;
        let mut current_byte_offset = span.byte_start;

        for (i, line) in lines.iter().enumerate() {
            let line_with_newline = format!("{line}\n");
            let line_tokens = self.counter.count(&line_with_newline);

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

                // Reset for next chunk
                current_lines.clear();
                current_tokens = 0;
                current_start_line = span.start_line + i;
                current_byte_offset += byte_len;
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
