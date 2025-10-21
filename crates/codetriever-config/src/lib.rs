//! Centralized configuration management for codetriever
//!
//! This crate provides a unified configuration system that eliminates duplication
//! across the codebase and provides type-safe, validated configuration with
//! support for multiple sources (environment, files, CLI, etc.).
//!
//! Configuration follows a simple hierarchy:
//! 1. Safe defaults (defined as constants)
//! 2. Environment variable overrides
//! 3. Runtime validation

pub mod error;
pub mod source;
pub mod validation;

pub use error::{ConfigError, ConfigResult};

// =============================================================================
// SAFE DEFAULTS - Work for any environment (dev, staging, prod, test)
// =============================================================================

// Embedding Model Configuration
const DEFAULT_EMBEDDING_MODEL_ID: &str = "jinaai/jina-embeddings-v2-base-code";
const DEFAULT_EMBEDDING_MODEL_DIMENSIONS: usize = 768; // JinaBERT v2 standard
const DEFAULT_EMBEDDING_MODEL_MAX_CONTEXT_TOKENS: usize = 512; // Conservative for memory
const DEFAULT_EMBEDDING_MODEL_POOL_SIZE: usize = 8; // Minimum for parallelism

// Performance Configuration
const DEFAULT_EMBEDDING_INDEXER_CHUNK_BATCH_SIZE: usize = 1; // Balance memory/speed for indexing (GPU)
const DEFAULT_EMBEDDING_SEARCH_CHUNK_BATCH_SIZE: usize = 1; // Typical concurrent API users
const DEFAULT_EMBEDDING_BATCH_TIMEOUT_MS: u64 = 1; // Low latency
const DEFAULT_EMBEDDING_USE_GPU: bool = true; // Use GPU if available

// Tokenizer Configuration
const DEFAULT_TOKENIZER_CONCURRENT_FILE_LIMIT: usize = 4; // Reasonable parallelism
const DEFAULT_TOKENIZER_MAX_CHUNK_TOKENS: usize = 512; // Matches model max_tokens
const DEFAULT_TOKENIZER_SPLIT_LARGE_UNITS: bool = true; // Always split large functions
const DEFAULT_CHUNK_QUEUE_CAPACITY: usize = 1000; // Bounded queue for back pressure
const DEFAULT_USE_PERSISTENT_QUEUE: bool = true; // PostgreSQL queue for persistence and crash recovery

// Database Configuration (safe local defaults)
const DEFAULT_DB_HOST: &str = "localhost";
const DEFAULT_DB_PORT: u16 = 5432;
const DEFAULT_DB_NAME: &str = "codetriever";
const DEFAULT_DB_USER: &str = "codetriever";
const DEFAULT_DB_PASSWORD: &str = "localdev123";
const DEFAULT_DB_SSL_MODE: &str = "disable";
const DEFAULT_DB_MAX_CONNECTIONS: u32 = 5; // Conservative
const DEFAULT_DB_MIN_CONNECTIONS: u32 = 2; // Keep some warm
const DEFAULT_DB_TIMEOUT_SECONDS: u64 = 30; // Reasonable timeout
const DEFAULT_DB_IDLE_TIMEOUT_SECONDS: u64 = 300; // 5 minutes
const DEFAULT_AUTO_MIGRATE: bool = true; // Auto-migrate by default

// Vector Storage Configuration
const DEFAULT_QDRANT_URL: &str = "http://localhost:6334";
const DEFAULT_VECTOR_DIMENSION: usize = 768; // Matches JinaBERT
const DEFAULT_VECTOR_TIMEOUT_SECONDS: u64 = 30;

// API Server Configuration
const DEFAULT_API_HOST: &str = "127.0.0.1"; // Localhost only for security
const DEFAULT_API_PORT: u16 = 3000;
const DEFAULT_API_TIMEOUT_SECONDS: u64 = 60;
const DEFAULT_API_ENABLE_CORS: bool = true;
const DEFAULT_API_ENABLE_DOCS: bool = true;

// Telemetry Configuration
const DEFAULT_TELEMETRY_ENABLED: bool = false; // Opt-in
const DEFAULT_TRACING_LEVEL: &str = "info";
const DEFAULT_TRACE_SAMPLE_RATE: f64 = 0.1; // Light sampling
const DEFAULT_TELEMETRY_SERVICE_NAME: &str = "codetriever";
const DEFAULT_TELEMETRY_ENVIRONMENT: &str = "development";

// Database imports for PostgreSQL functionality
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
};
use std::time::Duration;

/// Core configuration for the entire codetriever application
///
/// All settings have safe defaults and can be overridden via environment variables.
/// No profile/environment selection needed - same defaults work everywhere.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApplicationConfig {
    /// Embedding generation configuration
    pub embedding: EmbeddingConfig,

    /// Indexing service configuration
    pub indexing: IndexingConfig,

    /// Vector storage configuration
    pub vector_storage: VectorStorageConfig,

    /// Database configuration
    pub database: DatabaseConfig,

    /// API server configuration
    pub api: ApiConfig,

    /// Telemetry and observability configuration
    pub telemetry: TelemetryConfig,
}

