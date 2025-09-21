//! Trait definitions for token counting

use std::sync::Arc;

/// Trait for counting tokens in text
///
/// Implementations provide model-specific token counting
/// without coupling to embedding providers
pub trait TokenCounter: Send + Sync {
    /// Get the name/identifier of this counter
    fn name(&self) -> &str;

    /// Maximum number of tokens this model can handle
    fn max_tokens(&self) -> usize;

    /// Count tokens in the given text
    ///
    /// This should be fast and deterministic for the same input
    fn count(&self, text: &str) -> usize;

    /// Count tokens for multiple texts efficiently
    fn count_batch(&self, texts: &[&str]) -> Vec<usize> {
        texts.iter().map(|text| self.count(text)).collect()
    }
}

/// Type alias for shared token counter
pub type TokenCounterRef = Arc<dyn TokenCounter>;
