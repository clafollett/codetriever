//! Tiktoken-based token counter for OpenAI models

use super::traits::TokenCounter;
use anyhow::Result;
use tiktoken_rs::{CoreBPE, cl100k_base, o200k_base, p50k_base, p50k_edit, r50k_base};

/// Token counter using tiktoken for OpenAI models
pub struct TiktokenCounter {
    /// Model name for identification
    model_name: String,
    /// The tiktoken encoder
    encoder: CoreBPE,
    /// Maximum tokens this model supports
    max_tokens: usize,
}

impl TiktokenCounter {
    /// Create a new tiktoken counter for the specified model
    pub fn new(model_name: &str, max_tokens: usize) -> Result<Self> {
        let encoder = Self::get_encoder_for_model(model_name)?;

        Ok(Self {
            model_name: model_name.to_string(),
            encoder,
            max_tokens,
        })
    }

    /// Get the appropriate encoder for a model name
    fn get_encoder_for_model(model_name: &str) -> Result<CoreBPE> {
        // Match common model patterns
        let encoder = match model_name {
            // GPT-4 and GPT-3.5-turbo use cl100k_base
            name if name.starts_with("gpt-4") || name.starts_with("gpt-3.5") => cl100k_base()?,
            // O1 models use o200k_base
            name if name.starts_with("o1") => o200k_base()?,
            // Older GPT-3 models
            name if name.starts_with("text-davinci") || name.starts_with("text-curie") => {
                p50k_base()?
            }
            // Code models
            name if name.starts_with("code-") => p50k_base()?,
            // Edit models
            name if name.contains("-edit") => p50k_edit()?,
            // Legacy models
            name if name.starts_with("davinci") || name.starts_with("curie") => r50k_base()?,
            // Default to cl100k_base for unknown models
            _ => cl100k_base()?,
        };

        Ok(encoder)
    }

    /// Common model presets with their token limits
    pub fn gpt4() -> Result<Self> {
        Self::new("gpt-4", 8192)
    }

    pub fn gpt4_turbo() -> Result<Self> {
        Self::new("gpt-4-turbo", 128000)
    }

    pub fn gpt35_turbo() -> Result<Self> {
        Self::new("gpt-3.5-turbo", 16384)
    }

    pub fn o1_mini() -> Result<Self> {
        Self::new("o1-mini", 128000)
    }

    pub fn o1_preview() -> Result<Self> {
        Self::new("o1-preview", 128000)
    }
}

impl TokenCounter for TiktokenCounter {
    fn name(&self) -> &str {
        &self.model_name
    }

    fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    fn count(&self, text: &str) -> usize {
        self.encoder.encode_ordinary(text).len()
    }

    fn count_batch(&self, texts: &[&str]) -> Vec<usize> {
        texts
            .iter()
            .map(|text| self.encoder.encode_ordinary(text).len())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tiktoken_counter_creation() {
        let counter = TiktokenCounter::gpt4().expect("Should create GPT-4 counter");
        assert_eq!(counter.name(), "gpt-4");
        assert_eq!(counter.max_tokens(), 8192);
    }

    #[test]
    fn test_token_counting() {
        let counter = TiktokenCounter::gpt4().expect("Should create counter");

        // Simple text
        let count = counter.count("Hello, world!");
        assert!(count > 0, "Should count tokens");

        // Known token count for GPT-4 (cl100k_base)
        // "Hello" is typically 1 token, "," is 1, " world" is 1, "!" is 1
        assert!(count <= 5, "Simple text should be ~4 tokens");
    }

    #[test]
    fn test_batch_counting() {
        let counter = TiktokenCounter::gpt4().expect("Should create counter");

        let texts = vec!["Hello", "World", "Test"];
        let counts = counter.count_batch(&texts);

        assert_eq!(counts.len(), 3);
        assert!(counts.iter().all(|&c| c > 0));
    }

    #[test]
    fn test_different_models() {
        let gpt4 = TiktokenCounter::gpt4().expect("Should create GPT-4");
        let gpt35 = TiktokenCounter::gpt35_turbo().expect("Should create GPT-3.5");
        let o1 = TiktokenCounter::o1_mini().expect("Should create O1");

        assert_eq!(gpt4.max_tokens(), 8192);
        assert_eq!(gpt35.max_tokens(), 16384);
        assert_eq!(o1.max_tokens(), 128000);
    }

    #[test]
    fn test_encoder_selection() {
        // Test that different model patterns get appropriate encoders
        let models = vec![
            ("gpt-4-0314", 8192),
            ("gpt-3.5-turbo-16k", 16384),
            ("text-davinci-003", 4097),
            ("code-davinci-002", 8001),
            ("o1-preview", 128000),
        ];

        for (model, max_tokens) in models {
            let counter = TiktokenCounter::new(model, max_tokens)
                .unwrap_or_else(|_| panic!("Should create counter for {model}"));
            assert_eq!(counter.name(), model);
            assert_eq!(counter.max_tokens(), max_tokens);
        }
    }

    #[test]
    fn test_empty_text() {
        let counter = TiktokenCounter::gpt4().expect("Should create counter");
        assert_eq!(counter.count(""), 0);
    }

    #[test]
    fn test_unicode_handling() {
        let counter = TiktokenCounter::gpt4().expect("Should create counter");

        let emoji_text = "Hello 👋 World 🌍";
        let count = counter.count(emoji_text);
        assert!(count > 0, "Should handle emojis");

        let chinese = "你好世界";
        let count = counter.count(chinese);
        assert!(count > 0, "Should handle Chinese characters");
    }
}
