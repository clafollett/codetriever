pub mod dispatcher;
pub mod jina_bert_v2;
pub mod model;
pub mod pool;
pub mod service;
pub mod traits;

pub use model::EmbeddingModel;
pub use pool::EmbeddingModelPool;
pub use service::{DefaultEmbeddingProvider, DefaultEmbeddingService};
pub use traits::{EmbeddingProvider, EmbeddingService, EmbeddingStats};
// EmbeddingConfig now comes from codetriever-config crate to eliminate duplication
