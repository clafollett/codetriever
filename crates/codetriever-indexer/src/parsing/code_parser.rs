//! Code parsing module for extracting meaningful code elements using tree-sitter and heuristics

use crate::IndexerResult;
use crate::parsing::languages::get_language_config;
use crate::parsing::traits::ContentParser;
use codetriever_common::CommonError;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Arc;
use tokenizers::Tokenizer;
use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIteratorMut};

/// Represents a chunk of code extracted by the parser
#[derive(Debug, Clone)]
pub struct CodeChunk {
    /// Path to the source file
    pub file_path: String,
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
    /// Optional type/kind of code chunk (e.g., "function", "class", "impl")
    pub kind: Option<String>,
    /// Language of the code
    pub language: String,
    /// Optional function/class/method name if applicable
    pub name: Option<String>,
    /// Number of tokens in this chunk (if calculated)
    pub token_count: Option<usize>,
    /// Optional embedding vector (populated during indexing)
    pub embedding: Option<Vec<f32>>,
}

// Global cache for compiled tree-sitter queries - MASSIVE performance win!
// Queries are expensive to compile but can be reused across all parse operations
type QueryCache = HashMap<(usize, String), Arc<Query>>; // (language_id, query_string) -> compiled_query
static QUERY_CACHE: Lazy<std::sync::Mutex<QueryCache>> =
    Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

/// Get a cached tree-sitter query, compiling it only once per (language, query_str) pair
fn get_cached_query(language: &Language, query_str: &str) -> IndexerResult<Arc<Query>> {
    // Create cache key using language pointer address (more reliable than language.id())
    let lang_ptr = language as *const Language as usize;
    let key = (lang_ptr, query_str.to_string());

    // Check cache first
    {
        let cache = QUERY_CACHE.lock().map_err(|_| {
            crate::IndexerError::parse_error("Query cache lock poisoned".to_string())
        })?;
        if let Some(cached_query) = cache.get(&key) {
            return Ok(cached_query.clone());
        }
    }

    // Compile query if not cached
    let query = Query::new(language, query_str)
        .map_err(|e| crate::IndexerError::parse_error(format!("Failed to compile query: {e}")))?;
    let arc_query = Arc::new(query);

    // Cache the compiled query
    {
        let mut cache = QUERY_CACHE.lock().map_err(|_| {
            crate::IndexerError::parse_error("Query cache lock poisoned".to_string())
        })?;
        cache.insert(key, arc_query.clone());
    }

    Ok(arc_query)
}

/// A code parser that uses Tree-sitter and heuristics to extract meaningful elements from source code
pub struct CodeParser {
    tokenizer: Option<Arc<Tokenizer>>, // Tokenizer for counting (no truncation)
    split_large_units: bool,
    max_tokens: usize,
    overlap_tokens: usize,
}

impl Default for CodeParser {
    fn default() -> Self {
        // Use conservative defaults: split large units, 128 token overlap
        Self::new(None, true, 512, 128)
    }
}

impl CodeParser {
    /// Creates a new CodeParser with optional tokenizer for token counting
    pub fn new(
        tokenizer: Option<Arc<Tokenizer>>,
        split_large_units: bool,
        max_tokens: usize,
        overlap_tokens: usize,
    ) -> Self {
        // Share the original tokenizer Arc - we'll handle truncation per-call
        // This avoids the expensive clone of the entire tokenizer
        let counting_tokenizer = tokenizer;

        Self {
            tokenizer: counting_tokenizer,
            split_large_units,
            max_tokens,
            overlap_tokens,
        }
    }

    /// Count tokens in a text using the tokenizer if available
    ///
    /// Uses the shared tokenizer with add_special_tokens=false for accurate counting.
    /// The tokenizer is shared via Arc, avoiding expensive clones.
    fn count_tokens(&self, text: &str) -> Option<usize> {
        self.tokenizer.as_ref().and_then(|tokenizer| {
            // Use encode with add_special_tokens=false to avoid truncation issues
            // This gives us accurate token counts without special tokens
            tokenizer
                .encode(text, false)
                .ok()
                .map(|encoding| encoding.len())
        })
    }

