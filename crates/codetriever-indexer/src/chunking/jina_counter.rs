//! Jina model token counter implementation

use super::traits::TokenCounter;
use crate::IndexerResult;
use std::sync::Arc;
use tokenizers::Tokenizer;

/// Token counter for Jina BERT v2 model
pub struct JinaTokenCounter {
    tokenizer: Arc<Tokenizer>,
    max_tokens: usize,
}

impl JinaTokenCounter {
    /// Create a new Jina token counter with the given tokenizer
    pub fn new(tokenizer: Arc<Tokenizer>, max_tokens: usize) -> Self {
        Self {
            tokenizer,
            max_tokens,
        }
    }

    /// Load tokenizer from the model
    pub async fn from_model_id(model_id: &str, max_tokens: usize) -> IndexerResult<Self> {
        use crate::embedding::model::EmbeddingModel;

        // Create a temporary model just to get the tokenizer
        let mut model = EmbeddingModel::new(model_id.to_string(), max_tokens);
        model.ensure_model_loaded().await?;

        let tokenizer = model
            .get_tokenizer()
            .ok_or_else(|| crate::IndexerError::Other("Failed to load tokenizer".into()))?;

        Ok(Self {
            tokenizer,
            max_tokens,
        })
    }
}

impl TokenCounter for JinaTokenCounter {
    fn name(&self) -> &str {
        "jina-bert-v2"
    }

    fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    fn count(&self, text: &str) -> usize {
        // Use the tokenizer to count tokens without truncation
        self.tokenizer
            .encode(text, false)
            .map(|encoding| encoding.len())
            .unwrap_or(0)
    }

    fn count_batch(&self, texts: &[&str]) -> Vec<usize> {
        texts.iter().map(|text| self.count(text)).collect()
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_jina_counter_basic() {
        // This test requires the model to be available
        // We'll use a mock tokenizer for unit tests
        // Real integration tests will use the actual model
    }
}
