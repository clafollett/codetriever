//! Trait definitions for content parsing

use super::code_parser::CodeChunk;
use crate::Result;

/// Trait for parsing content into semantic chunks
///
/// Implementations can use different strategies:
/// - Tree-sitter for AST-based parsing
/// - Heuristic line-based splitting
/// - ML-based semantic segmentation
pub trait ContentParser: Send + Sync {
    /// Get the name of this parser
    fn name(&self) -> &str;

    /// Parse content into semantic chunks
    ///
    /// Returns a vector of CodeChunks representing logical units
    /// like functions, classes, or other semantic boundaries
    fn parse(&self, content: &str, language: &str, file_path: &str) -> Result<Vec<CodeChunk>>;

    /// Check if this parser supports a given language
    fn supports_language(&self, language: &str) -> bool;

    /// Get list of supported languages
    fn supported_languages(&self) -> Vec<&str>;
}

// Type alias for boxed content parsers
type BoxedContentParser = Box<dyn ContentParser>;

/// Parser that delegates to language-specific implementations
pub struct CompositeParser {
    parsers: Vec<BoxedContentParser>,
    fallback: BoxedContentParser,
}

impl CompositeParser {
    /// Create a new composite parser with a fallback
    pub fn new(fallback: Box<dyn ContentParser>) -> Self {
        Self {
            parsers: Vec::new(),
            fallback,
        }
    }

    /// Register a language-specific parser
    pub fn register(&mut self, parser: Box<dyn ContentParser>) {
        self.parsers.push(parser);
    }

    /// Find the best parser for a language
    fn find_parser(&self, language: &str) -> &dyn ContentParser {
        self.parsers
            .iter()
            .find(|p| p.supports_language(language))
            .map(|p| p.as_ref())
            .unwrap_or(self.fallback.as_ref())
    }
}

impl ContentParser for CompositeParser {
    fn name(&self) -> &str {
        "composite-parser"
    }

    fn parse(&self, content: &str, language: &str, file_path: &str) -> Result<Vec<CodeChunk>> {
        let parser = self.find_parser(language);
        parser.parse(content, language, file_path)
    }

    fn supports_language(&self, language: &str) -> bool {
        self.parsers.iter().any(|p| p.supports_language(language))
    }

    fn supported_languages(&self) -> Vec<&str> {
        self.parsers
            .iter()
            .flat_map(|p| p.supported_languages())
            .collect()
    }
}
