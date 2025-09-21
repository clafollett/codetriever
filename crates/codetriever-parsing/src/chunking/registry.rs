//! Token counter registry for model selection

use super::heuristic_counter::HeuristicCounter;
use super::jina_counter::JinaTokenCounter;
use super::tiktoken_counter::TiktokenCounter;
use super::traits::TokenCounterRef;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry for token counters by model ID
///
/// Supports Jina, OpenAI (via tiktoken), and heuristic fallback
pub struct TokenCounterRegistry {
    counters: HashMap<String, TokenCounterRef>,
    default_counter: TokenCounterRef,
}

impl TokenCounterRegistry {
    /// Create a new registry with all available counters
    pub async fn new(jina_tokenizer: Arc<tokenizers::Tokenizer>, max_tokens: usize) -> Self {
        let mut counters = HashMap::new();

        // Jina counter for Jina models
        let jina_counter: TokenCounterRef =
            Arc::new(JinaTokenCounter::new(jina_tokenizer, max_tokens));

        counters.insert(
            "jinaai/jina-embeddings-v2-small-en".to_string(),
            jina_counter.clone(),
        );
        counters.insert("jina-bert-v2".to_string(), jina_counter.clone());

        // GPT-4 models
        if let Ok(gpt4) = TiktokenCounter::new("gpt-4", 8192) {
            let gpt4_ref: TokenCounterRef = Arc::new(gpt4);
            counters.insert("gpt-4".to_string(), gpt4_ref.clone());
            counters.insert("gpt-4-0314".to_string(), gpt4_ref.clone());
            counters.insert("gpt-4-0613".to_string(), gpt4_ref.clone());
        }

        if let Ok(gpt4_32k) = TiktokenCounter::new("gpt-4-32k", 32768) {
            let gpt4_32k_ref: TokenCounterRef = Arc::new(gpt4_32k);
            counters.insert("gpt-4-32k".to_string(), gpt4_32k_ref.clone());
            counters.insert("gpt-4-32k-0314".to_string(), gpt4_32k_ref.clone());
            counters.insert("gpt-4-32k-0613".to_string(), gpt4_32k_ref.clone());
        }

        if let Ok(gpt4_turbo) = TiktokenCounter::new("gpt-4-turbo", 128000) {
            let gpt4_turbo_ref: TokenCounterRef = Arc::new(gpt4_turbo);
            counters.insert("gpt-4-turbo".to_string(), gpt4_turbo_ref.clone());
            counters.insert("gpt-4-turbo-preview".to_string(), gpt4_turbo_ref.clone());
            counters.insert("gpt-4-1106-preview".to_string(), gpt4_turbo_ref.clone());
            counters.insert("gpt-4-0125-preview".to_string(), gpt4_turbo_ref.clone());
        }

        if let Ok(gpt4o) = TiktokenCounter::new("gpt-4o", 128000) {
            let gpt4o_ref: TokenCounterRef = Arc::new(gpt4o);
            counters.insert("gpt-4o".to_string(), gpt4o_ref.clone());
            counters.insert("gpt-4o-2024-05-13".to_string(), gpt4o_ref.clone());
        }

        if let Ok(gpt4o_mini) = TiktokenCounter::new("gpt-4o-mini", 128000) {
            let gpt4o_mini_ref: TokenCounterRef = Arc::new(gpt4o_mini);
            counters.insert("gpt-4o-mini".to_string(), gpt4o_mini_ref.clone());
            counters.insert("gpt-4o-mini-2024-07-18".to_string(), gpt4o_mini_ref);
        }

        // GPT-5 models (Released August 2025)
        // Input: 272,000 tokens, Output: 128,000 tokens
        if let Ok(gpt5) = TiktokenCounter::new("gpt-5", 272000) {
            let gpt5_ref: TokenCounterRef = Arc::new(gpt5);
            counters.insert("gpt-5".to_string(), gpt5_ref.clone());
        }

        if let Ok(gpt5_mini) = TiktokenCounter::new("gpt-5-mini", 272000) {
            let gpt5_mini_ref: TokenCounterRef = Arc::new(gpt5_mini);
            counters.insert("gpt-5-mini".to_string(), gpt5_mini_ref.clone());
        }

        if let Ok(gpt5_nano) = TiktokenCounter::new("gpt-5-nano", 272000) {
            let gpt5_nano_ref: TokenCounterRef = Arc::new(gpt5_nano);
            counters.insert("gpt-5-nano".to_string(), gpt5_nano_ref.clone());
        }

        if let Ok(gpt5_chat) = TiktokenCounter::new("gpt-5-chat-latest", 272000) {
            let gpt5_chat_ref: TokenCounterRef = Arc::new(gpt5_chat);
            counters.insert("gpt-5-chat-latest".to_string(), gpt5_chat_ref.clone());
        }

        // GPT-3.5 models
        if let Ok(gpt35) = TiktokenCounter::new("gpt-3.5-turbo", 4096) {
            let gpt35_ref: TokenCounterRef = Arc::new(gpt35);
            counters.insert("gpt-3.5-turbo".to_string(), gpt35_ref.clone());
            counters.insert("gpt-3.5-turbo-0301".to_string(), gpt35_ref.clone());
            counters.insert("gpt-3.5-turbo-0613".to_string(), gpt35_ref);
        }

        if let Ok(gpt35_16k) = TiktokenCounter::new("gpt-3.5-turbo-16k", 16384) {
            let gpt35_16k_ref: TokenCounterRef = Arc::new(gpt35_16k);
            counters.insert("gpt-3.5-turbo-16k".to_string(), gpt35_16k_ref.clone());
            counters.insert("gpt-3.5-turbo-16k-0613".to_string(), gpt35_16k_ref);
        }

        // O1 models
        if let Ok(o1) = TiktokenCounter::new("o1-preview", 128000) {
            let o1_ref: TokenCounterRef = Arc::new(o1);
            counters.insert("o1-preview".to_string(), o1_ref.clone());
            counters.insert("o1-preview-2024-09-12".to_string(), o1_ref);
        }

        if let Ok(o1_mini) = TiktokenCounter::new("o1-mini", 128000) {
            let o1_mini_ref: TokenCounterRef = Arc::new(o1_mini);
            counters.insert("o1-mini".to_string(), o1_mini_ref.clone());
            counters.insert("o1-mini-2024-09-12".to_string(), o1_mini_ref);
        }

        // Legacy models (text-davinci, etc.)
        if let Ok(davinci) = TiktokenCounter::new("text-davinci-003", 4097) {
            let davinci_ref: TokenCounterRef = Arc::new(davinci);
            counters.insert("text-davinci-003".to_string(), davinci_ref.clone());
            counters.insert("text-davinci-002".to_string(), davinci_ref);
        }

        if let Ok(curie) = TiktokenCounter::new("text-curie-001", 2049) {
            let curie_ref: TokenCounterRef = Arc::new(curie);
            counters.insert("text-curie-001".to_string(), curie_ref);
        }

        // Code models
        if let Ok(code_davinci) = TiktokenCounter::new("code-davinci-002", 8001) {
            let code_ref: TokenCounterRef = Arc::new(code_davinci);
            counters.insert("code-davinci-002".to_string(), code_ref.clone());
            counters.insert("code-cushman-001".to_string(), code_ref);
        }

        // Heuristic fallback (fast, general purpose)
        let heuristic_counter: TokenCounterRef =
            Arc::new(HeuristicCounter::new("heuristic-fallback", 8192));

        Self {
            counters,
            default_counter: heuristic_counter, // Use heuristic as fallback
        }
    }

    /// Get a token counter for the specified model
    pub fn for_model(&self, model_id: &str) -> TokenCounterRef {
        // Try exact match first
        if let Some(counter) = self.counters.get(model_id) {
            return counter.clone();
        }

        // Try prefix matching for model variants
        for (key, counter) in &self.counters {
            if model_id.starts_with(key) || key.starts_with(model_id) {
                return counter.clone();
            }
        }

        // Fall back to heuristic counter
        self.default_counter.clone()
    }

    /// Get the default counter (Jina)
    pub fn default(&self) -> TokenCounterRef {
        self.default_counter.clone()
    }

    /// Register a new counter
    pub fn register(&mut self, model_id: String, counter: TokenCounterRef) {
        self.counters.insert(model_id, counter);
    }

    /// List all registered model IDs
    pub fn list_models(&self) -> Vec<String> {
        let mut models: Vec<String> = self.counters.keys().cloned().collect();
        models.sort();
        models
    }
}
