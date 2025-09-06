//! Heuristic-based token counter for fast estimation

use super::traits::TokenCounter;
use std::collections::HashMap;

/// Type alias for calibration cache
type CalibrationCache = HashMap<u64, usize>;

/// Type alias for calibration input data
type CalibrationData<'a> = &'a [(&'a str, usize)];

/// Fast heuristic token counter that estimates based on character patterns
///
/// Uses a simple but effective heuristic:
/// - Average ~4 characters per token for English text
/// - Adjusts for whitespace, punctuation, and special characters
/// - Can be calibrated with actual token counts for better accuracy
pub struct HeuristicCounter {
    name: String,
    max_tokens: usize,
    /// Characters per token ratio (default: 4.0)
    chars_per_token: f64,
    /// Calibration data: text hash -> actual token count
    calibration_data: Option<CalibrationCache>,
}

impl HeuristicCounter {
    /// Create a new heuristic counter with default settings
    pub fn new(name: &str, max_tokens: usize) -> Self {
        Self {
            name: name.to_string(),
            max_tokens,
            chars_per_token: 4.0, // Default ratio
            calibration_data: None,
        }
    }

    /// Create with a custom chars-per-token ratio
    pub fn with_ratio(name: &str, max_tokens: usize, chars_per_token: f64) -> Self {
        Self {
            name: name.to_string(),
            max_tokens,
            chars_per_token,
            calibration_data: None,
        }
    }

    /// Calibrate the counter with actual token counts
    ///
    /// This method would be called with sample texts and their actual token counts
    /// to improve the heuristic's accuracy over time
    pub fn calibrate(&mut self, texts: CalibrationData) {
        if texts.is_empty() {
            return;
        }

        // Calculate average chars per token from calibration data
        let total_chars: usize = texts.iter().map(|(text, _)| text.len()).sum();
        let total_tokens: usize = texts.iter().map(|(_, tokens)| *tokens).sum();

        if total_tokens > 0 {
            self.chars_per_token = total_chars as f64 / total_tokens as f64;
        }

        // Store calibration data for potential future use
        let mut calibration = CalibrationCache::new();
        for (text, token_count) in texts {
            let hash = Self::hash_text(text);
            calibration.insert(hash, *token_count);
        }
        self.calibration_data = Some(calibration);
    }

    /// Simple hash function for text
    fn hash_text(text: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }

    /// Estimate tokens using heuristics
    fn estimate_tokens(&self, text: &str) -> usize {
        // Check calibration cache first
        if let Some(ref calibration) = self.calibration_data {
            let hash = Self::hash_text(text);
            if let Some(&cached_count) = calibration.get(&hash) {
                return cached_count;
            }
        }

        // Count different character types for better estimation
        let mut word_chars = 0;
        let mut whitespace = 0;
        let mut punctuation = 0;
        let mut other = 0;

        for ch in text.chars() {
            if ch.is_alphabetic() || ch.is_numeric() {
                word_chars += 1;
            } else if ch.is_whitespace() {
                whitespace += 1;
            } else if ch.is_ascii_punctuation() {
                punctuation += 1;
            } else {
                other += 1;
            }
        }

        // Refined heuristic:
        // - Word characters: use the calibrated ratio
        // - Whitespace: typically part of the previous token
        // - Punctuation: often separate tokens
        // - Other (emojis, special chars): usually separate tokens

        let estimated = (word_chars as f64 / self.chars_per_token)
            + (punctuation as f64 * 0.8)  // Most punctuation becomes tokens
            + (other as f64 * 0.9); // Special chars often become tokens

        // Add a small factor for whitespace (some become tokens)
        let with_whitespace = estimated + (whitespace as f64 * 0.1);

        with_whitespace.ceil() as usize
    }
}

impl TokenCounter for HeuristicCounter {
    fn name(&self) -> &str {
        &self.name
    }

    fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    fn count(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        self.estimate_tokens(text)
    }

    fn count_batch(&self, texts: &[&str]) -> Vec<usize> {
        texts.iter().map(|text| self.count(text)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heuristic_counter_creation() {
        let counter = HeuristicCounter::new("test-heuristic", 4096);
        assert_eq!(counter.name(), "test-heuristic");
        assert_eq!(counter.max_tokens(), 4096);
        assert_eq!(counter.chars_per_token, 4.0);
    }

    #[test]
    fn test_custom_ratio() {
        let counter = HeuristicCounter::with_ratio("custom", 8192, 3.5);
        assert_eq!(counter.chars_per_token, 3.5);
    }

    #[test]
    fn test_empty_text() {
        let counter = HeuristicCounter::new("test", 4096);
        assert_eq!(counter.count(""), 0);
    }

    #[test]
    fn test_basic_estimation() {
        let counter = HeuristicCounter::new("test", 4096);

        // "Hello world" - roughly 11 chars, expect ~3 tokens
        let count = counter.count("Hello world");
        assert!((2..=4).contains(&count), "Expected 2-4 tokens, got {count}");

        // Longer text
        let long_text = "The quick brown fox jumps over the lazy dog";
        let count = counter.count(long_text);
        // 44 chars, expect ~11 tokens with default ratio
        assert!(
            (8..=14).contains(&count),
            "Expected 8-14 tokens, got {count}"
        );
    }

    #[test]
    fn test_punctuation_handling() {
        let counter = HeuristicCounter::new("test", 4096);

        // Punctuation should increase token count
        let with_punct = "Hello, world! How are you?";
        let without_punct = "Hello world How are you";

        let count_with = counter.count(with_punct);
        let count_without = counter.count(without_punct);

        assert!(
            count_with > count_without,
            "Punctuation should increase token count: {count_with} vs {count_without}"
        );
    }

    #[test]
    fn test_calibration() {
        let mut counter = HeuristicCounter::new("test", 4096);

        // Calibrate with known token counts
        let calibration_data = vec![("Hello", 1), ("Hello world", 2), ("The quick brown fox", 4)];

        counter.calibrate(&calibration_data);

        // After calibration, the ratio should be adjusted
        // Total chars: 5 + 11 + 19 = 35
        // Total tokens: 1 + 2 + 4 = 7
        // Expected ratio: 35/7 = 5.0
        assert!((counter.chars_per_token - 5.0).abs() < 0.01);

        // Calibrated texts should return exact counts
        assert_eq!(counter.count("Hello"), 1);
        assert_eq!(counter.count("Hello world"), 2);
    }

    #[test]
    fn test_unicode_handling() {
        let counter = HeuristicCounter::new("test", 4096);

        // Unicode should be handled
        let emoji_text = "Hello 👋 World 🌍";
        let count = counter.count(emoji_text);
        assert!(count > 0, "Should handle emojis");

        // Emojis typically become separate tokens
        assert!(count >= 4, "Emojis should increase token count");
    }

    #[test]
    fn test_batch_counting() {
        let counter = HeuristicCounter::new("test", 4096);

        let texts = vec!["Hello", "World", "Test"];
        let counts = counter.count_batch(&texts);

        assert_eq!(counts.len(), 3);
        assert!(counts.iter().all(|&c| c > 0));
    }
}
