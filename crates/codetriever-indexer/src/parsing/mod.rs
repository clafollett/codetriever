pub mod code_parser;
pub mod languages;

pub use code_parser::{CodeChunk, CodeParser};
pub use languages::{LanguageConfig, get_language_config, get_language_from_extension};
