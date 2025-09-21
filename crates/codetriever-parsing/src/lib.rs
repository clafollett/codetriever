//! Codetriever parsing and chunking crate
//!
//! This crate provides code parsing using tree-sitter and intelligent chunking
//! strategies for breaking code into meaningful segments for embedding.

pub mod chunking;
pub mod error;
pub mod parsing;

// Re-export main types
pub use chunking::{ChunkingService, CodeSpan, TokenBudget, TokenCounter, TokenCounterRegistry};
pub use error::{ParsingError, ParsingResult};
pub use parsing::{CodeChunk, CodeParser, ContentParser, get_language_from_extension};
