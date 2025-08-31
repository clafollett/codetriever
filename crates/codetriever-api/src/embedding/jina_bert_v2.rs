//! # JinaBERT V2 inference implementation  
//!
//! Updated implementation that matches the actual Jina Embeddings V2 model architecture
//! from HuggingFace, fixing weight naming mismatches in the original implementation.
//!
//! See: [Jina Embeddings V2 Base Code](https://huggingface.co/jinaai/jina-embeddings-v2-base-code)

// Import from candle crates directly
use candle_core::{D, DType, Device, IndexOp, Result, Tensor};
use candle_nn::{Embedding, LayerNorm, Linear, Module, VarBuilder};
use serde::Deserialize;

pub const DTYPE: DType = DType::F32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionEmbeddingType {
    Absolute,
    Alibi,
}

// https://huggingface.co/jinaai/jina-bert-implementation/blob/main/configuration_bert.py
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Config {
    pub vocab_size: usize,
    pub hidden_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub intermediate_size: usize,
    pub hidden_act: candle_nn::Activation,
    pub max_position_embeddings: usize,
    pub type_vocab_size: usize,
    pub initializer_range: f64,
    pub layer_norm_eps: f64,
    pub pad_token_id: usize,
    pub position_embedding_type: PositionEmbeddingType,
}

impl Config {
    pub fn v2_base() -> Self {
        // https://huggingface.co/jinaai/jina-embeddings-v2-base-en/blob/main/config.json
        Self {
            vocab_size: 30528,
            hidden_size: 768,
            num_hidden_layers: 12,
            num_attention_heads: 12,
            intermediate_size: 3072,
            hidden_act: candle_nn::Activation::Gelu,
            max_position_embeddings: 512, // Match our truncation length
            type_vocab_size: 2,
            initializer_range: 0.02,
            layer_norm_eps: 1e-12,
            pad_token_id: 0,
            position_embedding_type: PositionEmbeddingType::Alibi,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        vocab_size: usize,
        hidden_size: usize,
        num_hidden_layers: usize,
        num_attention_heads: usize,
        intermediate_size: usize,
        hidden_act: candle_nn::Activation,
        max_position_embeddings: usize,
        type_vocab_size: usize,
        initializer_range: f64,
        layer_norm_eps: f64,
        pad_token_id: usize,
        position_embedding_type: PositionEmbeddingType,
    ) -> Self {
        Config {
            vocab_size,
            hidden_size,
            num_hidden_layers,
            num_attention_heads,
            intermediate_size,
            hidden_act,
            max_position_embeddings,
            type_vocab_size,
            initializer_range,
            layer_norm_eps,
            pad_token_id,
            position_embedding_type,
        }
    }
}

#[derive(Clone, Debug)]
struct BertEmbeddings {
    word_embeddings: Embedding,
    // no position_embeddings as we only support alibi.
    token_type_embeddings: Embedding,
    layer_norm: LayerNorm,
    span: tracing::Span,
}

impl BertEmbeddings {
    fn new(vb: VarBuilder, cfg: &Config) -> Result<Self> {
        let word_embeddings =
            candle_nn::embedding(cfg.vocab_size, cfg.hidden_size, vb.pp("word_embeddings"))?;
        let token_type_embeddings = candle_nn::embedding(
            cfg.type_vocab_size,
            cfg.hidden_size,
            vb.pp("token_type_embeddings"),
        )?;
        let layer_norm =
            candle_nn::layer_norm(cfg.hidden_size, cfg.layer_norm_eps, vb.pp("LayerNorm"))?;
        Ok(Self {
            word_embeddings,
            token_type_embeddings,
            layer_norm,
            span: tracing::span!(tracing::Level::TRACE, "embeddings"),
        })
    }
}

impl Module for BertEmbeddings {
    fn forward(&self, input_ids: &Tensor) -> Result<Tensor> {
        let _enter = self.span.enter();
        let (b_size, seq_len) = input_ids.dims2()?;
        let input_embeddings = self.word_embeddings.forward(input_ids)?;
        let token_type_embeddings = Tensor::zeros(seq_len, DType::U32, input_ids.device())?
            .broadcast_left(b_size)?
            .apply(&self.token_type_embeddings)?;
        let embeddings = (&input_embeddings + token_type_embeddings)?;

        let embeddings = self.layer_norm.forward(&embeddings)?;

        Ok(embeddings)
    }
}

#[derive(Clone, Debug)]
struct BertSelfAttention {
    query: Linear,
    key: Linear,
    value: Linear,
    layer_norm_q: LayerNorm,
    layer_norm_k: LayerNorm,
    num_attention_heads: usize,
    attention_head_size: usize,
    span: tracing::Span,
    span_softmax: tracing::Span,
}

impl BertSelfAttention {
    fn new(vb: VarBuilder, cfg: &Config) -> Result<Self> {
        let attention_head_size = cfg.hidden_size / cfg.num_attention_heads;
        let all_head_size = cfg.num_attention_heads * attention_head_size;
        let hidden_size = cfg.hidden_size;
        let query = candle_nn::linear(hidden_size, all_head_size, vb.pp("query"))?;
        let value = candle_nn::linear(hidden_size, all_head_size, vb.pp("value"))?;
        let key = candle_nn::linear(hidden_size, all_head_size, vb.pp("key"))?;
        // Q/K normalization layers for jina-bert-v2-qk-post-norm
        let layer_norm_q =
            candle_nn::layer_norm(hidden_size, cfg.layer_norm_eps, vb.pp("layer_norm_q"))?;
        let layer_norm_k =
            candle_nn::layer_norm(hidden_size, cfg.layer_norm_eps, vb.pp("layer_norm_k"))?;
        Ok(Self {
            query,
            key,
            value,
            layer_norm_q,
            layer_norm_k,
            num_attention_heads: cfg.num_attention_heads,
            attention_head_size,
            span: tracing::span!(tracing::Level::TRACE, "self-attn"),
            span_softmax: tracing::span!(tracing::Level::TRACE, "softmax"),
        })
    }