/// Embedding configuration - consolidated from multiple sources
/// Follows architect's specification with nested structure for better organization
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddingConfig {
    /// Embedding provider configuration (local vs remote)
    pub provider: EmbeddingProvider,

    /// Model configuration and specifications
    pub model: ModelConfig,

    /// Performance and resource configuration
    pub performance: PerformanceConfig,

    /// Cache configuration for model storage
    pub cache: CacheConfig,
}

/// Embedding provider type - defines where embeddings are generated
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum EmbeddingProvider {
    /// Local model provider using Candle framework with GPU acceleration
    #[serde(rename = "local")]
    Local,

    /// Remote API provider (`HuggingFace`, `OpenAI`, etc.) for cloud-based inference
    #[serde(rename = "remote")]
    Remote,
}

impl Default for EmbeddingProvider {
    fn default() -> Self {
        Self::Local
    }
}

/// Model configuration and specifications - defines the ML model to use
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelConfig {
    /// Model identifier (e.g., "jinaai/jina-embeddings-v2-base-code")
    /// This determines which pre-trained model is loaded for embedding generation
    pub id: String,

    /// Maximum tokens the model can process in a single input
    /// Inputs longer than this will be truncated or chunked
    pub max_tokens: usize,

    /// Embedding dimensions produced by this model
    /// Must match vector storage configuration for consistency
    pub dimensions: usize,

    /// Model's actual maximum position embeddings from `HuggingFace` config.json
    /// This is the authoritative ceiling read from the model's config
    /// User's `max_tokens` MUST be ≤ this value
    #[serde(default)]
    pub max_position_embeddings: Option<usize>,

    /// Model capabilities and constraints for validation
    #[serde(default)]
    pub capabilities: ModelCapabilities,
}

/// Performance and resource configuration - controls runtime behavior
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PerformanceConfig {
    /// Batch size for indexing operations - controls GPU memory usage during model forward pass
    /// Larger batches = better GPU utilization but more memory
    /// This is the PRIMARY batching config for indexing workflows
    pub indexer_batch_size: usize,

    /// Batch size for search API operations - controls concurrent user query batching
    /// Only relevant for multi-user API scenarios
    /// Smaller than indexer batch since search handles 1-5 queries at a time
    pub search_batch_size: usize,

    /// Number of embedding model instances in the pool
    /// More instances allow parallel inference but use more memory
    /// Recommended: 2-4 for production, 1 for development
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,

    /// Maximum milliseconds to wait when collecting requests into a batch
    /// Lower = better latency, Higher = better throughput
    #[serde(default = "default_batch_timeout_ms")]
    pub batch_timeout_ms: u64,

    /// Whether to use GPU acceleration if available (Metal/CUDA)
    /// Significantly improves performance for large models
    pub use_gpu: bool,

    /// Specific GPU device to use (e.g., "cuda:0", "mps", "metal")
    /// Allows targeting specific GPUs in multi-GPU systems
    #[serde(default)]
    pub gpu_device: Option<String>,

    /// Memory limit in MB for model operations
    /// Prevents OOM errors by constraining model memory usage
    #[serde(default)]
    pub memory_limit_mb: Option<usize>,
}

/// Cache configuration for model storage - manages downloaded model persistence
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheConfig {
    /// Cache directory for downloaded models
    /// Models are large (GB) so caching prevents repeated downloads
    pub dir: Option<String>,

    /// Enable model caching to disk
    /// Disable for ephemeral environments or storage-constrained systems
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,

    /// Maximum cache size in MB to prevent disk exhaustion
    /// Old models are evicted when limit is reached
    #[serde(default)]
    pub max_size_mb: Option<usize>,
}

/// Model capabilities and constraints - enables advanced validation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelCapabilities {
    /// Supported input languages for this model
    /// Used to validate input text compatibility
    #[serde(default)]
    pub languages: Vec<String>,

    /// Maximum sequence length this model can handle
    /// Enables intelligent chunking strategies
    #[serde(default)]
    pub max_sequence_length: Option<usize>,

    /// Whether model is optimized for code embeddings vs natural language
    /// Code models better handle programming syntax and semantics
    #[serde(default)]
    pub code_optimized: bool,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            languages: vec!["en".to_string()],
            max_sequence_length: Some(8192),
            code_optimized: true, // Default to code-optimized for codetriever
        }
    }
}

/// Default cache enabled setting
const fn default_pool_size() -> usize {
    2
}

const fn default_batch_timeout_ms() -> u64 {
    10
}

const fn default_cache_enabled() -> bool {
    true // Enable caching by default to improve performance
}

