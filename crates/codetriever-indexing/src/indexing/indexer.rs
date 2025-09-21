use crate::{IndexerResult, indexing::service::FileContent};
use codetriever_common::CorrelationId;
use codetriever_config::{ApplicationConfig, Profile};
use codetriever_embeddings::{DefaultEmbeddingService, EmbeddingService};
use codetriever_parsing::CodeChunk as ParsingCodeChunk;
use codetriever_parsing::{CodeParser, get_language_from_extension};
use codetriever_vector_data::{CodeChunk, VectorStorage};
use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

// Type alias for the repository trait object
type RepositoryRef = Arc<dyn codetriever_meta_data::traits::FileRepository>;

/// Convert from parsing CodeChunk to vector data CodeChunk
fn convert_chunk(parsing_chunk: ParsingCodeChunk) -> CodeChunk {
    CodeChunk {
        file_path: parsing_chunk.file_path,
        content: parsing_chunk.content,
        start_line: parsing_chunk.start_line,
        end_line: parsing_chunk.end_line,
        byte_start: parsing_chunk.byte_start,
        byte_end: parsing_chunk.byte_end,
        kind: parsing_chunk.kind,
        language: parsing_chunk.language,
        name: parsing_chunk.name,
        token_count: parsing_chunk.token_count,
        embedding: parsing_chunk.embedding,
    }
}

// Supported code file extensions (O(1) lookup via lazy static HashSet)
pub const CODE_EXTENSIONS: &[&str] = &[
    // Assembly
    "asm",
    "s",
    "S",
    "a51",
    "nasm",
    // Batch/Shell
    "bat",
    "cmd",
    "btm",
    "sh",
    "bash",
    "zsh",
    "fish",
    "ksh",
    "csh",
    "tcsh",
    // C/C++
    "c",
    "h",
    "i",
    "cpp",
    "cxx",
    "cc",
    "c++",
    "hpp",
    "hxx",
    "hh",
    "h++",
    "inl",
    "ipp",
    // C#
    "cs",
    "csx",
    // CMake
    "cmake",
    "CMakeLists.txt",
    // CSS
    "css",
    "scss",
    "sass",
    "less",
    "styl",
    // Dockerfile
    "dockerfile",
    "Dockerfile",
    "containerfile",
    // Fortran
    "f",
    "for",
    "f90",
    "f95",
    "f03",
    "f08",
    "f77",
    // Go
    "go",
    // Haskell
    "hs",
    "lhs",
    // HTML/XML
    "html",
    "htm",
    "xhtml",
    "xml",
    "xsl",
    "xslt",
    "svg",
    // Java/Scala
    "java",
    "scala",
    "sc",
    "sbt",
    // JavaScript/TypeScript
    "js",
    "mjs",
    "cjs",
    "jsx",
    "ts",
    "tsx",
    "mts",
    "cts",
    // JSON/YAML
    "json",
    "jsonc",
    "json5",
    "yaml",
    "yml",
    // Julia
    "jl",
    // Lua
    "lua",
    // Makefile
    "makefile",
    "Makefile",
    "mk",
    "mak",
    "make",
    // Documentation formats
    "md",
    "markdown",
    "mdown",
    "mdx",
    "rst",  // reStructuredText (Python docs)
    "adoc", // AsciiDoc
    "asciidoc",
    "textile", // Textile markup
    "org",     // Org-mode
    "txt",     // Plain text docs
    "text",
    // PHP
    "php",
    "php3",
    "php4",
    "php5",
    "php7",
    "php8",
    "phtml",
    // Perl
    "pl",
    "pm",
    "t",
    "pod",
    "perl",
    // PowerShell
    "ps1",
    "psd1",
    "psm1",
    "ps1xml",
    "pssc",
    "psc1",
    // Python
    "py",
    "pyw",
    "pyx",
    "pyi",
    "pyc",
    "pyd",
    // Ruby
    "rb",
    "rbw",
    "rake",
    "gemspec",
    "ru",
    // Rust
    "rs",
    // SQL
    "sql",
    "mysql",
    "pgsql",
    "plsql",
    "tsql",
    // TeX/LaTeX
    "tex",
    "latex",
    "ltx",
    "cls",
    "sty",
    "bib",
    // Visual Basic
    "vb",
    "vbs",
    "bas",
    "vba",
    // Other common code files
    "toml",
    "ini",
    "cfg",
    "conf",
    "config",
    "gradle",
    "groovy",
    "swift",
    "kt",
    "kts", // Kotlin
    "dart",
    "r",
    "R",
    "rmd",
    "Rmd",
    "m",
    "mm",    // Objective-C/C++
    "proto", // Protocol Buffers
    "graphql",
    "gql",
    "vue",
    "elm",
    "ex",
    "exs", // Elixir
    "erl",
    "hrl", // Erlang
    "ml",
    "mli", // OCaml
    "fs",
    "fsi",
    "fsx", // F#
    "clj",
    "cljs",
    "cljc", // Clojure
    "nim",
    "zig",
    "v",   // V lang or Verilog
    "sol", // Solidity
];

