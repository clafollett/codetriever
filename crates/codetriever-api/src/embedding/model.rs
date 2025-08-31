//! Embedding model for semantic code search.
//!
//! This module provides the core embedding functionality for Codetriever, enabling
//! semantic understanding of code through vector embeddings. The embedding strategy
//! focuses on local-first processing with no cloud dependencies.
//!
//! # Architecture
//!
//! The embedding pipeline follows this flow:
//! ```text
//! Code Text → Semantic Chunking → Vector Embeddings → Similarity Search
//! ```
//!
//! # Design Principles
//!
//! - **Local-first**: All embedding computation happens on-device using Candle
//! - **Privacy-focused**: No code ever leaves your machine
//! - **Performance-oriented**: Sub-10ms embedding for real-time search
//! - **Language-agnostic**: Works with any programming language via transformer models
use crate::Result;
use crate::embedding::jina_bert_v2::{BertModel as JinaBertModel, Config as JinaBertConfig};
use candle_core::{D, DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use hf_hub::{Repo, RepoType, api::tokio::Api};
use tokenizers::tokenizer::Tokenizer;

/// Trait for embedding models to abstract over different BERT variants
trait EmbedderModel: Send + Sync {
    fn forward(&self, token_ids: &Tensor, attention_mask: Option<&Tensor>) -> Result<Tensor>;
}

impl EmbedderModel for BertModel {
    fn forward(&self, token_ids: &Tensor, attention_mask: Option<&Tensor>) -> Result<Tensor> {
        let token_type_ids = token_ids.zeros_like().map_err(|e| {
            crate::Error::Embedding(format!("Failed to create token type ids: {e}"))
        })?;
        // Pass attention mask through to BERT
        self.forward(token_ids, &token_type_ids, attention_mask)
            .map_err(|e| crate::Error::Embedding(format!("BERT forward pass failed: {e}")))
    }
}

impl EmbedderModel for JinaBertModel {
    fn forward(&self, token_ids: &Tensor, attention_mask: Option<&Tensor>) -> Result<Tensor> {
        // Use our custom forward that handles attention mask
        self.forward(token_ids, attention_mask)
            .map_err(|e| crate::Error::Embedding(format!("JinaBERT forward pass failed: {e}")))
    }
}

/// Core embedding model for semantic code understanding.
///
// Type alias for cleaner code
type BoxedEmbedderModel = Box<dyn EmbedderModel>;

/// `EmbeddingModel` provides high-performance, local-first vector embeddings
/// for code snippets, enabling semantic search across large codebases without
/// cloud dependencies. Built on Candle for efficient on-device inference.
pub struct EmbeddingModel {
    model_id: String,
    device: Device,
    model: Option<BoxedEmbedderModel>,
    tokenizer: Option<Tokenizer>,
}

impl EmbeddingModel {
    /// Creates a new embedding model instance with the specified model identifier.
    ///
    /// # Arguments
    ///
    /// * `model_id` - The identifier for the embedding model to use (e.g., "jinaai/jina-embeddings-v2-base-code")
    ///
    /// # Returns
    ///
    /// A new `EmbeddingModel` instance ready for embedding operations.
    pub fn new(model_id: String) -> Self {
        let device = if candle_core::utils::cuda_is_available() {
            Device::new_cuda(0).unwrap_or(Device::Cpu)
        } else if candle_core::utils::metal_is_available() {
            Device::new_metal(0).unwrap_or(Device::Cpu)
        } else {
            Device::Cpu
        };

        Self {
            model_id,
            device,
            model: None,
            tokenizer: None,
        }
    }

    /// Generate embeddings for a batch of text inputs.
    ///
    /// This method converts text inputs into dense vector representations that capture
    /// semantic meaning, enabling similarity-based search and retrieval.
    ///
    /// # Arguments
    ///
    /// * `texts` - A vector of strings to generate embeddings for
    ///
    /// # Returns
    ///
    /// A vector of embeddings, where each embedding is a vector of f32 values.
    /// The dimensionality depends on the model (typically 768 for Jina models).
    pub async fn embed(&mut self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        // Check for Hugging Face token for model downloading
        if std::env::var("HF_TOKEN").is_err() && std::env::var("HUGGING_FACE_HUB_TOKEN").is_err() {
            return Err(crate::Error::Configuration(
                "HF_TOKEN or HUGGING_FACE_HUB_TOKEN environment variable required for model download".to_string(),
            ));
        }

        // Ensure model is loaded
        self.ensure_model_loaded().await?;

        // Get the model and tokenizer
        let model = self
            .model
            .as_ref()
            .ok_or_else(|| crate::Error::Embedding("Model not loaded".to_string()))?;

        let tokenizer = self
            .tokenizer
            .as_mut()
            .ok_or_else(|| crate::Error::Embedding("Tokenizer not loaded".to_string()))?;

        // Use F32 for all models for numerical stability
        let dtype = DType::F32;

        // Configure tokenizer
        use tokenizers::{PaddingParams, TruncationParams};
        tokenizer.with_padding(Some(PaddingParams {
            strategy: tokenizers::PaddingStrategy::BatchLongest,
            ..Default::default()
        }));
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: 512, // More reasonable for code snippets
                ..Default::default()
            }))
            .map_err(|e| crate::Error::Embedding(format!("Failed to set truncation: {e}")))?;

        // Encode texts
        let encodings = tokenizer
            .encode_batch(texts, true)
            .map_err(|e| crate::Error::Embedding(format!("Tokenization failed: {e}")))?;

        // Convert to tensors
        let batch_size = encodings.len();
        let max_len = encodings.iter().map(|e| e.len()).max().unwrap_or(0);

        let mut input_ids_vec = Vec::new();

        for encoding in &encodings {
            let ids = encoding.get_ids();

            // Pad to max length
            let mut padded_ids = ids.to_vec();
            while padded_ids.len() < max_len {
                padded_ids.push(0);
            }

            for id in padded_ids {
                input_ids_vec.push(id as i64);
            }
        }

        // Create tensors - input_ids stay as i64 as expected by embedding layer
        let input_ids =
            Tensor::from_vec(input_ids_vec.clone(), &[batch_size, max_len], &self.device).map_err(
                |e| crate::Error::Embedding(format!("Failed to create input tensor: {e}")),
            )?;

        // Create attention mask based on actual attention mask from tokenizer
        // The tokenizer returns proper attention masks that handle CLS/SEP tokens correctly
        // Use f16 to match model dtype for Jina models
        let attention_mask_vec: Vec<f32> = encodings
            .iter()
            .flat_map(|encoding| {
                let attention_mask = encoding.get_attention_mask();
                let mut mask = attention_mask.to_vec();
                // Pad to max length
                while mask.len() < max_len {
                    mask.push(0);
                }
                mask.iter().map(|&m| m as f32).collect::<Vec<_>>()
            })
            .collect();
        // Create the attention mask tensor with the correct dtype
        let attention_mask =
            Tensor::from_vec(attention_mask_vec, &[batch_size, max_len], &self.device)
                .and_then(|t| t.to_dtype(dtype))
                .map_err(|e| {
                    crate::Error::Embedding(format!("Failed to create attention mask: {e}"))
                })?;

        // Forward pass with attention mask
        let output = model.forward(&input_ids, Some(&attention_mask))?;

        // Jina models use MEAN POOLING with attention mask!
        // Expand attention mask to match output dimensions [batch, seq_len, hidden_dim]
        // Ensure mask is in the same dtype as output
        let mask_expanded = attention_mask
            .unsqueeze(2)
            .and_then(|m| m.broadcast_as(output.shape()))
            .and_then(|m| m.to_dtype(output.dtype()))
            .map_err(|e| crate::Error::Embedding(format!("Failed to expand mask: {e}")))?;

        // Apply mask and sum
        let masked_output = output
            .broadcast_mul(&mask_expanded)
            .map_err(|e| crate::Error::Embedding(format!("Failed to apply mask: {e}")))?;
        let sum_embeddings = masked_output
            .sum(1) // Sum over sequence dimension
            .map_err(|e| crate::Error::Embedding(format!("Failed to sum embeddings: {e}")))?;

        // Sum mask for each sequence (to get count of non-padding tokens)
        // The Python version sums across dim 1 to get [batch, hidden_dim] -> [batch, 1]
        // We need to sum the expanded mask across the sequence dimension only
        let mask_sum = mask_expanded
            .sum(1) // This gives us [batch, hidden_dim] where each value is the count of non-padding tokens
            .map_err(|e| crate::Error::Embedding(format!("Failed to sum mask: {e}")))?;

        // Avoid division by zero - clamp to minimum of 1e-9
        // mask_sum has shape [batch, hidden_dim] so we need to broadcast the minimum
        let min_val = 1e-9f32;
        let mask_sum_clamped = mask_sum
            .clamp(min_val, f32::INFINITY)
            .map_err(|e| crate::Error::Embedding(format!("Failed to clamp mask sum: {e}")))?;

        // Mean pooling: divide sum by number of non-padding tokens
        let mean_pooled = sum_embeddings
            .broadcast_div(&mask_sum_clamped)
            .map_err(|e| crate::Error::Embedding(format!("Failed to divide for mean: {e}")))?;

        // Normalize embeddings for cosine similarity
        let norms = mean_pooled
            .sqr()
            .and_then(|x| x.sum_keepdim(D::Minus1))
            .and_then(|x| x.sqrt())
            .map_err(|e| crate::Error::Embedding(format!("Failed to normalize: {e}")))?;

        let normalized = mean_pooled.broadcast_div(&norms).map_err(|e| {
            crate::Error::Embedding(format!("Failed to normalize embeddings: {e}"))
        })?;

        // Convert to Vec<Vec<f32>> - convert to F32 for output regardless of internal dtype
        let embeddings_vec = normalized
            .to_dtype(DType::F32)
            .and_then(|t| t.to_vec2::<f32>())
            .map_err(|e| crate::Error::Embedding(format!("Failed to convert to vec: {e}")))?;

        Ok(embeddings_vec)
    }

    /// Ensures the model is loaded and ready for inference.
    async fn ensure_model_loaded(&mut self) -> Result<()> {
        if self.model.is_some() {
            return Ok(());
        }

        // Initialize API
        let api = Api::new()
            .map_err(|e| crate::Error::Embedding(format!("Failed to create HF API: {e}")))?;
        let repo = api.repo(Repo::new(self.model_id.clone(), RepoType::Model));

        // Download config
        let config_path = repo
            .get("config.json")
            .await
            .map_err(|e| crate::Error::Embedding(format!("Failed to download config: {e}")))?;

        // Parse config to determine model type
        let config_str = std::fs::read_to_string(&config_path)
            .map_err(|e| crate::Error::Embedding(format!("Failed to read config: {e}")))?;

        // Check if it's a Jina model by looking for specific markers
        let is_jina = config_str.contains("\"position_embedding_type\": \"alibi\"")
            || config_str.contains("jina")
            || config_str.contains("JinaBert");

        // Download model weights
        let weights_path = match repo.get("model.safetensors").await {
            Ok(path) => path,
            Err(_) => repo.get("pytorch_model.bin").await.map_err(|e| {
                crate::Error::Embedding(format!("Failed to download model weights: {e}"))
            })?,
        };

        // Load model based on type
        let model: Box<dyn EmbedderModel> = if is_jina {
            // Parse as JinaBERT config
            let mut config: JinaBertConfig = serde_json::from_str(&config_str).map_err(|e| {
                crate::Error::Embedding(format!("Failed to parse Jina config: {e}"))
            })?;

            // Override max_position_embeddings to match our truncation
            config.max_position_embeddings = 512;

            // Load Jina model weights - convert to F32 for numerical stability
            // Even though weights are stored as F16, we use F32 for computation
            let vb = unsafe {
                VarBuilder::from_mmaped_safetensors(&[&weights_path], DType::F32, &self.device)
                    .map_err(|e| {
                        crate::Error::Embedding(format!("Failed to load Jina weights: {e}"))
                    })?
            };

            Box::new(JinaBertModel::new(vb, &config).map_err(|e| {
                crate::Error::Embedding(format!("Failed to initialize JinaBERT model: {e}"))
            })?)
        } else {
            // Standard loading for non-Jina models
            let vb = if weights_path.to_string_lossy().ends_with(".safetensors") {
                unsafe {
                    VarBuilder::from_mmaped_safetensors(&[&weights_path], DType::F32, &self.device)
                        .map_err(|e| {
                            crate::Error::Embedding(format!("Failed to load safetensors: {e}"))
                        })?
                }
            } else {
                VarBuilder::from_pth(&weights_path, DType::F32, &self.device).map_err(|e| {
                    crate::Error::Embedding(format!("Failed to load pytorch weights: {e}"))
                })?
            };

            // Parse as standard BERT config
            let config: BertConfig = serde_json::from_str(&config_str).map_err(|e| {
                crate::Error::Embedding(format!("Failed to parse BERT config: {e}"))
            })?;
            Box::new(BertModel::load(vb, &config).map_err(|e| {
                crate::Error::Embedding(format!("Failed to initialize BERT model: {e}"))
            })?)
        };

        self.model = Some(model);

        // Download and load tokenizer
        let tokenizer_path = repo
            .get("tokenizer.json")
            .await
            .map_err(|e| crate::Error::Embedding(format!("Failed to download tokenizer: {e}")))?;
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| crate::Error::Embedding(format!("Failed to load tokenizer: {e}")))?;
        self.tokenizer = Some(tokenizer);

        Ok(())
    }
}

impl Default for EmbeddingModel {
    fn default() -> Self {
        // Using CodeBERT as it has standard RoBERTa architecture that works with Candle
        // Jina model has convergence issues we're still debugging
        Self::new("microsoft/codebert-base".to_string())
    }
}
