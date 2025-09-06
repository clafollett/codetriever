pub mod jina_bert_v2;
pub mod model;
pub mod service;
pub mod traits;

pub use model::EmbeddingModel;
pub use service::{DefaultEmbeddingProvider, DefaultEmbeddingService};
pub use traits::{EmbeddingConfig, EmbeddingProvider, EmbeddingService, EmbeddingStats};