    /// Helper to create a CodeChunk with token counting
    fn create_chunk(
        &self,
        file_path: &str,
        content: String,
        start_line: usize,
        end_line: usize,
        kind: Option<String>,
        language: &str,
        name: Option<String>,
    ) -> CodeChunk {
        // Store content length before moving
        let content_len = content.len();
        self.create_chunk_with_byte_offsets(
            file_path,
            content, // Move instead of clone - avoid unnecessary allocation
            start_line,
            end_line,
            0, // For non-tree-sitter parsing, we don't have accurate byte offsets
            content_len,
            kind,
            language,
            name,
        )
    }

    /// Helper to create a CodeChunk with token counting and explicit byte offsets
    #[allow(clippy::too_many_arguments)]
    fn create_chunk_with_byte_offsets(
        &self,
        file_path: &str,
        content: String,
        start_line: usize,
        end_line: usize,
        byte_start: usize,
        byte_end: usize,
        kind: Option<String>,
        language: &str,
        name: Option<String>,
    ) -> CodeChunk {
        let token_count = self.count_tokens(&content);

        // Warn if chunk exceeds token limit
        if let Some(count) = token_count
            && count > self.max_tokens
        {
            log::warn!(
                "Chunk exceeds {} tokens ({count} tokens) in {file_path}: lines {start_line}-{end_line}",
                self.max_tokens
            );
        }

        CodeChunk {
            file_path: file_path.to_string(),
            content,
            start_line,
            end_line,
            byte_start,
            byte_end,
            kind,
            language: language.to_string(),
            name,
            token_count,
            embedding: None,
        }
    }

    /// Creates a parser for the given language
    fn create_parser_for_language(language: &tree_sitter::Language) -> Option<Parser> {
        let mut parser = Parser::new();
        if parser.set_language(language).is_ok() {
            Some(parser)
        } else {
            None
        }
    }

    /// Split content into token-based chunks with overlap
    fn split_by_tokens(
        &self,
        content: &str,
        file_path: &str,
        language: &str,
        kind: &str,
        name: Option<String>,
        start_line: usize,
    ) -> Option<Vec<CodeChunk>> {
        let tokenizer = self.tokenizer.as_ref()?;
        let encoding = tokenizer.encode(content, false).ok()?;
        let tokens = encoding.get_ids().to_vec();
        let total_tokens = tokens.len();

        if total_tokens <= self.max_tokens {
            return None; // No splitting needed
        }

        let mut chunks = Vec::new();
        let mut chunk_start = 0;
        let mut part_num = 1;

        // Track cumulative lines for accurate line numbers
        let original_lines: Vec<&str> = content.lines().collect();
        let total_lines = original_lines.len();

        while chunk_start < total_tokens {
            let chunk_end = (chunk_start + self.max_tokens).min(total_tokens);
            let chunk_tokens = &tokens[chunk_start..chunk_end];

            if let Ok(chunk_text) = tokenizer.decode(chunk_tokens, false) {
                // Count lines in this chunk
                let chunk_line_count = chunk_text.lines().count();

                // Rough estimate: assume tokens are distributed evenly across lines
                let estimated_start_line = start_line + (chunk_start * total_lines / total_tokens);
                let estimated_end_line = estimated_start_line + chunk_line_count;

                chunks.push(self.create_chunk(
                    file_path,
                    chunk_text,
                    estimated_start_line,
                    estimated_end_line,
                    Some(format!("{kind}_part{part_num}")),
                    language,
                    name.clone(),
                ));
                part_num += 1;
            }

            // Move to next chunk with overlap
            let next_start = chunk_end.saturating_sub(self.overlap_tokens);

            // Ensure we always make progress
            if next_start >= chunk_end || next_start <= chunk_start {
                chunk_start = chunk_end; // Force progress
            } else {
                chunk_start = next_start;
            }

            // Stop if we've processed everything
            if chunk_start >= total_tokens {
                break;
            }
        }

        Some(chunks)
    }