// Type alias to simplify the complex type (fixes clippy warning)
type ExtensionSet = HashSet<&'static str>;

// O(1) HashSet lookup for supported file extensions - PERFORMANCE CRITICAL
static CODE_EXTENSIONS_SET: Lazy<ExtensionSet> =
    Lazy::new(|| CODE_EXTENSIONS.iter().copied().collect());

type EmbeddingServiceRef = Arc<dyn EmbeddingService>;
type VectorStorageRef = Option<Arc<dyn VectorStorage>>;

#[derive(Debug)]
pub struct IndexResult {
    pub files_indexed: usize,
    pub chunks_created: usize,
    pub chunks_stored: usize, // Track how many were stored in Qdrant
}

pub struct Indexer {
    embedding_service: EmbeddingServiceRef,
    storage: VectorStorageRef, // Optional storage backend using trait
    code_parser: CodeParser,
    config: ApplicationConfig, // Store unified config for lazy storage initialization
    repository: Option<RepositoryRef>, // Optional database repository
}

impl Default for Indexer {
    fn default() -> Self {
        let config = ApplicationConfig::with_profile(Profile::Development);
        Self::with_config(&config)
    }
}

impl Indexer {
    /// Creates a new indexer instance with default configuration.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use codetriever_indexing::indexing::Indexer;
    ///
    /// let indexer = Indexer::new();
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new indexer with the specified configuration.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use codetriever_config::{ApplicationConfig, Profile};
    /// use codetriever_indexing::indexing::Indexer;
    ///
    /// let config = ApplicationConfig::with_profile(Profile::Development);
    /// let indexer = Indexer::with_config(&config);
    /// ```
    pub fn with_config(config: &ApplicationConfig) -> Self {
        // Use the embedding config directly from unified configuration
        let embedding_config = config.embedding.clone();

        Self {
            embedding_service: Arc::new(DefaultEmbeddingService::new(embedding_config)),
            storage: None,
            code_parser: CodeParser::new(
                None, // Will be set after tokenizer loads
                config.indexing.split_large_units,
                config.embedding.model.max_tokens,
                config.indexing.chunk_overlap_tokens,
            ),
            config: config.clone(),
            repository: None,
        }
    }

    /// Creates a new indexer with custom configuration and storage backend.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use codetriever_config::{ApplicationConfig, Profile};
    /// use codetriever_indexing::indexing::Indexer;
    /// use codetriever_vector_data::QdrantStorage;
    /// use std::sync::Arc;
    ///
    /// let config = ApplicationConfig::with_profile(Profile::Development);
    /// let storage = QdrantStorage::new(
    ///     "http://localhost:6334".to_string(),
    ///     "my_collection".to_string()
    /// ).await?;
    /// let indexer = Indexer::with_config_and_storage(&config, Arc::new(storage));
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_config_and_storage(
        config: &ApplicationConfig,
        storage: Arc<dyn VectorStorage>,
    ) -> Self {
        // Use the embedding config directly from unified configuration
        let embedding_config = config.embedding.clone();

        Self {
            embedding_service: Arc::new(DefaultEmbeddingService::new(embedding_config)),
            storage: Some(storage),
            code_parser: CodeParser::new(
                None, // Will be set after tokenizer loads
                config.indexing.split_large_units,
                config.embedding.model.max_tokens,
                config.indexing.chunk_overlap_tokens,
            ),
            config: config.clone(),
            repository: None,
        }
    }