    fn transpose_for_scores(&self, xs: &Tensor) -> Result<Tensor> {
        let mut x_shape = xs.dims().to_vec();
        x_shape.pop();
        x_shape.push(self.num_attention_heads);
        x_shape.push(self.attention_head_size);
        xs.reshape(x_shape)?.transpose(1, 2)?.contiguous()
    }

    fn forward(
        &self,
        xs: &Tensor,
        bias: &Tensor,
        attention_mask: Option<&Tensor>,
    ) -> Result<Tensor> {
        let _enter = self.span.enter();
        // Apply linear projections
        let query_layer = self.query.forward(xs)?;
        let key_layer = self.key.forward(xs)?;
        let value_layer = self.value.forward(xs)?;

        // Apply LayerNorm to Q and K BEFORE transpose_for_scores (critical for jina-bert-v2-qk-post-norm)
        let query_layer = self.layer_norm_q.forward(&query_layer)?;
        let key_layer = self.layer_norm_k.forward(&key_layer)?;

        // Transpose for multi-head attention
        let query_layer = self.transpose_for_scores(&query_layer)?;
        let key_layer = self.transpose_for_scores(&key_layer)?;
        let value_layer = self.transpose_for_scores(&value_layer)?;

        let attention_scores = query_layer.matmul(&key_layer.t()?)?;
        let mut attention_scores = (attention_scores / (self.attention_head_size as f64).sqrt())?;

        // Apply attention mask if provided (before softmax)
        if let Some(mask) = attention_mask {
            attention_scores = attention_scores.broadcast_add(mask)?;
        }

        // Add ALiBi bias (separate from attention mask)
        let attention_scores = attention_scores.broadcast_add(bias)?;

        let attention_probs = {
            let _enter_sm = self.span_softmax.enter();
            candle_nn::ops::softmax_last_dim(&attention_scores)?
        };
        let context_layer = attention_probs.matmul(&value_layer)?;
        let context_layer = context_layer.transpose(1, 2)?.contiguous()?;
        let context_layer = context_layer.flatten_from(D::Minus2)?;
        Ok(context_layer)
    }
}

#[derive(Clone, Debug)]
struct BertSelfOutput {
    dense: Linear,
    layer_norm: LayerNorm,
    span: tracing::Span,
}

impl BertSelfOutput {
    fn new(vb: VarBuilder, cfg: &Config) -> Result<Self> {
        let dense = candle_nn::linear(cfg.hidden_size, cfg.hidden_size, vb.pp("dense"))?;
        let layer_norm =
            candle_nn::layer_norm(cfg.hidden_size, cfg.layer_norm_eps, vb.pp("LayerNorm"))?;
        Ok(Self {
            dense,
            layer_norm,
            span: tracing::span!(tracing::Level::TRACE, "self-out"),
        })
    }

    fn forward(&self, xs: &Tensor, input_tensor: &Tensor) -> Result<Tensor> {
        let _enter = self.span.enter();
        let xs = self.dense.forward(xs)?;
        self.layer_norm.forward(&(xs + input_tensor)?)
    }
}

#[derive(Clone, Debug)]
struct BertAttention {
    self_attention: BertSelfAttention,
    self_output: BertSelfOutput,
    span: tracing::Span,
}

impl BertAttention {
    fn new(vb: VarBuilder, cfg: &Config) -> Result<Self> {
        let self_attention = BertSelfAttention::new(vb.pp("self"), cfg)?;
        let self_output = BertSelfOutput::new(vb.pp("output"), cfg)?;
        Ok(Self {
            self_attention,
            self_output,
            span: tracing::span!(tracing::Level::TRACE, "attn"),
        })
    }