    /// Split content by lines (fallback when no tokenizer)
    fn split_by_lines(
        &self,
        content: &str,
        file_path: &str,
        language: &str,
        kind: &str,
        name: Option<String>,
        start_line: usize,
    ) -> Vec<CodeChunk> {
        let lines: Vec<&str> = content.lines().collect();
        let mid_point = lines.len() / 2;
        let overlap = 10.min(lines.len() / 10);

        vec![
            self.create_chunk(
                file_path,
                lines[..mid_point + overlap].join("\n"),
                start_line,
                start_line + mid_point + overlap,
                Some(format!("{kind}_part1")),
                language,
                name.clone(),
            ),
            self.create_chunk(
                file_path,
                lines[mid_point..].join("\n"),
                start_line + mid_point,
                start_line + lines.len() - 1,
                Some(format!("{kind}_part2")),
                language,
                name.clone(),
            ),
        ]
    }

    /// Smart splitting for chunks that exceed token limits
    fn split_large_chunk(
        &self,
        content: &str,
        file_path: &str,
        language: &str,
        kind: &str,
        name: Option<String>,
        start_line: usize,
    ) -> Vec<CodeChunk> {
        let mut chunks = Vec::new();

        // For classes, try to split at method boundaries
        if kind == "class" || kind == "struct" || kind == "impl" {
            // Extract class signature/header
            let lines: Vec<&str> = content.lines().collect();
            let mut class_header = Vec::new();
            let mut in_body = false;
            let mut current_method = Vec::new();
            let mut method_start_line = start_line;

            for (i, line) in lines.iter().enumerate() {
                let line_num = start_line + i;

                if !in_body {
                    class_header.push(*line);
                    // Detect start of class body (first { or first indented line)
                    if line.contains('{') || (i > 0 && line.starts_with("    ")) {
                        in_body = true;
                        method_start_line = line_num + 1;
                    }
                } else {
                    current_method.push(*line);

                    // Check if we should create a chunk
                    let current_content = format!(
                        "{}\n    // ... (continued)\n{}",
                        class_header.join("\n"),
                        current_method.join("\n")
                    );

                    if let Some(token_count) = self.count_tokens(&current_content) {
                        // Create chunk if approaching limit
                        if token_count >= self.max_tokens - self.overlap_tokens {
                            chunks.push(self.create_chunk(
                                file_path,
                                current_content,
                                method_start_line,
                                line_num,
                                Some(format!("{kind}_part")),
                                language,
                                name.clone(),
                            ));
                            current_method.clear();
                            method_start_line = line_num + 1;
                        }
                    }
                }
            }

            // Add remaining content
            if !current_method.is_empty() {
                let final_content = format!(
                    "{}\n    // ... (continued)\n{}",
                    class_header.join("\n"),
                    current_method.join("\n")
                );
                chunks.push(self.create_chunk(
                    file_path,
                    final_content,
                    method_start_line,
                    start_line + lines.len() - 1,
                    Some(format!("{kind}_part")),
                    language,
                    name.clone(),
                ));
            }
        } else {
            // For functions: try token-based splitting first, then fallback to lines
            if let Some(token_chunks) =
                self.split_by_tokens(content, file_path, language, kind, name.clone(), start_line)
            {
                return token_chunks;
            }

            // Fallback to line-based splitting
            return self.split_by_lines(
                content,
                file_path,
                language,
                kind,
                name.clone(),
                start_line,
            );
        }

        if chunks.is_empty() {
            // Fallback: return original as single chunk even if too large
            chunks.push(self.create_chunk(
                file_path,
                content.to_string(),
                start_line,
                start_line + content.lines().count(),
                Some(kind.to_string()),
                language,
                name.clone(),
            ));
        }

        chunks
    }

