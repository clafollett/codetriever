//! Chunking module for token-aware code splitting

pub mod heuristic_counter;
pub mod jina_counter;
pub mod registry;
pub mod service;
pub mod tiktoken_counter;
pub mod traits;

pub use heuristic_counter::HeuristicCounter;
pub use jina_counter::JinaTokenCounter;
pub use registry::TokenCounterRegistry;
pub use service::{ChunkingService, CodeSpan, TokenBudget};
pub use tiktoken_counter::TiktokenCounter;
pub use traits::TokenCounter;