    fn forward(
        &self,
        xs: &Tensor,
        bias: &Tensor,
        attention_mask: Option<&Tensor>,
    ) -> Result<Tensor> {
        let _enter = self.span.enter();
        let self_outputs = self.self_attention.forward(xs, bias, attention_mask)?;
        let attention_output = self.self_output.forward(&self_outputs, xs)?;
        Ok(attention_output)
    }
}

#[derive(Clone, Debug)]
struct BertGLUMLP {
    up_gated_layer: Linear, // Renamed to match actual weights
    act: candle_nn::Activation,
    down_layer: Linear, // Renamed to match actual weights
    intermediate_size: usize,
}

impl BertGLUMLP {
    fn new(vb: VarBuilder, cfg: &Config) -> Result<Self> {
        // Updated to match actual Jina Embeddings V2 model weight names
        let up_gated_layer = candle_nn::linear_no_bias(
            cfg.hidden_size,
            cfg.intermediate_size * 2,
            vb.pp("up_gated_layer"),
        )?;
        let act = candle_nn::Activation::Gelu; // geglu
        let down_layer =
            candle_nn::linear(cfg.intermediate_size, cfg.hidden_size, vb.pp("down_layer"))?;

        Ok(Self {
            up_gated_layer,
            act,
            down_layer,
            intermediate_size: cfg.intermediate_size,
        })
    }
}

impl Module for BertGLUMLP {
    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        // According to actual model: NO residual connection in MLP itself
        let xs = xs.apply(&self.up_gated_layer)?;

        // Split into gated and non-gated parts
        // Note: Python code has them swapped - up is first half, gated is second half
        let up = xs.narrow(D::Minus1, 0, self.intermediate_size)?;
        let gated = xs.narrow(D::Minus1, self.intermediate_size, self.intermediate_size)?;

        // Apply activation to gated part and multiply with up part
        let activated = gated.apply(&self.act)?;
        let xs = (up * activated)?;

        // Apply down layer
        xs.apply(&self.down_layer)
    }
}

#[derive(Clone, Debug)]
struct BertLayer {
    attention: BertAttention,
    mlp: BertGLUMLP,
    layer_norm_1: LayerNorm,
    layer_norm_2: LayerNorm,
    span: tracing::Span,
}

impl BertLayer {
    fn new(vb: VarBuilder, cfg: &Config) -> Result<Self> {
        let attention = BertAttention::new(vb.pp("attention"), cfg)?;
        let mlp = BertGLUMLP::new(vb.pp("mlp"), cfg)?;
        let layer_norm_1 =
            candle_nn::layer_norm(cfg.hidden_size, cfg.layer_norm_eps, vb.pp("layer_norm_1"))?;
        let layer_norm_2 =
            candle_nn::layer_norm(cfg.hidden_size, cfg.layer_norm_eps, vb.pp("layer_norm_2"))?;
        Ok(Self {
            attention,
            mlp,
            layer_norm_1,
            layer_norm_2,
            span: tracing::span!(tracing::Level::TRACE, "layer"),
        })
    }

    fn forward(
        &self,
        xs: &Tensor,
        bias: &Tensor,
        attention_mask: Option<&Tensor>,
    ) -> Result<Tensor> {
        let _enter = self.span.enter();

        // Post-norm architecture from actual model:
        // residual = layer_norm_1(residual + attention_output)
        // mlp_output = mlp(residual)
        // layer_output = layer_norm_2(residual + mlp_output)

        let residual = xs;
        let attention_output = self.attention.forward(residual, bias, attention_mask)?;

        // First LayerNorm after attention
        let residual = self.layer_norm_1.forward(&(residual + attention_output)?)?;

        // MLP
        let mlp_output = self.mlp.forward(&residual)?;

        // Second LayerNorm after MLP
        let layer_output = self.layer_norm_2.forward(&(residual + mlp_output)?)?;

        Ok(layer_output)
    }
}

fn build_alibi_bias(cfg: &Config, device: &Device) -> Result<Tensor> {
    let n_heads = cfg.num_attention_heads;
    let seq_len = cfg.max_position_embeddings;
    let alibi_bias = Tensor::arange(0, seq_len as i64, device)?.to_dtype(DType::F32)?;
    let alibi_bias = {
        let a1 = alibi_bias.reshape((1, seq_len))?;
        let a2 = alibi_bias.reshape((seq_len, 1))?;
        // ALiBi uses absolute distances
        a1.broadcast_sub(&a2)?.abs()?.broadcast_left(n_heads)?
    };
    let mut n_heads2 = 1;
    while n_heads2 < n_heads {
        n_heads2 *= 2
    }
    let slopes = (1..=n_heads2)
        .map(|v| -1f32 / 2f32.powf((v * 8) as f32 / n_heads2 as f32))
        .collect::<Vec<_>>();
    let slopes = if n_heads2 == n_heads {
        slopes
    } else {
        slopes
            .iter()
            .skip(1)
            .step_by(2)
            .chain(slopes.iter().step_by(2))
            .take(n_heads)
            .cloned()
            .collect::<Vec<f32>>()
    };
    let slopes = Tensor::new(slopes, device)?.reshape((n_heads, 1, 1))?;
    // Keep ALiBi bias in F32 for precision, will convert when using
    alibi_bias.to_dtype(DType::F32)?.broadcast_mul(&slopes)
}

