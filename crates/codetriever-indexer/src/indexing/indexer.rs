use crate::{
    Result,
    config::Config,
    embedding::{DefaultEmbeddingService, EmbeddingConfig, EmbeddingService},
    indexing::service::FileContent,
    parsing::{CodeChunk, CodeParser, get_language_from_extension},
    storage::VectorStorage,
};
use std::path::Path;
use std::sync::Arc;

// Type alias for the repository trait object
type RepositoryRef = Arc<dyn codetriever_data::traits::FileRepository>;

// Supported code file extensions
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

type BoxedVectorStorage = Box<dyn VectorStorage>;
type BoxedEmbeddingService = Box<dyn EmbeddingService>;

#[derive(Debug)]
pub struct IndexResult {
    pub files_indexed: usize,
    pub chunks_created: usize,
    pub chunks_stored: usize, // Track how many were stored in Qdrant
}

pub struct Indexer {
    embedding_service: BoxedEmbeddingService,
    storage: Option<BoxedVectorStorage>, // Optional storage backend using trait
    code_parser: CodeParser,
    config: Config,                    // Store config for lazy storage initialization
    repository: Option<RepositoryRef>, // Optional database repository
}

impl Default for Indexer {
    fn default() -> Self {
        let config = Config::default();
        Self::with_config(&config)
    }
}

impl Indexer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(config: &Config) -> Self {
        let embedding_config = EmbeddingConfig {
            model_id: config.embedding_model.clone(),
            max_tokens: config.max_embedding_tokens,
            batch_size: 32,
            use_gpu: false,
            cache_dir: None,
        };