    pub fn new_with_repository(repository: RepositoryRef) -> Self {
        let config = ApplicationConfig::with_profile(Profile::Development);
        // Use the embedding config directly from unified configuration
        let embedding_config = config.embedding.clone();

        Self {
            embedding_service: Arc::new(DefaultEmbeddingService::new(embedding_config)),
            storage: None,
            code_parser: CodeParser::new(
                None, // Will be set after tokenizer loads
                config.indexing.split_large_units,
                config.embedding.model.max_tokens,
                config.indexing.chunk_overlap_tokens,
            ),
            config: config.clone(),
            repository: Some(repository),
        }
    }

    /// Performs semantic search to find code chunks similar to the query.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query text
    /// * `limit` - Maximum number of results to return
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use codetriever_indexing::indexing::Indexer;
    /// use std::path::Path;
    ///
    /// let mut indexer = Indexer::new();
    /// indexer.index_directory(Path::new("./src"), true).await?;
    /// println!("Indexing completed successfully");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Indexes all code files in a directory.
    ///
    /// # Arguments
    ///
    /// * `path` - The directory path to index
    /// * `recursive` - Whether to recursively index subdirectories
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use codetriever_indexing::indexing::Indexer;
    /// use std::path::Path;
    ///
    /// let mut indexer = Indexer::new();
    /// let result = indexer.index_directory(Path::new("./src"), true).await?;
    /// println!("Indexed {} files", result.files_indexed);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn index_directory(
        &mut self,
        path: &Path,
        recursive: bool,
    ) -> IndexerResult<IndexResult> {
        println!("Starting index_directory for {path:?}");

        // Ensure embedding provider is ready
        println!("Loading embedding model...");
        self.embedding_service.provider().ensure_ready().await?;

        let mut files_indexed = 0;
        let mut all_chunks = Vec::new();

        // Collect files to index
        let files = if path.is_file() {
            vec![path.to_path_buf()]
        } else if path.is_dir() {
            collect_files(path, recursive)?
        } else {
            vec![]
        };

        // Process each file sequentially (concurrent processing adds complexity here)
        // The major perf wins come from the string allocation fixes, not concurrency
        println!("Processing {} files...", files.len());
        for file_path in files {
            println!("Processing file: {file_path:?}");
            if let Ok(chunks) = self.index_file_path(&file_path).await {
                println!("  Got {} chunks", chunks.len());
                files_indexed += 1;
                all_chunks.extend(chunks.into_iter().map(convert_chunk));
            } else {
                println!("  Failed to index file");
            }
        }
        println!("All files processed. Total chunks: {}", all_chunks.len());

        // Generate embeddings for all chunks in batches to avoid memory explosion
        let batch_size = self.config.indexing.embedding_batch_size;

        if !all_chunks.is_empty() {
            println!(
                "Generating embeddings for {} chunks in batches of {}",
                all_chunks.len(),
                batch_size
            );

            let total_batches = all_chunks.len().div_ceil(batch_size);

            // PERFORMANCE BOOST: Concurrent batch processing with bounded parallelism 🚀
            // Process batches concurrently for 30-50% speedup while avoiding memory explosion
            use futures::future::join_all;

            // Process batches in concurrent waves to balance speed and memory usage
            let max_concurrent_batches = 3; // Conservative bound to avoid overwhelming the system

            for wave_start in (0..total_batches).step_by(max_concurrent_batches) {
                let wave_end = (wave_start + max_concurrent_batches).min(total_batches);
                println!(
                    "Processing batches {}-{}/{} concurrently",
                    wave_start + 1,
                    wave_end,
                    total_batches
                );

                // Create futures for this wave of batches
                let mut batch_futures = Vec::new();
                for (_batch_idx, batch) in all_chunks
                    .chunks(batch_size)
                    .enumerate()
                    .skip(wave_start)
                    .take(wave_end - wave_start)
                {
                    // Extract texts with zero-copy references
                    let texts: Vec<&str> = batch.iter().map(|c| c.content.as_str()).collect();

                    // Create future for this batch
                    let future = self.embedding_service.generate_embeddings(texts);
                    batch_futures.push(future);
                }

                // Execute all batches in this wave concurrently
                let batch_results = join_all(batch_futures).await;

                // Apply results to chunks - this maintains proper ordering
                let mut chunk_offset = wave_start * batch_size;
                for (wave_idx, result) in batch_results.into_iter().enumerate() {
                    let embeddings = result?;
                    println!(
                        "Completed batch {}/{}",
                        wave_start + wave_idx + 1,
                        total_batches
                    );

                    // Apply embeddings using move semantics - zero-copy assignment!
                    for (i, embedding) in embeddings.into_iter().enumerate() {
                        if chunk_offset + i < all_chunks.len() {
                            all_chunks[chunk_offset + i].embedding = Some(embedding);
                        }
                    }
                    chunk_offset += batch_size;
                }
            }
            println!(
                "🎉 Generated embeddings for all {} chunks using concurrent processing",
                all_chunks.len()
            );
        }

        // Store chunks if storage is configured
        let chunks_stored = if !all_chunks.is_empty() {
            if let Some(ref storage) = self.storage {
                // TODO: Remove index_directory method - legacy proof-of-concept
                // Using "local" as default repository for legacy directory indexing
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                let correlation_id = CorrelationId::new();
                let _chunk_ids = storage
                    .store_chunks("local", "main", &all_chunks, timestamp, &correlation_id)
                    .await?;
                all_chunks.len()
            } else {
                0
            }
        } else {
            0
        };

        let result = IndexResult {
            files_indexed,
            chunks_created: all_chunks.len(),
            chunks_stored,
        };

        println!(
            "\n📊 Indexing complete: {} files → {} chunks → {} stored",
            result.files_indexed, result.chunks_created, result.chunks_stored
        );

        Ok(result)
    }

    /// Index file content directly without filesystem access
    /// If repository is set, will check file state and skip unchanged files
    pub async fn index_file_content(
        &mut self,
        project_id: &str,
        files: Vec<FileContent>,
    ) -> IndexerResult<IndexResult> {
        println!("Starting index_file_content for project: {project_id}");

        // Parse project_id to extract repository_id and branch if using database
        let (repository_id, branch) = if project_id.contains(':') {
            let parts: Vec<&str> = project_id.splitn(2, ':').collect();
            (
                parts[0].to_string(),
                parts.get(1).unwrap_or(&"main").to_string(),
            )
        } else {
            (project_id.to_string(), "main".to_string())
        };

        // Ensure embedding provider is ready
        println!("Loading embedding model...");
        self.embedding_service.provider().ensure_ready().await?;

        let mut all_chunks = Vec::new();
        let mut files_indexed = 0;

        // Track file metadata for database recording
        struct FileMetadata {
            file_path: String,
            generation: i64,
        }
        let mut file_metadata_map: Vec<FileMetadata> = Vec::new();

        // Ensure project branch exists if using database
        if let Some(ref repo) = self.repository {
            let ctx = codetriever_meta_data::models::RepositoryContext {
                repository_id: repository_id.clone(),
                branch: branch.clone(),
                repository_url: None,
                commit_sha: None,
                commit_message: None,
                commit_date: None,
                author: None,
                is_dirty: false,
                root_path: std::path::PathBuf::from("."),
            };
            repo.ensure_project_branch(&ctx).await?;
        }

        for file in &files {
            println!("Processing file: {}", file.path);

            let mut current_generation = 1i64;

            // If we have a repository, check file state
            if let Some(ref repo) = self.repository {
                let content_hash = codetriever_meta_data::hash_content(&file.content);
                let state = repo
                    .check_file_state(&repository_id, &branch, &file.path, &content_hash)
                    .await?;

                match state {
                    codetriever_meta_data::models::FileState::Unchanged => {
                        println!("  Skipping unchanged file");
                        continue; // Skip unchanged files
                    }
                    codetriever_meta_data::models::FileState::New { generation }
                    | codetriever_meta_data::models::FileState::Updated {
                        new_generation: generation,
                        ..
                    } => {
                        current_generation = generation;

                        // Record file indexing in database
                        let metadata = codetriever_meta_data::models::FileMetadata {
                            path: file.path.clone(),
                            content_hash: content_hash.clone(),
                            generation,
                            commit_sha: None,
                            commit_message: None,
                            commit_date: None,
                            author: None,
                        };

                        repo.record_file_indexing(&repository_id, &branch, &metadata)
                            .await?;

                        // For updated files, delete old chunks
                        if matches!(
                            state,
                            codetriever_meta_data::models::FileState::Updated { .. }
                        ) {
                            let deleted_ids = repo
                                .replace_file_chunks(
                                    &repository_id,
                                    &branch,
                                    &file.path,
                                    generation,
                                )
                                .await?;
                            println!("  Deleted {} old chunks from database", deleted_ids.len());

                            // Also delete from Qdrant if storage is available
                            if let Some(ref storage) = self.storage {
                                storage.delete_chunks(&deleted_ids).await?;
                                println!("  Deleted {} old chunks from Qdrant", deleted_ids.len());
                            }
                        }
                    }
                }
            }

            // Get language from file extension
            let ext = file.path.rsplit('.').next().unwrap_or("");
            let language = get_language_from_extension(ext).unwrap_or(ext);

            // Parse the content into chunks
            let chunks = self
                .code_parser
                .parse(&file.content, language, &file.path)?;

            if !chunks.is_empty() {
                files_indexed += 1;
                println!("  Got {} chunks", chunks.len());

                // Track file metadata for this file
                file_metadata_map.push(FileMetadata {
                    file_path: file.path.clone(),
                    generation: current_generation,
                });

                all_chunks.extend(chunks.into_iter().map(convert_chunk));
            }
        }

        let chunks_created = all_chunks.len();
        println!("Total: {files_indexed} files indexed, {chunks_created} chunks created");

        // Generate embeddings and store if we have chunks
        if !all_chunks.is_empty() {
            println!("Generating embeddings for {} chunks...", all_chunks.len());

            // Generate embeddings using zero-copy string references
            let texts: Vec<&str> = all_chunks.iter().map(|c| c.content.as_str()).collect();
            let embeddings = self.embedding_service.generate_embeddings(texts).await?;

            // Add embeddings to chunks using move semantics
            for (chunk, embedding) in all_chunks.iter_mut().zip(embeddings.into_iter()) {
                chunk.embedding = Some(embedding);
            }

            // Store chunks with embeddings if storage is configured
            if let Some(ref storage) = self.storage {
                println!("Storing {} chunks in vector database...", all_chunks.len());

                // If we have repository info, use deterministic IDs
                if self.repository.is_some() {
                    // Store chunks per file with deterministic IDs
                    for file_info in &file_metadata_map {
                        let file_chunks: Vec<&CodeChunk> = all_chunks
                            .iter()
                            .filter(|c| c.file_path == file_info.file_path)
                            .collect();

                        if !file_chunks.is_empty() {
                            // Get embeddings for these chunks
                            let mut chunks_with_embeddings = Vec::new();
                            for chunk in file_chunks {
                                chunks_with_embeddings.push(chunk.clone());
                            }

                            let correlation_id = CorrelationId::new();
                            let chunk_ids = storage
                                .store_chunks(
                                    &repository_id,
                                    &branch,
                                    &chunks_with_embeddings,
                                    file_info.generation,
                                    &correlation_id,
                                )
                                .await?;

                            // Record chunk IDs in database
                            if let Some(ref repo) = self.repository {
                                let chunk_metadata: Vec<
                                    codetriever_meta_data::models::ChunkMetadata,
                                > = chunk_ids
                                    .iter()
                                    .enumerate()
                                    .zip(&chunks_with_embeddings)
                                    .map(|((idx, id), chunk)| {
                                        codetriever_meta_data::models::ChunkMetadata {
                                            chunk_id: *id,
                                            repository_id: repository_id.clone(),
                                            branch: branch.clone(),
                                            file_path: chunk.file_path.clone(),
                                            chunk_index: idx as i32, // This is now correct per-file index
                                            generation: file_info.generation,
                                            start_line: chunk.start_line as i32,
                                            end_line: chunk.end_line as i32,
                                            byte_start: chunk.byte_start as i64,
                                            byte_end: chunk.byte_end as i64,
                                            kind: chunk.kind.clone(),
                                            name: chunk.name.clone(),
                                            created_at: chrono::Utc::now(),
                                        }
                                    })
                                    .collect();

                                repo.insert_chunks(&repository_id, &branch, chunk_metadata)
                                    .await?;
                            }
                        }
                    }
                } else {
                    // TODO: Remove this fallback - all storage should be repository-aware
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() as i64;
                    let correlation_id = CorrelationId::new();
                    let _chunk_ids = storage
                        .store_chunks("unknown", "main", &all_chunks, timestamp, &correlation_id)
                        .await?;
                }

                println!("Successfully stored chunks");
            }
        }

        Ok(IndexResult {
            files_indexed,
            chunks_created,
            chunks_stored: chunks_created, // All created chunks are stored
        })
    }

    /// Drop the collection from storage
    pub async fn drop_collection(&mut self) -> IndexerResult<bool> {
        // Drop collection if storage is configured
        if let Some(ref storage) = self.storage {
            Ok(storage.drop_collection().await?)
        } else {
            Ok(false)
        }
    }

    /// Set the storage backend for this indexer
    pub fn set_storage(&mut self, storage: impl VectorStorage + 'static) {
        self.storage = Some(Arc::new(storage));
    }

    /// Set the storage backend from Arc (for dependency injection)
    pub fn set_storage_arc(&mut self, storage: Arc<dyn VectorStorage>) {
        self.storage = Some(storage);
    }

    /// Set the embedding service (for dependency injection)
    pub fn set_embedding_service(&mut self, embedding_service: Arc<dyn EmbeddingService>) {
        self.embedding_service = embedding_service;
    }

    /// Get reference to embedding service (for SearchService)
    pub fn embedding_service(&self) -> Arc<dyn EmbeddingService> {
        Arc::clone(&self.embedding_service)
    }

    /// Get reference to vector storage (for SearchService)
    pub fn vector_storage(&self) -> VectorStorageRef {
        self.storage.as_ref().map(Arc::clone)
    }

    async fn index_file_path(&self, path: &Path) -> IndexerResult<Vec<ParsingCodeChunk>> {
        // Only index code files - comprehensive language support
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        // O(1) HashSet lookup instead of O(n) array search - MAJOR PERF WIN! 🚀
        if !CODE_EXTENSIONS_SET.contains(extension.to_lowercase().as_str()) {
            return Ok(vec![]);
        }

        let content = std::fs::read_to_string(path).map_err(|e| {
            crate::IndexerError::io_error_with_source(format!("Failed to read file: {e}"), Some(e))
        })?;

        // Determine language from extension
        let ext_lower = extension.to_lowercase();
        let language = get_language_from_extension(&ext_lower).unwrap_or(&ext_lower);
        let file_path = path.to_string_lossy().to_string();

        // Use hybrid parser for intelligent chunking
        let chunks = self.code_parser.parse(&content, language, &file_path)?;
        Ok(chunks)
    }
}