    /// Parses source code and extracts meaningful code chunks
    pub fn parse(
        &self,
        code: &str,
        language: &str,
        file_path: &str,
    ) -> IndexerResult<Vec<CodeChunk>> {
        // Normalize line endings to LF for consistent parsing
        // This handles files with mixed line endings (CRLF, LF, or both)
        let normalized_code = code.replace("\r\n", "\n").replace('\r', "\n");
        let code = normalized_code.as_str();

        // Get language configuration
        let config = get_language_config(language);

        // Try tree-sitter parsing if we have a language config with tree-sitter support
        if let Some(lang_config) = config
            && let Some(tree_sitter_language) = &lang_config.tree_sitter_language
            && let Some(query_str) = lang_config.tree_sitter_query
            && let Ok(chunks) = self.parse_with_tree_sitter(
                code,
                language,
                file_path,
                tree_sitter_language,
                query_str,
            )
            && !chunks.is_empty()
        {
            return Ok(chunks);
        }

        // Fall back to heuristic parsing
        self.parse_with_heuristics(code, language, file_path, config)
    }

    fn parse_with_tree_sitter(
        &self,
        code: &str,
        language: &str,
        file_path: &str,
        tree_sitter_language: &tree_sitter::Language,
        query_str: &str,
    ) -> IndexerResult<Vec<CodeChunk>> {
        // Create a parser for this language
        let mut parser = Self::create_parser_for_language(tree_sitter_language)
            .ok_or_else(|| anyhow::anyhow!("Failed to create parser for language"))?;

        let tree = parser
            .parse(code, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse code"))?;

        let root = tree.root_node();
        let mut chunks = Vec::new();

        // PERFORMANCE: Use cached queries to avoid expensive recompilation 🚀
        let query = get_cached_query(tree_sitter_language, query_str)
            .map_err(|e| anyhow::anyhow!("Failed to get cached query: {}", e))?;

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, root, code.as_bytes());

        while let Some(match_) = matches.next_mut() {
            for capture in match_.captures {
                let node = capture.node;
                let start_line = node.start_position().row + 1;
                let end_line = node.end_position().row + 1;
                let byte_range = node.byte_range();
                let content = &code[byte_range.clone()];

                // Extract name if possible
                let name = self.extract_name_from_node(&node, code);

                // Check if chunk needs splitting based on token count
                let content_str = content.to_string();
                if let Some(token_count) = self.count_tokens(&content_str) {
                    if self.split_large_units && token_count > self.max_tokens {
                        println!(
                            "📝 Splitting large {} '{}' ({} tokens > {} max) into smaller chunks",
                            node.kind(),
                            name.as_deref().unwrap_or("<unnamed>"),
                            token_count,
                            self.max_tokens
                        );
                        // Use smart splitting for large chunks
                        let split_chunks = self.split_large_chunk(
                            &content_str,
                            file_path,
                            language,
                            node.kind(),
                            name,
                            start_line,
                        );
                        chunks.extend(split_chunks);
                    } else {
                        // Create normal chunk with accurate byte offsets
                        let chunk = self.create_chunk_with_byte_offsets(
                            file_path,
                            content_str,
                            start_line,
                            end_line,
                            byte_range.start,
                            byte_range.end,
                            Some(node.kind().to_string()),
                            language,
                            name,
                        );
                        chunks.push(chunk);
                    }
                } else {
                    // If token counting fails, create chunk anyway with byte offsets
                    let chunk = self.create_chunk_with_byte_offsets(
                        file_path,
                        content_str,
                        start_line,
                        end_line,
                        byte_range.start,
                        byte_range.end,
                        Some(node.kind().to_string()),
                        language,
                        name,
                    );
                    chunks.push(chunk);
                }
            }
        }

        // If no specific constructs found, fall back to top-level items
        if chunks.is_empty() {
            self.extract_top_level_items(&root, code, file_path, language, &mut chunks);
        }

        Ok(chunks)
    }