        Self {
            embedding_service: Box::new(DefaultEmbeddingService::new(embedding_config)),
            storage: None,
            code_parser: CodeParser::new(
                None, // Will be set after tokenizer loads
                config.split_large_semantic_units,
                config.max_embedding_tokens,
                config.chunk_overlap_tokens,
            ),
            config: config.clone(),
            repository: None,
        }
    }

    pub fn with_config_and_storage(config: &Config, storage: BoxedVectorStorage) -> Self {
        let embedding_config = EmbeddingConfig {
            model_id: config.embedding_model.clone(),
            max_tokens: config.max_embedding_tokens,
            batch_size: 32,
            use_gpu: false,
            cache_dir: None,
        };

        Self {
            embedding_service: Box::new(DefaultEmbeddingService::new(embedding_config)),
            storage: Some(storage),
            code_parser: CodeParser::new(
                None, // Will be set after tokenizer loads
                config.split_large_semantic_units,
                config.max_embedding_tokens,
                config.chunk_overlap_tokens,
            ),
            config: config.clone(),
            repository: None,
        }
    }

    pub fn new_with_repository(repository: RepositoryRef) -> Self {
        let config = Config::default();
        let embedding_config = EmbeddingConfig {
            model_id: config.embedding_model.clone(),
            max_tokens: config.max_embedding_tokens,
            batch_size: 32,
            use_gpu: false,
            cache_dir: None,
        };

        Self {
            embedding_service: Box::new(DefaultEmbeddingService::new(embedding_config)),
            storage: None,
            code_parser: CodeParser::new(
                None, // Will be set after tokenizer loads
                config.split_large_semantic_units,
                config.max_embedding_tokens,
                config.chunk_overlap_tokens,
            ),
            config: config.clone(),
            repository: Some(repository),
        }
    }

    pub async fn search(&mut self, query: &str, limit: usize) -> Result<Vec<CodeChunk>> {
        // Generate embedding for the query
        let embeddings = self
            .embedding_service
            .generate_embeddings(vec![query.to_string()])
            .await?;

        if embeddings.is_empty() {
            return Ok(vec![]);
        }

        let query_embedding = embeddings.into_iter().next().unwrap();

        // Search in Qdrant if storage is configured
        if let Some(ref storage) = self.storage {
            storage.search(query_embedding, limit).await
        } else {
            Ok(vec![])
        }
    }

    pub async fn index_directory(&mut self, path: &Path, recursive: bool) -> Result<IndexResult> {
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

        // Process each file
        println!("Processing {} files...", files.len());
        for file_path in files {
            println!("Processing file: {file_path:?}");
            if let Ok(chunks) = self.index_file_path(&file_path).await {
                println!("  Got {} chunks", chunks.len());
                files_indexed += 1;
                all_chunks.extend(chunks);
            } else {
                println!("  Failed to index file");
            }
        }
        println!("All files processed. Total chunks: {}", all_chunks.len());

        // Generate embeddings for all chunks in batches to avoid memory explosion
        let batch_size = self.config.embedding_batch_size;

        if !all_chunks.is_empty() {
            println!(
                "Generating embeddings for {} chunks in batches of {}",
                all_chunks.len(),
                batch_size
            );

            let total_batches = all_chunks.len().div_ceil(batch_size);

            for batch_start in (0..all_chunks.len()).step_by(batch_size) {
                let batch_end = (batch_start + batch_size).min(all_chunks.len());
                let batch = &mut all_chunks[batch_start..batch_end];

                println!(
                    "Processing batch {}/{}",
                    batch_start / batch_size + 1,
                    total_batches
                );

                let texts: Vec<String> = batch.iter().map(|c| c.content.clone()).collect();

                let embeddings = self.embedding_service.generate_embeddings(texts).await?;

                for (chunk, embedding) in batch.iter_mut().zip(embeddings.iter()) {
                    chunk.embedding = Some(embedding.clone());
                }
            }
            println!("Generated embeddings for all {} chunks", all_chunks.len());
        }

        // Store chunks if storage is configured
        let chunks_stored = if !all_chunks.is_empty() {
            if let Some(ref storage) = self.storage {
                storage.store_chunks(&all_chunks).await?
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
    ) -> Result<IndexResult> {
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
            let ctx = codetriever_data::models::RepositoryContext {
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
                let content_hash = codetriever_data::hash_content(&file.content);
                let state = repo
                    .check_file_state(&repository_id, &branch, &file.path, &content_hash)
                    .await?;

                match state {
                    codetriever_data::models::FileState::Unchanged => {
                        println!("  Skipping unchanged file");
                        continue; // Skip unchanged files
                    }
                    codetriever_data::models::FileState::New { generation }
                    | codetriever_data::models::FileState::Updated {
                        new_generation: generation,
                        ..
                    } => {
                        current_generation = generation;

                        // Record file indexing in database
                        let metadata = codetriever_data::models::FileMetadata {
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
                        if matches!(state, codetriever_data::models::FileState::Updated { .. }) {
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

                all_chunks.extend(chunks);
            }
        }

        let chunks_created = all_chunks.len();
        println!("Total: {files_indexed} files indexed, {chunks_created} chunks created");

        // Generate embeddings and store if we have chunks
        if !all_chunks.is_empty() {
            println!("Generating embeddings for {} chunks...", all_chunks.len());

            // Generate embeddings in batches
            let texts: Vec<String> = all_chunks.iter().map(|c| c.content.clone()).collect();
            let embeddings = self.embedding_service.generate_embeddings(texts).await?;

            // Add embeddings to chunks
            for (chunk, embedding) in all_chunks.iter_mut().zip(embeddings) {
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

                            let chunk_ids = storage
                                .store_chunks_with_ids(
                                    &repository_id,
                                    &branch,
                                    &chunks_with_embeddings,
                                    file_info.generation,
                                )
                                .await?;

                            // Record chunk IDs in database
                            if let Some(ref repo) = self.repository {
                                let chunk_metadata: Vec<codetriever_data::models::ChunkMetadata> =
                                    chunk_ids
                                        .iter()
                                        .enumerate()
                                        .zip(&chunks_with_embeddings)
                                        .map(|((idx, id), chunk)| {
                                            codetriever_data::models::ChunkMetadata {
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
                    // Fallback to old method without deterministic IDs
                    storage.store_chunks(&all_chunks).await?;
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
    pub async fn drop_collection(&mut self) -> Result<bool> {
        // Drop collection if storage is configured
        if let Some(ref storage) = self.storage {
            storage.drop_collection().await
        } else {
            Ok(false)
        }
    }

    /// Set the storage backend for this indexer
    pub fn set_storage(&mut self, storage: impl VectorStorage + 'static) {
        self.storage = Some(Box::new(storage));
    }

    async fn index_file_path(&self, path: &Path) -> Result<Vec<CodeChunk>> {
        // Only index code files - comprehensive language support
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        if !CODE_EXTENSIONS.contains(&extension.to_lowercase().as_str()) {
            return Ok(vec![]);
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::Error::Io(format!("Failed to read file: {e}")))?;

        // Determine language from extension
        let ext_lower = extension.to_lowercase();
        let language = get_language_from_extension(&ext_lower).unwrap_or(&ext_lower);
        let file_path = path.to_string_lossy().to_string();

        // Use hybrid parser for intelligent chunking
        let chunks = self.code_parser.parse(&content, language, &file_path)?;
        Ok(chunks)
    }
}

// Standalone function to collect files
fn collect_files(dir: &Path, recursive: bool) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                files.push(path);
            } else if recursive && path.is_dir() {
                files.extend(collect_files(&path, recursive)?);
            }
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexing::service::FileContent;
    use codetriever_data::{mock::MockFileRepository, models::*, traits::FileRepository};

    #[tokio::test]
    async fn test_indexer_uses_file_repository_to_check_state() {
        // Arrange - Create mock repository
        let mock_repo = Arc::new(MockFileRepository::new());

        // Create indexer with the mock repository
        let mut indexer = Indexer::new_with_repository(mock_repo.clone());

        // Act - Index a file using index_file_content
        let content = r#"
fn main() {
    println!(\"Hello, world!\");
}
"#;
        let file_content = FileContent {
            path: "src/main.rs".to_string(),
            content: content.to_string(),
            hash: codetriever_data::hash_content(content),
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

        // Pre-populate with existing file with the hash we will use
        let content = "test content";
        let content_hash = codetriever_data::hash_content(content);

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

        let mut indexer = Indexer::new_with_repository(mock_repo.clone());

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

        // Index file first time
        let mut indexer = Indexer::new_with_repository(mock_repo.clone());

        let file_v1 = FileContent {
            path: "src/main.rs".to_string(),
            content: "content v1".to_string(),
            hash: codetriever_data::hash_content("content v1"),
        };

        indexer
            .index_file_content("test_repo:main", vec![file_v1])
            .await
            .unwrap();

        // Act - Index with different content
        let file_v2 = FileContent {
            path: "src/main.rs".to_string(),
            content: "content v2".to_string(),
            hash: codetriever_data::hash_content("content v2"),
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