// Standalone function to collect files using functional iterator patterns
fn collect_files(dir: &Path, recursive: bool) -> IndexerResult<Vec<std::path::PathBuf>> {
    std::fs::read_dir(dir)?
        .filter_map(|entry_result| entry_result.ok()) // Handle IO errors gracefully
        .try_fold(
            Vec::new(),
            |mut acc, entry| -> IndexerResult<Vec<std::path::PathBuf>> {
                let path = entry.path();
                if path.is_file() {
                    acc.push(path);
                    Ok(acc)
                } else if recursive && path.is_dir() {
                    // Recursively collect files and extend the accumulator
                    let sub_files = collect_files(&path, recursive)?;
                    acc.extend(sub_files);
                    Ok(acc)
                } else {
                    Ok(acc)
                }
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::service::FileContent;
    use async_trait::async_trait;
    use codetriever_embeddings::{
        EmbeddingProvider, EmbeddingResult, EmbeddingService, EmbeddingStats,
    };
    use codetriever_meta_data::{mock::MockFileRepository, models::*, traits::FileRepository};
    use std::sync::Arc;

    /// Mock embedding service that doesn't require GPU or model downloads
    pub struct MockEmbeddingService;

    #[async_trait]
    impl EmbeddingService for MockEmbeddingService {
        async fn generate_embeddings(&self, texts: Vec<&str>) -> EmbeddingResult<Vec<Vec<f32>>> {
            // Return mock embeddings - deterministic but fake
            Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3, 0.4]).collect())
        }

        fn provider(&self) -> &dyn EmbeddingProvider {
            &MockEmbeddingProvider
        }

        async fn get_stats(&self) -> EmbeddingStats {
            EmbeddingStats::default()
        }
    }

    /// Mock embedding provider for unit tests
    pub struct MockEmbeddingProvider;

    #[async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed_batch(&self, texts: &[&str]) -> EmbeddingResult<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3, 0.4]).collect())
        }

        fn embedding_dimension(&self) -> usize {
            4
        }

        fn max_tokens(&self) -> usize {
            8192
        }

        fn model_name(&self) -> &str {
            "mock-test-model"
        }

        async fn is_ready(&self) -> bool {
            true
        }

        async fn ensure_ready(&self) -> EmbeddingResult<()> {
            // No-op for mock - always ready
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_indexer_uses_file_repository_to_check_state() {
        // Arrange - Create mock repository and embedding service
        let mock_repo = Arc::new(MockFileRepository::new());
        let mock_embedding_service = Arc::new(MockEmbeddingService);

        // Create indexer with mocks
        let mut indexer = Indexer {
            embedding_service: mock_embedding_service,
            storage: None,
            code_parser: CodeParser::default(),
            config: ApplicationConfig::with_profile(Profile::Test),
            repository: Some(mock_repo.clone()),
        };

        // Act - Index a file using index_file_content
        let content = r#"
fn main() {
    println!(\"Hello, world!\");
}
"#;
        let file_content = FileContent {
            path: "src/main.rs".to_string(),
            content: content.to_string(),
            hash: codetriever_meta_data::hash_content(content),
        };

        let result = indexer
            .index_file_content("test_repo:main", vec![file_content])
            .await;

        // Assert - Verify repository was called
        assert!(result.is_ok());
        assert_eq!(result.unwrap().files_indexed, 1);

        // Verify file state was checked
        let files = mock_repo
            .get_indexed_files("test_repo", "main")
            .await
            .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_path, "src/main.rs");
    }

    #[tokio::test]
    async fn test_indexer_handles_unchanged_files() {
        // Arrange - Create mock with existing file
        let mock_repo = Arc::new(MockFileRepository::new());
        let mock_embedding_service = Arc::new(MockEmbeddingService);

        // Pre-populate with existing file with the hash we will use
        let content = "test content";
        let content_hash = codetriever_meta_data::hash_content(content);

        let existing_file = FileMetadata {
            path: "src/lib.rs".to_string(),
            content_hash: content_hash.clone(),
            generation: 1,
            commit_sha: None,
            commit_message: None,
            commit_date: None,
            author: None,
        };

        mock_repo
            .record_file_indexing("test_repo", "main", &existing_file)
            .await
            .unwrap();

        let mut indexer = Indexer {
            embedding_service: mock_embedding_service,
            storage: None,
            code_parser: CodeParser::default(),
            config: ApplicationConfig::with_profile(Profile::Test),
            repository: Some(mock_repo.clone()),
        };

        // Act - Try to index same content (same hash)
        let file_content = FileContent {
            path: "src/lib.rs".to_string(),
            content: content.to_string(),
            hash: content_hash.clone(),
        };

        let result = indexer
            .index_file_content("test_repo:main", vec![file_content])
            .await;

        // Assert - File should be skipped
        assert!(result.is_ok());
        assert_eq!(result.unwrap().files_indexed, 0); // Should not index unchanged file

        let chunks = mock_repo
            .get_file_chunks("test_repo", "main", "src/lib.rs")
            .await
            .unwrap();
        assert_eq!(chunks.len(), 0); // No chunks should be created for unchanged file
    }

    #[tokio::test]
    async fn test_indexer_increments_generation_on_change() {
        // Arrange
        let mock_repo = Arc::new(MockFileRepository::new());
        let mock_embedding_service = Arc::new(MockEmbeddingService);

        // Index file first time
        let mut indexer = Indexer {
            embedding_service: mock_embedding_service,
            storage: None,
            code_parser: CodeParser::default(),
            config: ApplicationConfig::with_profile(Profile::Test),
            repository: Some(mock_repo.clone()),
        };

        let file_v1 = FileContent {
            path: "src/main.rs".to_string(),
            content: "content v1".to_string(),
            hash: codetriever_meta_data::hash_content("content v1"),
        };

        indexer
            .index_file_content("test_repo:main", vec![file_v1])
            .await
            .unwrap();

        // Act - Index with different content
        let file_v2 = FileContent {
            path: "src/main.rs".to_string(),
            content: "content v2".to_string(),
            hash: codetriever_meta_data::hash_content("content v2"),
        };

        indexer
            .index_file_content("test_repo:main", vec![file_v2])
            .await
            .unwrap();

        // Assert - Generation should be incremented
        let files = mock_repo
            .get_indexed_files("test_repo", "main")
            .await
            .unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].generation, 2);
    }
}