    fn parse_with_heuristics(
        &self,
        code: &str,
        language: &str,
        file_path: &str,
        config: Option<&'static crate::parsing::languages::LanguageConfig>,
    ) -> IndexerResult<Vec<CodeChunk>> {
        let mut chunks = Vec::new();

        // Line endings are already normalized to LF in parse()
        let line_ending = "\n";

        let lines: Vec<&str> = code.lines().collect();

        if lines.is_empty() {
            return Ok(chunks);
        }

        let mut current_chunk = Vec::new();
        let mut current_start = 1;
        let mut brace_depth = 0;
        let mut indent_depth = 0;
        let mut in_function = false;
        let mut in_class = false;
        let mut current_name: Option<String> = None;

        let uses_braces = config.is_none_or(|c| c.uses_braces);
        let uses_indentation = config.is_some_and(|c| c.uses_indentation);

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let line_num = i + 1;
            let current_indent = line.len() - line.trim_start().len();

            // Detect function/class/method starts
            if self.is_definition_start(trimmed, config) {
                // Save previous chunk if it exists
                if !current_chunk.is_empty() && (in_function || in_class) {
                    let content = current_chunk.join(line_ending);
                    let chunk = self.create_chunk(
                        file_path,
                        content,
                        current_start,
                        i,
                        Some(if in_class {
                            "class".to_string()
                        } else {
                            "function".to_string()
                        }),
                        language,
                        current_name.clone(),
                    );
                    chunks.push(chunk);
                    current_chunk.clear();
                }

                current_start = line_num;
                in_function = self.is_function_start(trimmed, config);
                in_class = self.is_class_start(trimmed, config);
                current_name = self.extract_name_from_line(trimmed);
                brace_depth = 0;
                indent_depth = current_indent;
            }

            current_chunk.push(*line);

            if uses_braces {
                // Track brace depth for brace-based languages
                for ch in line.chars() {
                    match ch {
                        '{' | '(' | '[' => brace_depth += 1,
                        '}' | ')' | ']' => {
                            brace_depth -= 1;
                            // End of a definition block
                            if brace_depth == 0 && (in_function || in_class) {
                                let content = current_chunk.join(line_ending);
                                let chunk = self.create_chunk(
                                    file_path,
                                    content,
                                    current_start,
                                    line_num,
                                    Some(if in_class {
                                        "class".to_string()
                                    } else {
                                        "function".to_string()
                                    }),
                                    language,
                                    current_name.clone(),
                                );
                                chunks.push(chunk);
                                current_chunk.clear();
                                in_function = false;
                                in_class = false;
                                current_name = None;
                                current_start = line_num + 1;
                            }
                        }
                        _ => {}
                    }
                }
            } else if uses_indentation {
                // For indentation-based languages like Python
                if (in_function || in_class)
                    && current_indent <= indent_depth
                    && !trimmed.is_empty()
                {
                    // End of indented block
                    let content = current_chunk[..current_chunk.len() - 1].join(line_ending);
                    let chunk = self.create_chunk(
                        file_path,
                        content,
                        current_start,
                        i,
                        Some(if in_class {
                            "class".to_string()
                        } else {
                            "function".to_string()
                        }),
                        language,
                        current_name.clone(),
                    );
                    chunks.push(chunk);
                    current_chunk = vec![*line];
                    in_function = false;
                    in_class = false;
                    current_name = None;
                    current_start = line_num;
                }
            }

            // Fallback: chunk based on token count if we're not in a definition
            if !in_function && !in_class && !current_chunk.is_empty() {
                let current_content = current_chunk.join(line_ending);
                if let Some(token_count) = self.count_tokens(&current_content) {
                    // Create chunk if approaching token limit (leave room for more lines)
                    if token_count >= self.max_tokens - self.overlap_tokens {
                        let chunk = self.create_chunk(
                            file_path,
                            current_content,
                            current_start,
                            line_num,
                            None,
                            language,
                            None,
                        );
                        chunks.push(chunk);

                        // Calculate overlap: keep some lines for context
                        // Try to keep approximately fallback_overlap_tokens worth of content
                        let mut overlap_lines = Vec::new();
                        let mut overlap_tokens = 0;

                        // Walk backwards through current_chunk to build overlap
                        for line in current_chunk.iter().rev() {
                            if let Some(line_tokens) = self.count_tokens(line) {
                                if overlap_tokens + line_tokens > self.overlap_tokens {
                                    break;
                                }
                                overlap_tokens += line_tokens;
                                overlap_lines.insert(0, *line);
                            }
                        }

                        // Start next chunk with overlap
                        current_chunk = overlap_lines;
                        current_start = line_num - current_chunk.len() + 1;
                    }
                }
            }
        }

