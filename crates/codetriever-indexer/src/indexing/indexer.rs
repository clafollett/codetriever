use crate::{
    Result,
    config::Config,
    embedding::EmbeddingModel,
    indexing::service::FileContent,
    parsing::{CodeChunk, CodeParser, get_language_from_extension},
    storage::QdrantStorage,
};
use std::path::Path;

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

#[derive(Debug)]
pub struct IndexResult {
    pub files_indexed: usize,
    pub chunks_created: usize,
    pub chunks_stored: usize, // Track how many were stored in Qdrant
}

pub struct Indexer {
    embedding_model: EmbeddingModel,
    storage: Option<QdrantStorage>, // Optional storage backend
    code_parser: CodeParser,
    config: Config, // Store config for lazy storage initialization
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
        Self {
            embedding_model: EmbeddingModel::new(
                config.embedding_model.clone(),
                config.max_embedding_tokens,
            ),
            storage: None,
            code_parser: CodeParser::new(
                None, // Will be set after tokenizer loads
                config.split_large_semantic_units,
                config.max_embedding_tokens,
                config.chunk_overlap_tokens,
            ),
            config: config.clone(),
        }
    }

    pub fn with_config_and_storage(config: &Config, storage: QdrantStorage) -> Self {
        Self {
            embedding_model: EmbeddingModel::new(
                config.embedding_model.clone(),
                config.max_embedding_tokens,
            ),
            storage: Some(storage),
            code_parser: CodeParser::new(
                None, // Will be set after tokenizer loads
                config.split_large_semantic_units,
                config.max_embedding_tokens,
                config.chunk_overlap_tokens,
            ),
            config: config.clone(),
        }
    }

    pub async fn search(&mut self, query: &str, limit: usize) -> Result<Vec<CodeChunk>> {
        // Generate embedding for the query
        let query_embedding = self.embedding_model.embed(vec![query.to_string()]).await?;

        if query_embedding.is_empty() {
            return Ok(vec![]);
        }

        // Search in Qdrant if storage is configured
        if let Some(ref storage) = self.storage {
            storage.search(query_embedding[0].clone(), limit).await
        } else {
            Ok(vec![])
        }
    }

    pub async fn index_directory(&mut self, path: &Path, recursive: bool) -> Result<IndexResult> {
        println!("Starting index_directory for {path:?}");

        // Ensure embedding model is loaded first to get tokenizer
        println!("Loading embedding model...");
        self.embedding_model.ensure_model_loaded().await?;

        // Share tokenizer with parser (parser will configure it for counting)
        println!("Setting up tokenizer for parser...");
        if let Some(tokenizer) = self.embedding_model.get_tokenizer() {
            println!("Creating CodeParser with tokenizer...");
            self.code_parser = CodeParser::new(
                Some(tokenizer),
                self.config.split_large_semantic_units,
                self.config.max_embedding_tokens,
                self.config.chunk_overlap_tokens,
            );
            println!("CodeParser created");
        } else {
            println!("No tokenizer available from embedding model");
        }

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
            if let Ok(chunks) = self.index_file(&file_path).await {
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

                let embeddings = self.embedding_model.embed(texts).await?;

                for (chunk, embedding) in batch.iter_mut().zip(embeddings.iter()) {
                    chunk.embedding = Some(embedding.clone());
                }
            }
            println!("Generated embeddings for all {} chunks", all_chunks.len());
        }

        // Ensure storage is initialized and store chunks
        let chunks_stored = if !all_chunks.is_empty() {
            self.ensure_storage().await?;
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
    pub async fn index_content(
        &mut self,
        project_id: &str,
        files: Vec<FileContent>,
    ) -> Result<IndexResult> {
        println!("Starting index_content for project: {project_id}");

        // Ensure embedding model is loaded first to get tokenizer
        println!("Loading embedding model...");
        self.embedding_model.ensure_model_loaded().await?;

        // Share tokenizer with parser
        if let Some(tokenizer) = self.embedding_model.get_tokenizer() {
            self.code_parser = CodeParser::new(
                Some(tokenizer),
                self.config.split_large_semantic_units,
                self.config.max_embedding_tokens,
                self.config.chunk_overlap_tokens,
            );
        }

        // Ensure storage is initialized
        self.ensure_storage().await?;

        let mut all_chunks = Vec::new();
        let mut files_indexed = 0;

        for file in &files {
            println!("Processing file: {}", file.path);

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
            let embeddings = self.embedding_model.embed(texts).await?;

            // Add embeddings to chunks
            for (chunk, embedding) in all_chunks.iter_mut().zip(embeddings) {
                chunk.embedding = Some(embedding);
            }

            // Store chunks with embeddings
            if let Some(ref storage) = self.storage {
                println!("Storing {} chunks in vector database...", all_chunks.len());
                storage.store_chunks(&all_chunks).await?;
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
        // Ensure storage is initialized first
        self.ensure_storage().await?;

        if let Some(ref storage) = self.storage {
            storage.drop_collection().await
        } else {
            Ok(false)
        }
    }

    /// Ensure storage is initialized (lazy initialization)
    async fn ensure_storage(&mut self) -> Result<()> {
        if self.storage.is_none() {
            println!("Initializing Qdrant storage at {}", self.config.qdrant_url);
            let storage = QdrantStorage::new(
                self.config.qdrant_url.clone(),
                self.config.qdrant_collection.clone(),
            )
            .await?;
            println!("Qdrant storage initialized successfully");
            self.storage = Some(storage);
        }
        Ok(())
    }

    async fn index_file(&self, path: &Path) -> Result<Vec<CodeChunk>> {
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
