use crate::{
    Result,
    config::Config,
    embedding::EmbeddingModel,
    parser::{CodeParser, get_language_from_extension},
    storage::QdrantStorage,
};
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    pub file_path: String,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub embedding: Option<Vec<f32>>,
}

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
            embedding_model: EmbeddingModel::new(config.embedding_model.clone()),
            storage: None,
            code_parser: CodeParser::new(
                None,
                config.split_large_semantic_units,
                config.fallback_chunk_overlap_tokens,
            ),
            config: config.clone(),
        }
    }

    pub fn with_config_and_storage(config: &Config, storage: QdrantStorage) -> Self {
        Self {
            embedding_model: EmbeddingModel::new(config.embedding_model.clone()),
            storage: Some(storage),
            code_parser: CodeParser::new(
                None,
                config.split_large_semantic_units,
                config.fallback_chunk_overlap_tokens,
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
        for file_path in files {
            if let Ok(chunks) = self.index_file(&file_path).await {
                files_indexed += 1;
                all_chunks.extend(chunks);
            }
        }

        // Generate embeddings for all chunks
        if !all_chunks.is_empty() {
            let texts: Vec<String> = all_chunks.iter().map(|c| c.content.clone()).collect();
            let embeddings = self.embedding_model.embed(texts).await?;

            for (chunk, embedding) in all_chunks.iter_mut().zip(embeddings.iter()) {
                chunk.embedding = Some(embedding.clone());
            }
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

        Ok(IndexResult {
            files_indexed,
            chunks_created: all_chunks.len(),
            chunks_stored,
        })
    }

    /// Ensure storage is initialized (lazy initialization)
    async fn ensure_storage(&mut self) -> Result<()> {
        if self.storage.is_none() {
            let storage = QdrantStorage::new(
                self.config.qdrant_url.clone(),
                self.config.qdrant_collection.clone(),
            )
            .await?;
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
        let parser_chunks = self.code_parser.parse(&content, language, &file_path)?;

        // Convert parser chunks to indexer chunks
        let mut chunks = Vec::new();
        for parser_chunk in parser_chunks {
            chunks.push(CodeChunk {
                file_path: parser_chunk.file_path,
                content: parser_chunk.content,
                start_line: parser_chunk.start_line,
                end_line: parser_chunk.end_line,
                embedding: None,
            });
        }

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