        // Add remaining chunk
        if !current_chunk.is_empty() {
            let content = current_chunk.join(line_ending);
            let kind = if in_function {
                Some("function".to_string())
            } else if in_class {
                Some("class".to_string())
            } else {
                None
            };
            let name = if in_function || in_class {
                current_name
            } else {
                None
            };
            let chunk = self.create_chunk(
                file_path,
                content,
                current_start,
                lines.len(),
                kind,
                language,
                name,
            );
            chunks.push(chunk);
        }

        Ok(chunks)
    }

    fn is_definition_start(
        &self,
        line: &str,
        config: Option<&'static crate::parsing::languages::LanguageConfig>,
    ) -> bool {
        self.is_function_start(line, config) || self.is_class_start(line, config)
    }

    fn is_function_start(
        &self,
        line: &str,
        config: Option<&'static crate::parsing::languages::LanguageConfig>,
    ) -> bool {
        if let Some(cfg) = config {
            cfg.function_keywords
                .iter()
                .any(|&keyword| line.starts_with(keyword))
        } else {
            // Fallback patterns
            let patterns = [
                "fn ",
                "def ",
                "function ",
                "func ",
                "public ",
                "private ",
                "protected ",
                "async fn",
                "async function",
                "async def",
                "pub fn",
                "pub(crate) fn",
            ];
            patterns.iter().any(|p| line.starts_with(p))
        }
    }

    fn is_class_start(
        &self,
        line: &str,
        config: Option<&'static crate::parsing::languages::LanguageConfig>,
    ) -> bool {
        if let Some(cfg) = config {
            cfg.class_keywords
                .iter()
                .any(|&keyword| line.starts_with(keyword))
        } else {
            // Fallback patterns
            let patterns = [
                "class ",
                "struct ",
                "enum ",
                "interface ",
                "impl ",
                "trait ",
                "type ",
            ];
            patterns.iter().any(|p| line.starts_with(p))
        }
    }

    fn extract_name_from_node(&self, node: &tree_sitter::Node, code: &str) -> Option<String> {
        // Try to find identifier/name nodes
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            if matches!(child.kind(), "identifier" | "name" | "property_identifier") {
                return Some(code[child.byte_range()].to_string());
            }
        }

        None
    }

    fn extract_name_from_line(&self, line: &str) -> Option<String> {
        // Simple regex-like extraction for common patterns
        let tokens: Vec<&str> = line.split_whitespace().collect();

        // Look for patterns like "def function_name(" or "class ClassName:"
        for (i, token) in tokens.iter().enumerate() {
            if matches!(
                *token,
                "def"
                    | "fn"
                    | "function"
                    | "func"
                    | "class"
                    | "struct"
                    | "interface"
                    | "impl"
                    | "trait"
            ) && let Some(next) = tokens.get(i + 1)
            {
                // Clean up the name (remove parentheses, colons, etc.)
                let name = next
                    .trim_end_matches('(')
                    .trim_end_matches(':')
                    .trim_end_matches('{')
                    .trim_end_matches('<');
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }

        None
    }

    fn extract_top_level_items(
        &self,
        root: &tree_sitter::Node,
        code: &str,
        file_path: &str,
        language: &str,
        chunks: &mut Vec<CodeChunk>,
    ) {
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
            // Skip trivial nodes
            if child.kind() == "comment" || child.byte_range().len() < 10 {
                continue;
            }

            let start_line = child.start_position().row + 1;
            let end_line = child.end_position().row + 1;
            let byte_range = child.byte_range();
            let content = &code[byte_range.clone()];

            let chunk = self.create_chunk_with_byte_offsets(
                file_path,
                content.to_string(),
                start_line,
                end_line,
                byte_range.start,
                byte_range.end,
                Some(child.kind().to_string()),
                language,
                self.extract_name_from_node(&child, code),
            );
            chunks.push(chunk);
        }
    }
}

