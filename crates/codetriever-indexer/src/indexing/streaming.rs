//! Streaming indexing pipeline for memory-efficient processing
//!
//! This module provides a streaming approach to indexing that processes
//! files in batches without loading all chunks into memory at once.

use crate::{
    Result,
    embedding::EmbeddingService,
    parsing::{CodeChunk, CodeParser, get_language_from_extension},
    storage::VectorStorage,
};
use std::path::Path;
use tokio::fs;

/// Configuration for streaming batch processing
#[derive(Debug, Clone)]
pub struct StreamingConfig {
    /// Number of files to process in parallel
    pub file_batch_size: usize,
    /// Number of chunks to accumulate before processing
    pub chunk_batch_size: usize,
    /// Maximum memory usage in bytes (approximate)
    pub max_memory_bytes: usize,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            file_batch_size: 10,
            chunk_batch_size: 100,
            max_memory_bytes: 512 * 1024 * 1024, // 512MB default
        }
    }
}

/// Streaming indexing processor that handles large repositories efficiently
pub struct StreamingIndexer<E, S>
where
    E: EmbeddingService,
    S: VectorStorage,
{
    embedding_service: E,
    storage: S,
    parser: CodeParser,
    config: StreamingConfig,
}

impl<E, S> StreamingIndexer<E, S>
where
    E: EmbeddingService,
    S: VectorStorage,
{
    pub fn new(
        embedding_service: E,
        storage: S,
        parser: CodeParser,
        config: StreamingConfig,
    ) -> Self {
        Self {
            embedding_service,
            storage,
            parser,
            config,
        }
    }

    /// Process files in a streaming fashion, yielding control periodically
    pub async fn index_directory(&mut self, path: &Path, recursive: bool) -> Result<IndexResult> {
        // Ensure embedding provider is ready
        self.embedding_service.provider().ensure_ready().await?;

        let files = if path.is_file() {
            vec![path.to_path_buf()]
        } else if path.is_dir() {
            collect_files(path, recursive)?
        } else {
            vec![]
        };

        let mut total_files = 0;
        let mut total_chunks = 0;
        let mut total_stored = 0;

        // Process files in batches to limit memory usage
        for file_batch in files.chunks(self.config.file_batch_size) {
            let mut batch_chunks = Vec::with_capacity(self.config.chunk_batch_size);

            // Process each file in the batch
            for file_path in file_batch {
                if let Ok(content) = fs::read_to_string(file_path).await {
                    let extension = file_path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or("");
                    let language = get_language_from_extension(extension);
                    let file_chunks = self.parser.parse(
                        &content,
                        language.unwrap_or("text"),
                        file_path.to_str().unwrap_or(""),
                    )?;

                    total_files += 1;
                    batch_chunks.extend(file_chunks);

                    // Process chunks if we've accumulated enough
                    if batch_chunks.len() >= self.config.chunk_batch_size {
                        let stored = self.process_chunk_batch(&mut batch_chunks).await?;
                        total_chunks += batch_chunks.len();
                        total_stored += stored;
                        batch_chunks.clear();
                    }
                }
            }

            // Process any remaining chunks in the batch
            if !batch_chunks.is_empty() {
                let stored = self.process_chunk_batch(&mut batch_chunks).await?;
                total_chunks += batch_chunks.len();
                total_stored += stored;
            }

            // Yield to allow other tasks to run
            tokio::task::yield_now().await;
        }

        Ok(IndexResult {
            files_indexed: total_files,
            chunks_created: total_chunks,
            chunks_stored: total_stored,
        })
    }

    /// Process a batch of chunks: generate embeddings and store
    async fn process_chunk_batch(&mut self, chunks: &mut [CodeChunk]) -> Result<usize> {
        if chunks.is_empty() {
            return Ok(0);
        }

        // Generate embeddings for the batch
        let texts: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
        let embeddings = self.embedding_service.generate_embeddings(texts).await?;

        // Attach embeddings to chunks
        for (chunk, embedding) in chunks.iter_mut().zip(embeddings.iter()) {
            chunk.embedding = Some(embedding.clone());
        }

        // Store the chunks
        let stored = self.storage.store_chunks(chunks).await?;

        Ok(stored)
    }
}

/// Result of indexing operation
#[derive(Debug, Clone)]
pub struct IndexResult {
    pub files_indexed: usize,
    pub chunks_created: usize,
    pub chunks_stored: usize,
}

// Helper function to collect files
fn collect_files(dir: &Path, recursive: bool) -> Result<Vec<std::path::PathBuf>> {
    use walkdir::WalkDir;

    let walker = if recursive {
        WalkDir::new(dir)
    } else {
        WalkDir::new(dir).max_depth(1)
    };

    let files: Vec<_> = walker
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            let path = e.path();
            path.extension()
                .and_then(|ext| ext.to_str())
                .and_then(get_language_from_extension)
                .is_some()
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedding::EmbeddingProvider;
    use crate::storage::MockStorage;
    use async_trait::async_trait;

    // Simple mock embedding service for testing
    struct TestEmbeddingService {
        provider: TestEmbeddingProvider,
    }

    impl TestEmbeddingService {
        fn new() -> Self {
            Self {
                provider: TestEmbeddingProvider,
            }
        }
    }

    #[async_trait]
    impl EmbeddingService for TestEmbeddingService {
        async fn generate_embeddings(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
            // Return dummy embeddings using the optimized interface
            Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
        }

        fn provider(&self) -> &dyn EmbeddingProvider {
            &self.provider
        }

        async fn get_stats(&self) -> crate::embedding::EmbeddingStats {
            crate::embedding::EmbeddingStats {
                total_embeddings: 0,
                total_batches: 0,
                avg_batch_time_ms: 0.0,
                model_name: "test-model".to_string(),
                embedding_dimension: 3,
            }
        }
    }

    struct TestEmbeddingProvider;

    #[async_trait]
    impl EmbeddingProvider for TestEmbeddingProvider {
        async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            // Return dummy embeddings for each text
            Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
        }

        fn embedding_dimension(&self) -> usize {
            3
        }

        fn max_tokens(&self) -> usize {
            512
        }

        fn model_name(&self) -> &str {
            "test-model"
        }

        async fn is_ready(&self) -> bool {
            true
        }

        async fn ensure_ready(&self) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_streaming_config_defaults() {
        let config = StreamingConfig::default();
        assert_eq!(config.file_batch_size, 10);
        assert_eq!(config.chunk_batch_size, 100);
        assert_eq!(config.max_memory_bytes, 512 * 1024 * 1024);
    }

    #[tokio::test]
    async fn test_streaming_indexer_creation() {
        let embedding_service = TestEmbeddingService::new();
        let storage = MockStorage::new();
        let parser = CodeParser::new(None, false, 512, 128);
        let config = StreamingConfig {
            file_batch_size: 2,
            chunk_batch_size: 5,
            max_memory_bytes: 1024 * 1024,
        };

        let _indexer = StreamingIndexer::new(embedding_service, storage, parser, config);
        // Successfully created the indexer
    }
}