#[derive(Clone, Debug)]
struct BertEncoder {
    alibi: Tensor,
    layers: Vec<BertLayer>,
    span: tracing::Span,
}

impl BertEncoder {
    fn new(vb: VarBuilder, cfg: &Config) -> Result<Self> {
        if cfg.position_embedding_type != PositionEmbeddingType::Alibi {
            return Err(candle_core::Error::Msg(
                "only alibi is supported as a position-embedding-type".to_string(),
            ));
        }
        let layers = (0..cfg.num_hidden_layers)
            .map(|index| BertLayer::new(vb.pp(format!("layer.{index}")), cfg))
            .collect::<Result<Vec<_>>>()?;
        let span = tracing::span!(tracing::Level::TRACE, "encoder");
        let alibi = build_alibi_bias(cfg, vb.device())?;
        Ok(Self {
            alibi,
            layers,
            span,
        })
    }
}

impl BertEncoder {
    fn forward(&self, xs: &Tensor, attention_mask: Option<&Tensor>) -> Result<Tensor> {
        let _enter = self.span.enter();
        let batch_size = xs.dim(0)?;
        let seq_len = xs.dim(1)?;

        // Get alibi bias and add batch dimension
        // alibi shape is [n_heads, max_seq_len, max_seq_len]
        let n_heads = self.alibi.dim(0)?;
        let alibi_bias = self
            .alibi
            .i((.., ..seq_len, ..seq_len))? // [n_heads, seq_len, seq_len]
            .unsqueeze(0)? // [1, n_heads, seq_len, seq_len]
            .broadcast_as(&[batch_size, n_heads, seq_len, seq_len])?; // [batch, n_heads, seq_len, seq_len]

        // Convert attention mask to extended format if provided
        let extended_attention_mask = if let Some(mask) = attention_mask {
            // Convert attention mask (1 for valid, 0 for padding) to attention bias
            // mask shape: [batch, seq_len] where 1 = valid token, 0 = padding
            // Need shape [batch, 1, 1, seq_len] that broadcasts to [batch, heads, seq_len, seq_len]
            let mask_expanded = mask
                .unsqueeze(1)? // [batch, 1, seq_len]
                .unsqueeze(1)?; // [batch, 1, 1, seq_len]
            // Convert: 0 (padding) -> -1e9, 1 (valid) -> 0
            Some(((1.0 - mask_expanded)? * -1e9)?)
        } else {
            None
        };

        let mut xs = xs.clone();
        for layer in self.layers.iter() {
            xs = layer.forward(&xs, &alibi_bias, extended_attention_mask.as_ref())?
        }
        Ok(xs)
    }
}

impl Module for BertEncoder {
    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        self.forward(xs, None)
    }
}

#[derive(Clone, Debug)]
pub struct BertModel {
    embeddings: BertEmbeddings,
    encoder: BertEncoder,
    pub device: Device,
    span: tracing::Span,
}

impl BertModel {
    pub fn new(vb: VarBuilder, cfg: &Config) -> Result<Self> {
        // Jina models don't have "bert." prefix in their weights
        // They use direct paths like "embeddings.word_embeddings.weight"
        let embeddings = BertEmbeddings::new(vb.pp("embeddings"), cfg)?;
        let encoder = BertEncoder::new(vb.pp("encoder"), cfg)?;
        Ok(Self {
            embeddings,
            encoder,
            device: vb.device().clone(),
            span: tracing::span!(tracing::Level::TRACE, "model"),
        })
    }
}

impl BertModel {
    pub fn forward(&self, input_ids: &Tensor, attention_mask: Option<&Tensor>) -> Result<Tensor> {
        let _enter = self.span.enter();
        let embedding_output = self.embeddings.forward(input_ids)?;

        let sequence_output = self.encoder.forward(&embedding_output, attention_mask)?;
        Ok(sequence_output)
    }
}

impl Module for BertModel {
    fn forward(&self, input_ids: &Tensor) -> Result<Tensor> {
        // For backward compatibility, but we should use the version with attention_mask
        self.forward(input_ids, None)
    }
}