impl EmbeddingConfig {
    /// Load configuration from environment variables with safe defaults
    pub fn from_env() -> Self {
        // Provider configuration - determines local vs remote inference
        let provider = std::env::var("CODETRIEVER_EMBEDDING_PROVIDER")
            .ok()
            .and_then(|s| match s.as_str() {
                "local" => Some(EmbeddingProvider::Local),
                "remote" => Some(EmbeddingProvider::Remote),
                _ => None,
            })
            .unwrap_or_default();

        // Model configuration with comprehensive environment override support
        let model_id = std::env::var("CODETRIEVER_EMBEDDING_MODEL")
            .unwrap_or_else(|_| DEFAULT_EMBEDDING_MODEL_ID.to_string());

        let mode_max_tokens = std::env::var("CODETRIEVER_EMBEDDING_MAX_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_EMBEDDING_MODEL_MAX_CONTEXT_TOKENS);

        let mode_dimensions = std::env::var("CODETRIEVER_EMBEDDING_DIMENSION")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_EMBEDDING_MODEL_DIMENSIONS);

        let model_pool_size = std::env::var("CODETRIEVER_EMBEDDING_MODEL_POOL_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_EMBEDDING_MODEL_POOL_SIZE);

        // Performance configuration - controls runtime resource usage and optimization
        let indexer_chunk_batch_size =
            std::env::var("CODETRIEVER_EMBEDDING_INDEXER_CHUNK_BATCH_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_EMBEDDING_INDEXER_CHUNK_BATCH_SIZE);

        let search_chunk_batch_size =
            std::env::var("CODETRIEVER_EMBEDDING_SEARCH_CHUNK_BATCH_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_EMBEDDING_SEARCH_CHUNK_BATCH_SIZE);

        let batch_timeout_ms = std::env::var("CODETRIEVER_EMBEDDING_BATCH_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_EMBEDDING_BATCH_TIMEOUT_MS);

        let use_gpu = std::env::var("CODETRIEVER_EMBEDDING_USE_GPU")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_EMBEDDING_USE_GPU);

        let gpu_device = std::env::var("CODETRIEVER_EMBEDDING_GPU_DEVICE").ok();

        let memory_limit_mb = std::env::var("CODETRIEVER_EMBEDDING_MEMORY_LIMIT_MB")
            .ok()
            .and_then(|s| s.parse().ok());

        // Cache configuration - manages model persistence and storage optimization
        let cache_dir = std::env::var("CODETRIEVER_EMBEDDING_CACHE_DIR")
            .ok()
            .or_else(|| {
                Some(
                    dirs::cache_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from(".cache"))
                        .join("codetriever")
                        .to_string_lossy()
                        .to_string(),
                )
            });

        let cache_enabled = std::env::var("CODETRIEVER_EMBEDDING_CACHE_ENABLED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true); // Enable caching by default

        let cache_max_size_mb = std::env::var("CODETRIEVER_EMBEDDING_CACHE_MAX_SIZE_MB")
            .ok()
            .and_then(|s| s.parse().ok());

        Self {
            provider,
            model: ModelConfig {
                id: model_id,
                max_tokens: mode_max_tokens,
                dimensions: mode_dimensions,
                max_position_embeddings: None, // Will be populated when model loads
                capabilities: ModelCapabilities::default(),
            },
            performance: PerformanceConfig {
                indexer_batch_size: indexer_chunk_batch_size,
                search_batch_size: search_chunk_batch_size,
                pool_size: model_pool_size,
                batch_timeout_ms,
                use_gpu,
                gpu_device,
                memory_limit_mb,
            },
            cache: CacheConfig {
                dir: cache_dir,
                enabled: cache_enabled,
                max_size_mb: cache_max_size_mb,
            },
        }
    }
}

impl validation::Validate for EmbeddingConfig {
    fn validate(&self) -> ConfigResult<()> {
        // Validate nested model configuration
        validation::validate_non_empty(&self.model.id, "model.id")?;
        validation::validate_range(self.model.max_tokens as u64, 1, 100_000, "model.max_tokens")?;
        validation::validate_range(self.model.dimensions as u64, 1, 10_000, "model.dimensions")?;

        // Validate performance configuration
        validation::validate_range(
            self.performance.indexer_batch_size as u64,
            1,
            1000,
            "performance.indexer_batch_size",
        )?;
        validation::validate_range(
            self.performance.search_batch_size as u64,
            1,
            1000,
            "performance.search_batch_size",
        )?;

        // Advanced validation as specified by architect
        if let Some(memory_limit) = self.performance.memory_limit_mb {
            let estimated_usage = self.estimate_memory_usage();
            if estimated_usage > memory_limit {
                return Err(ConfigError::Generic {
                    message: format!(
                        "Estimated memory usage ({estimated_usage} MB) exceeds limit ({memory_limit} MB)"
                    ),
                });
            }
        }

        // Validate model capabilities if specified
        if let Some(max_seq_len) = self.model.capabilities.max_sequence_length
            && max_seq_len < self.model.max_tokens
        {
            return Err(ConfigError::Generic {
                message: format!(
                    "Model max_tokens ({}) exceeds model's sequence length capability ({max_seq_len})",
                    self.model.max_tokens
                ),
            });
        }

        // Cache validation
        if self.cache.enabled
            && let Some(cache_dir) = &self.cache.dir
        {
            validation::validate_non_empty(cache_dir, "cache.dir")?;
        }

        Ok(())
    }
}

impl EmbeddingConfig {
    /// Estimate memory usage for this embedding configuration
    /// As specified by architect for memory constraint validation
    fn estimate_memory_usage(&self) -> usize {
        // Base model memory estimation based on architecture
        let model_memory = match self.model.id.as_str() {
            model if model.contains("base") => 2048, // ~2GB for base models
            model if model.contains("small") => 512, // ~512MB for small models
            model if model.contains("test") => 10,   // Minimal for test models
            _ => 1024,                               // Conservative default estimate
        };

        // Batch processing memory overhead (use indexer batch size as it's larger)
        #[allow(clippy::arithmetic_side_effects)]
        let batch_memory =
            (self.model.dimensions * self.performance.indexer_batch_size * 4) / (1024 * 1024); // f32 vectors

        #[allow(clippy::arithmetic_side_effects)]
        let total_memory = model_memory + batch_memory;
        total_memory
    }
}

/// Indexing configuration - consolidated
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexingConfig {
    /// Maximum chunk size in tokens
    pub max_chunk_tokens: usize,

    /// Whether to split large code units
    pub split_large_units: bool,

    /// Number of concurrent indexing tasks
    pub concurrency_limit: usize,

    /// Chunk queue capacity (bounded for back pressure control)
    pub chunk_queue_capacity: usize,

    /// Use PostgreSQL-backed persistent queue (true) or in-memory queue (false)
    pub use_persistent_queue: bool,
}

impl IndexingConfig {
    /// Load configuration from environment variables with safe defaults
    pub fn from_env() -> Self {
        let max_chunk_tokens = std::env::var("CODETRIEVER_INDEXING_MAX_CHUNK_TOKENS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_TOKENIZER_MAX_CHUNK_TOKENS);

        let split_large_units = std::env::var("CODETRIEVER_INDEXING_SPLIT_LARGE_UNITS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_TOKENIZER_SPLIT_LARGE_UNITS);

        let concurrency_limit = std::env::var("CODETRIEVER_INDEXING_CONCURRENCY_LIMIT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_TOKENIZER_CONCURRENT_FILE_LIMIT);

        let chunk_queue_capacity = std::env::var("CODETRIEVER_INDEXING_CHUNK_QUEUE_CAPACITY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_CHUNK_QUEUE_CAPACITY);

        let use_persistent_queue = std::env::var("CODETRIEVER_INDEXING_USE_PERSISTENT_QUEUE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_USE_PERSISTENT_QUEUE);

        Self {
            max_chunk_tokens,
            split_large_units,
            concurrency_limit,
            chunk_queue_capacity,
            use_persistent_queue,
        }
    }
}

impl validation::Validate for IndexingConfig {
    fn validate(&self) -> ConfigResult<()> {
        validation::validate_range(self.max_chunk_tokens as u64, 1, 10_000, "max_chunk_tokens")?;
        validation::validate_range(self.concurrency_limit as u64, 1, 100, "concurrency_limit")?;
        validation::validate_range(
            self.chunk_queue_capacity as u64,
            100,
            100_000,
            "chunk_queue_capacity",
        )?;
        Ok(())
    }
}

/// Vector storage configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VectorStorageConfig {
    /// Qdrant server URL
    pub url: String,

    /// Collection name
    pub collection_name: String,

    /// Vector dimensions
    pub vector_dimension: usize,

    /// Connection timeout in seconds
    pub timeout_seconds: u64,
}

impl VectorStorageConfig {
    /// Load configuration from environment variables with safe defaults
    pub fn from_env() -> Self {
        let url = std::env::var("CODETRIEVER_VECTOR_STORAGE_URL")
            .unwrap_or_else(|_| DEFAULT_QDRANT_URL.to_string());

        let collection_name = std::env::var("CODETRIEVER_VECTOR_STORAGE_COLLECTION_NAME")
            .unwrap_or_else(|_| "codetriever".to_string());

        let vector_dimension = std::env::var("CODETRIEVER_VECTOR_STORAGE_DIMENSION")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_VECTOR_DIMENSION);

        let timeout_seconds = std::env::var("CODETRIEVER_VECTOR_STORAGE_TIMEOUT_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_VECTOR_TIMEOUT_SECONDS);

        Self {
            url,
            collection_name,
            vector_dimension,
            timeout_seconds,
        }
    }
}

impl validation::Validate for VectorStorageConfig {
    fn validate(&self) -> ConfigResult<()> {
        validation::validate_url(&self.url, "url")?;
        validation::validate_non_empty(&self.collection_name, "collection_name")?;
        validation::validate_range(self.vector_dimension as u64, 1, 10_000, "vector_dimension")?;
        validation::validate_range(self.timeout_seconds, 1, 3600, "timeout_seconds")?;
        Ok(())
    }
}

/// Database configuration - comprehensive `PostgreSQL` configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatabaseConfig {
    /// Database URL (full connection string)
    pub url: String,

    /// Database host
    pub host: String,

    /// Database port
    pub port: u16,

    /// Database name
    pub database: String,

    /// Username for authentication
    pub username: String,

    /// Password for authentication (use environment variables for security)
    pub password: String,

    /// SSL mode for connections ("disable", "prefer", "require")
    pub ssl_mode: String,

    /// Maximum number of connections in pool
    pub max_connections: u32,

    /// Minimum number of connections in pool
    pub min_connections: u32,

    /// Connection timeout in seconds
    pub timeout_seconds: u64,

    /// Idle timeout in seconds
    pub idle_timeout_seconds: u64,

    /// Enable migrations on startup
    pub auto_migrate: bool,
}

impl DatabaseConfig {
    /// Load configuration from environment variables with safe defaults
    pub fn from_env() -> Self {
        let host = std::env::var("CODETRIEVER_DATABASE_HOST")
            .or_else(|_| std::env::var("DB_HOST"))
            .unwrap_or_else(|_| DEFAULT_DB_HOST.to_string());

        let port = std::env::var("CODETRIEVER_DATABASE_PORT")
            .or_else(|_| std::env::var("DB_PORT"))
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_DB_PORT);

        let database = std::env::var("CODETRIEVER_DATABASE_NAME")
            .or_else(|_| std::env::var("DB_NAME"))
            .unwrap_or_else(|_| DEFAULT_DB_NAME.to_string());

        let username = std::env::var("CODETRIEVER_DATABASE_USERNAME")
            .or_else(|_| std::env::var("DB_USER"))
            .unwrap_or_else(|_| DEFAULT_DB_USER.to_string());

        let password = std::env::var("CODETRIEVER_DATABASE_PASSWORD")
            .or_else(|_| std::env::var("DB_PASSWORD"))
            .unwrap_or_else(|_| {
                tracing::warn!(
                    "Using default database password '{}' - Set CODETRIEVER_DATABASE_PASSWORD or DB_PASSWORD environment variable. NEVER use default password in production!",
                    DEFAULT_DB_PASSWORD
                );
                DEFAULT_DB_PASSWORD.to_string()
            });

        let ssl_mode = std::env::var("CODETRIEVER_DATABASE_SSL_MODE")
            .or_else(|_| std::env::var("DB_SSLMODE"))
            .unwrap_or_else(|_| DEFAULT_DB_SSL_MODE.to_string());

        let max_connections = std::env::var("CODETRIEVER_DATABASE_MAX_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_DB_MAX_CONNECTIONS);

        let min_connections = std::env::var("CODETRIEVER_DATABASE_MIN_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_DB_MIN_CONNECTIONS);

        let timeout_seconds = std::env::var("CODETRIEVER_DATABASE_TIMEOUT_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_DB_TIMEOUT_SECONDS);

        let idle_timeout_seconds = std::env::var("CODETRIEVER_DATABASE_IDLE_TIMEOUT_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_DB_IDLE_TIMEOUT_SECONDS);

        let auto_migrate = std::env::var("CODETRIEVER_DATABASE_AUTO_MIGRATE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_AUTO_MIGRATE);

        // Construct comprehensive URL if not provided
        let url = std::env::var("CODETRIEVER_DATABASE_URL").unwrap_or_else(|_| {
            format!("postgresql://{username}:{password}@{host}:{port}/{database}")
        });

        Self {
            url,
            host,
            port,
            database,
            username,
            password,
            ssl_mode,
            max_connections,
            min_connections,
            timeout_seconds,
            idle_timeout_seconds,
            auto_migrate,
        }
    }
}

impl validation::Validate for DatabaseConfig {
    fn validate(&self) -> ConfigResult<()> {
        validation::validate_non_empty(&self.url, "url")?;
        validation::validate_range(u64::from(self.max_connections), 1, 1000, "max_connections")?;
        validation::validate_range(self.timeout_seconds, 1, 3600, "timeout_seconds")?;
        Ok(())
    }
}

impl DatabaseConfig {
    /// Convert string SSL mode to `PgSslMode`
    fn parse_ssl_mode(&self) -> PgSslMode {
        match self.ssl_mode.as_str() {
            "disable" => PgSslMode::Disable,
            "require" => PgSslMode::Require,
            _ => PgSslMode::Prefer, // Safe default for "prefer" and unknown values
        }
    }

    /// Build `PostgreSQL` connection options (no URL with password exposed!)
    /// This method creates type-safe connection options for `PostgreSQL`
    pub fn connect_options(&self) -> PgConnectOptions {
        PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .database(&self.database)
            .username(&self.username)
            .password(&self.password)
            .ssl_mode(self.parse_ssl_mode())
    }

    /// Create a `PostgreSQL` connection pool with proper configuration
    ///
    /// # Errors
    /// Returns an error if connection to database fails
    pub async fn create_pool(&self) -> Result<PgPool, sqlx::Error> {
        PgPoolOptions::new()
            .max_connections(self.max_connections)
            .min_connections(self.min_connections)
            .acquire_timeout(Duration::from_secs(self.timeout_seconds))
            .idle_timeout(Duration::from_secs(self.idle_timeout_seconds))
            .connect_with(self.connect_options())
            .await
    }

    /// Get connection info for logging (NO PASSWORD!)
    /// This method provides safe connection information for logging and debugging
    pub fn safe_connection_string(&self) -> String {
        format!(
            "{}@{}:{}/{} (ssl: {:?})",
            self.username, self.host, self.port, self.database, self.ssl_mode
        )
    }
}

/// API server configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiConfig {
    /// Server host
    pub host: String,

    /// Server port
    pub port: u16,

    /// Request timeout in seconds
    pub timeout_seconds: u64,

    /// Enable CORS
    pub enable_cors: bool,

    /// Enable OpenAPI/Swagger documentation
    pub enable_docs: bool,
}

impl ApiConfig {
    /// Load configuration from environment variables with safe defaults
    pub fn from_env() -> Self {
        let host =
            std::env::var("CODETRIEVER_API_HOST").unwrap_or_else(|_| DEFAULT_API_HOST.to_string());

        let port = std::env::var("CODETRIEVER_API_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_API_PORT);

        let timeout_seconds = std::env::var("CODETRIEVER_API_TIMEOUT_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_API_TIMEOUT_SECONDS);

        let enable_cors = std::env::var("CODETRIEVER_API_ENABLE_CORS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_API_ENABLE_CORS);

        let enable_docs = std::env::var("CODETRIEVER_API_ENABLE_DOCS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_API_ENABLE_DOCS);

        Self {
            host,
            port,
            timeout_seconds,
            enable_cors,
            enable_docs,
        }
    }
}

impl validation::Validate for ApiConfig {
    fn validate(&self) -> ConfigResult<()> {
        validation::validate_non_empty(&self.host, "host")?;
        if self.port != 0 {
            validation::validate_port(self.port, "port")?;
        }
        validation::validate_range(self.timeout_seconds, 1, 3600, "timeout_seconds")?;
        Ok(())
    }
}

/// Telemetry and observability configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TelemetryConfig {
    /// Enable telemetry collection
    pub enabled: bool,

    /// OpenTelemetry endpoint URL
    pub otlp_endpoint: Option<String>,

    /// Tracing level (trace, debug, info, warn, error)
    pub tracing_level: String,

    /// Enable metrics collection
    pub enable_metrics: bool,

    /// Metrics server port
    pub metrics_port: u16,

    /// Sample rate for traces (0.0 to 1.0)
    pub trace_sample_rate: f64,

    /// Service name for telemetry
    pub service_name: String,

    /// Environment label for telemetry
    pub environment: String,
}

impl TelemetryConfig {
    /// Load configuration from environment variables with safe defaults
    pub fn from_env() -> Self {
        let enabled = std::env::var("CODETRIEVER_TELEMETRY_ENABLED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_TELEMETRY_ENABLED);

        let otlp_endpoint = std::env::var("CODETRIEVER_TELEMETRY_OTLP_ENDPOINT").ok();

        let tracing_level = std::env::var("CODETRIEVER_TELEMETRY_TRACING_LEVEL")
            .unwrap_or_else(|_| DEFAULT_TRACING_LEVEL.to_string());

        let enable_metrics = std::env::var("CODETRIEVER_TELEMETRY_ENABLE_METRICS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_TELEMETRY_ENABLED);

        let metrics_port = std::env::var("CODETRIEVER_TELEMETRY_METRICS_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0); // Random port by default

        let trace_sample_rate = std::env::var("CODETRIEVER_TELEMETRY_TRACE_SAMPLE_RATE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_TRACE_SAMPLE_RATE);

        let service_name = std::env::var("CODETRIEVER_TELEMETRY_SERVICE_NAME")
            .unwrap_or_else(|_| DEFAULT_TELEMETRY_SERVICE_NAME.to_string());

        let environment = std::env::var("CODETRIEVER_TELEMETRY_ENVIRONMENT")
            .unwrap_or_else(|_| DEFAULT_TELEMETRY_ENVIRONMENT.to_string());

        Self {
            enabled,
            otlp_endpoint,
            tracing_level,
            enable_metrics,
            metrics_port,
            trace_sample_rate,
            service_name,
            environment,
        }
    }
}

impl validation::Validate for TelemetryConfig {
    fn validate(&self) -> ConfigResult<()> {
        validation::validate_non_empty(&self.service_name, "service_name")?;
        validation::validate_non_empty(&self.environment, "environment")?;

        if let Some(ref endpoint) = self.otlp_endpoint {
            validation::validate_url(endpoint, "otlp_endpoint")?;
        }

        if self.metrics_port != 0 {
            validation::validate_port(self.metrics_port, "metrics_port")?;
        }

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let sample_rate_scaled = (self.trace_sample_rate * 1000.0) as u64;
        validation::validate_range(sample_rate_scaled, 0, 1000, "trace_sample_rate")?;

        // Validate tracing level
        match self.tracing_level.to_lowercase().as_str() {
            "trace" | "debug" | "info" | "warn" | "error" => Ok(()),
            _ => Err(ConfigError::Generic {
                message: format!("Invalid tracing level: {}", self.tracing_level),
            }),
        }
    }
}

impl ApplicationConfig {
    /// Load configuration from environment variables with safe defaults
    ///
    /// All configuration is loaded from environment variables or falls back
    /// to safe defaults that work in any environment (dev, staging, prod, test).
    pub fn from_env() -> Self {
        Self {
            embedding: EmbeddingConfig::from_env(),
            indexing: IndexingConfig::from_env(),
            vector_storage: VectorStorageConfig::from_env(),
            database: DatabaseConfig::from_env(),
            api: ApiConfig::from_env(),
            telemetry: TelemetryConfig::from_env(),
        }
    }

    /// Estimate memory usage in MB based on configuration
    ///
    /// This method calculates the total expected memory usage by analyzing:
    /// - Base application overhead (100 MB)
    /// - Embedding model memory requirements based on model type
    /// - Vector storage memory for dimension processing and batching
    /// - Database connection pool memory (2 MB per connection)
    /// - Indexing concurrency memory (50 MB per concurrent task)
    ///
    /// The calculation helps prevent out-of-memory errors by validating
    /// against system constraints before service initialization.
    pub fn estimate_memory_usage_mb(&self) -> u64 {
        // Base memory for the application runtime (Go/Java-style base overhead)
        let base_memory = 100; // 100 MB base

        // Embedding model memory - varies significantly by model architecture
        // Base models: ~2GB (transformer layers + attention matrices)
        // Small models: ~512MB (reduced layer count)
        // Test models: ~10MB (minimal for fast testing)
        let embedding_memory = match self.embedding.model.id.as_str() {
            model if model.contains("base") => 2048, // ~2GB for base models
            model if model.contains("small") => 512, // ~512MB for small models
            model if model.contains("test") => 10,   // Minimal for test models
            _ => 1024, // Conservative default estimate for unknown models
        };

        // Vector storage memory - calculated from dimension size and batch processing
        // Each vector element is f32 (4 bytes), multiplied by dimensions and batch size
        // This represents the memory needed for vector operations during indexing
        #[allow(clippy::arithmetic_side_effects)]
        let vector_memory = (self.vector_storage.vector_dimension as u64
            * 4
            * self.embedding.performance.indexer_batch_size as u64)
            / (1024 * 1024);

        // Database connection pool memory - PostgreSQL connection overhead
        // Each connection requires ~2MB for buffers, prepared statements, etc.
        #[allow(clippy::arithmetic_side_effects)]
        let db_memory = u64::from(self.database.max_connections) * 2; // ~2MB per connection

        // Indexing concurrency memory - parallel processing overhead
        // Each concurrent indexing task needs ~50MB for file parsing,
        // chunk processing, and embedding generation queues
        #[allow(clippy::arithmetic_side_effects)]
        let indexing_memory = self.indexing.concurrency_limit as u64 * 50; // ~50MB per concurrent task

        #[allow(clippy::arithmetic_side_effects)]
        let total_memory =
            base_memory + embedding_memory + vector_memory + db_memory + indexing_memory;
        total_memory
    }
}

impl validation::Validate for ApplicationConfig {
    fn validate(&self) -> ConfigResult<()> {
        self.embedding.validate()?;
        self.indexing.validate()?;
        self.vector_storage.validate()?;
        self.database.validate()?;
        self.api.validate()?;
        self.telemetry.validate()?;

        // Cross-field validation - embedding dimension must match vector storage
        if self.embedding.model.dimensions != self.vector_storage.vector_dimension {
            return Err(ConfigError::Generic {
                message: format!(
                    "Embedding dimension ({}) must match vector storage dimension ({})",
                    self.embedding.model.dimensions, self.vector_storage.vector_dimension
                ),
            });
        }

        // Memory constraint validation
        let estimated_memory_mb = self.estimate_memory_usage_mb();
        if let Some(system_memory) = get_system_memory_mb()
            && estimated_memory_mb > system_memory.saturating_mul(80).saturating_div(100)
        {
            // Max 80% of system memory
            return Err(ConfigError::Generic {
                message: format!(
                    "Estimated memory usage ({estimated_memory_mb} MB) exceeds 80% of system memory ({system_memory} MB)"
                ),
            });
        }

        Ok(())
    }
}

/// Get system memory in MB if available
fn get_system_memory_mb() -> Option<u64> {
    // Use sysinfo or similar crate for actual implementation
    // For now, return None to disable memory validation in tests
    std::env::var("CODETRIEVER_SYSTEM_MEMORY_MB")
        .ok()
        .and_then(|s| s.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::Validate;

    #[test]
    fn test_application_config_can_be_created() {
        let config = ApplicationConfig::from_env();
        assert!(config.embedding.model.id.contains("jina")); // Uses real model
        assert_eq!(
            config.embedding.model.max_tokens,
            DEFAULT_EMBEDDING_MODEL_MAX_CONTEXT_TOKENS
        );
    }

    #[test]
    fn test_config_validation_rejects_invalid_urls() {
        let mut config = ApplicationConfig::from_env();
        config.vector_storage.url = "not-a-valid-url".to_string();

        let validation_result = config.validate();
        assert!(validation_result.is_err());
    }

    #[test]
    fn test_config_can_be_serialized_to_toml() {
        let config = ApplicationConfig::from_env();
        let toml_result = toml::to_string(&config);
        assert!(toml_result.is_ok(), "Config should serialize to TOML");

        if let Ok(toml_string) = toml_result {
            assert!(toml_string.contains("embedding"));
            assert!(toml_string.contains("database"));
        }
    }

    #[test]
    fn test_config_uses_safe_defaults() {
        let config = ApplicationConfig::from_env();

        // All configs should use safe defaults that work in any environment
        assert_eq!(
            config.embedding.model.max_tokens,
            DEFAULT_EMBEDDING_MODEL_MAX_CONTEXT_TOKENS
        );
        assert_eq!(
            config.indexing.concurrency_limit,
            DEFAULT_TOKENIZER_CONCURRENT_FILE_LIMIT
        );
        assert_eq!(config.api.enable_docs, DEFAULT_API_ENABLE_DOCS);
    }

    #[test]
    fn test_environment_variable_overrides() {
        // Test that environment variables properly override defaults
        unsafe {
            std::env::set_var("CODETRIEVER_EMBEDDING_INDEXER_CHUNK_BATCH_SIZE", "999");
            std::env::set_var("CODETRIEVER_API_PORT", "1234");
        }

        let config = ApplicationConfig::from_env();

        assert_eq!(config.embedding.performance.indexer_batch_size, 999);
        assert_eq!(config.api.port, 1234);

        // Cleanup
        unsafe {
            std::env::remove_var("CODETRIEVER_EMBEDDING_INDEXER_CHUNK_BATCH_SIZE");
            std::env::remove_var("CODETRIEVER_API_PORT");
        }
    }

    #[test]
    fn test_cross_field_validation_catches_dimension_mismatch() {
        let mut config = ApplicationConfig::from_env();
        config.embedding.model.dimensions = 512;
        config.vector_storage.vector_dimension = 256; // Mismatch!

        let validation_result = config.validate();
        assert!(validation_result.is_err());

        if let Err(error) = validation_result {
            assert!(error.to_string().contains("dimension"));
            assert!(error.to_string().contains("must match"));
        }
    }

    #[test]
    fn test_memory_estimation_calculation() {
        let config = ApplicationConfig::from_env();
        let memory_usage = config.estimate_memory_usage_mb();

        // Base model uses ~2GB + overhead
        assert!(
            memory_usage > 2000,
            "Config memory usage was: {memory_usage} MB (uses real base model)"
        );
        assert!(
            memory_usage < 5000,
            "Config memory usage was: {memory_usage} MB (should be reasonable)"
        );
    }

    #[test]
    fn test_telemetry_config_validation() {
        let mut config = ApplicationConfig::from_env();
        config.telemetry.tracing_level = "invalid-level".to_string();

        let validation_result = config.validate();
        assert!(validation_result.is_err());

        if let Err(error) = validation_result {
            assert!(error.to_string().contains("Invalid tracing level"));
        }
    }

    #[test]
    fn test_from_env_creates_valid_config() {
        let config = ApplicationConfig::from_env();
        let validation_result = config.validate();
        assert!(
            validation_result.is_ok(),
            "from_env() should create valid config: {validation_result:?}"
        );
    }

    #[test]
    fn test_embedding_model_consistency() {
        let config = ApplicationConfig::from_env();

        // All configs use the correct Jina model
        assert_eq!(
            config.embedding.model.id,
            "jinaai/jina-embeddings-v2-base-code"
        );
        assert_eq!(
            config.embedding.model.dimensions,
            DEFAULT_EMBEDDING_MODEL_DIMENSIONS
        );
    }

    #[test]
    fn test_configuration_source_loading() {
        use crate::source::{ConfigurationLoader, EnvironmentSource};

        let loader = ConfigurationLoader::new().add_source(Box::new(EnvironmentSource));

        let config_result = loader.load();
        assert!(config_result.is_ok());

        if let Ok(config) = config_result {
            assert!(config.validate().is_ok());
        }
    }

    #[test]
    fn test_telemetry_defaults() {
        let config = ApplicationConfig::from_env();

        // Uses safe defaults for telemetry
        assert!(
            (config.telemetry.trace_sample_rate - DEFAULT_TRACE_SAMPLE_RATE).abs() < f64::EPSILON
        );
        assert_eq!(config.telemetry.tracing_level, DEFAULT_TRACING_LEVEL);
        assert_eq!(
            config.telemetry.service_name,
            DEFAULT_TELEMETRY_SERVICE_NAME
        );
    }

    #[test]
    fn test_configuration_serialization_roundtrip() {
        // Test TOML serialization/deserialization without file I/O
        let original_config = ApplicationConfig::from_env();

        let toml_result = toml::to_string(&original_config);
        assert!(toml_result.is_ok());

        if let Ok(toml_string) = toml_result {
            let parsed_result: Result<ApplicationConfig, _> = toml::from_str(&toml_string);
            assert!(parsed_result.is_ok());

            if let Ok(parsed_config) = parsed_result {
                assert_eq!(
                    original_config.embedding.model.id,
                    parsed_config.embedding.model.id
                );
                assert_eq!(original_config.api.port, parsed_config.api.port);
                assert!(parsed_config.validate().is_ok());
            }
        }
    }
}