impl ContentParser for CodeParser {
    fn name(&self) -> &str {
        "tree-sitter-parser"
    }

    fn parse(
        &self,
        content: &str,
        language: &str,
        file_path: &str,
    ) -> IndexerResult<Vec<CodeChunk>> {
        // Delegate to the existing parse method
        CodeParser::parse(self, content, language, file_path)
    }

    fn supports_language(&self, language: &str) -> bool {
        // Check if we have a language config for this
        let config = get_language_config(language);
        config.is_some()
    }

    fn supported_languages(&self) -> Vec<&str> {
        vec![
            "rust",
            "python",
            "javascript",
            "typescript",
            "go",
            "c",
            "cpp",
            "java",
            "csharp",
            "ruby",
            "php",
            "swift",
            "kotlin",
            "scala",
            "haskell",
            "elixir",
            "json",
            "xml",
            "yaml",
            "toml",
            "html",
            "css",
            "sql",
            "bash",
            "powershell",
            "dockerfile",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_initialization() {
        let _parser = CodeParser::new(None, true, 512, 128);
        // Parser should be created successfully
        // (parsers are now created on-demand, not stored)
    }

    #[test]
    fn test_rust_parsing() {
        let parser = CodeParser::new(None, true, 512, 128);
        let code = r#"
fn main() {
    println!("Hello, world!");
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}

struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}
"#;

        let chunks = parser.parse(code, "rust", "test.rs").unwrap();
        assert!(!chunks.is_empty());

        // Should extract functions, structs, and impls
        let function_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.kind.as_ref().is_some_and(|k| k.contains("function")))
            .collect();
        assert!(function_chunks.len() >= 2); // main and add

        // Check that we extracted names
        let main_chunk = chunks
            .iter()
            .find(|c| c.name.as_ref().is_some_and(|n| n == "main"));
        assert!(main_chunk.is_some());
    }

    #[test]
    fn test_python_parsing() {
        let parser = CodeParser::new(None, true, 512, 128);
        let code = r#"
def hello():
    print("Hello, world!")

class MyClass:
    def __init__(self):
        self.value = 42
    
    def get_value(self):
        return self.value

async def async_function():
    await some_operation()
"#;

        let chunks = parser.parse(code, "python", "test.py").unwrap();
        assert!(!chunks.is_empty());

        // Should handle indentation-based parsing
        let class_chunks: Vec<_> = chunks
            .iter()
            .filter(|c| c.kind.as_ref().is_some_and(|k| k.contains("class")))
            .collect();
        assert!(!class_chunks.is_empty());
    }

    #[test]
    fn test_heuristic_fallback() {
        let parser = CodeParser::new(None, true, 512, 128);
        // Use a language without tree-sitter support or malformed code
        let code = r#"
function test() {
    // Some code
}

class Example {
    method() {
        return 42;
    }
}
"#;

        // Even without perfect parsing, should extract something
        let chunks = parser.parse(code, "unknown", "test.txt").unwrap();
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_chunk_metadata() {
        let parser = CodeParser::new(None, true, 512, 128);
        let code = "fn test() {\n    println!(\"test\");\n}";

        let chunks = parser.parse(code, "rust", "/path/to/file.rs").unwrap();
        assert!(!chunks.is_empty());

        let chunk = &chunks[0];
        assert_eq!(chunk.file_path, "/path/to/file.rs");
        assert_eq!(chunk.language, "rust");
        assert!(chunk.start_line > 0);
        assert!(chunk.end_line >= chunk.start_line);
        assert!(chunk.content.contains("fn test"));
    }
}
