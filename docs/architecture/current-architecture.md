# Codetriever Architecture Documentation

## System Overview

Codetriever is a code search and retrieval system that indexes codebases using semantic embeddings, enabling intelligent code discovery through vector similarity search.

## High-Level Architecture

```mermaid
graph TB
    subgraph "External Systems"
        PG[(PostgreSQL)]
        QD[(Qdrant)]
        FS[File System]
    end
    
    subgraph "Codetriever System"
        API[codetriever-api]
        IDX[codetriever-indexer]
        DATA[codetriever-data]
    end
    
    API --> IDX
    API --> DATA
    IDX --> DATA
    IDX --> QD
    DATA --> PG
    IDX --> FS
    
    style API fill:#e1f5fe
    style IDX fill:#fff3e0
    style DATA fill:#f3e5f5
```

## Component Architecture

```mermaid
classDiagram
    class Indexer {
        -embedding_service: Box~dyn EmbeddingService~
        -storage: Option~Box~dyn VectorStorage~~
        -code_parser: CodeParser
        -config: Config
        -repository: Option~Arc~dyn FileRepository~~
        +new() Self
        +with_config(config) Self
        +index_directory(path, recursive) IndexResult
        +index_files(files) IndexResult
        +search(query, limit) Vec~SearchResult~
    }
    
    class ContentParser {
        <<trait>>
        +name() str
        +parse(content, language, file_path) Vec~CodeChunk~
        +supports_language(language) bool
        +supported_languages() Vec~str~
    }
    
    class CodeParser {
        -tokenizer: Option~Arc~Tokenizer~~
        -split_large_semantic_units: bool
        -max_tokens: usize
        -overlap_tokens: usize
        +new(tokenizer, split, max, overlap) Self
        +parse(content, language, file_path) Vec~CodeChunk~
        +count_tokens(text) Option~usize~
    }
    
    class ChunkingService {
        -counter: Arc~dyn TokenCounter~
        -budget: TokenBudget
        +new(counter, budget) Self
        +chunk_spans(file_path, spans) Vec~CodeChunk~
        +merge_small_spans(spans) Vec~CodeSpan~
        +split_large_span(span) Vec~CodeSpan~
    }
    
    class TokenCounter {
        <<trait>>
        +name() str
        +max_tokens() usize
        +count(text) usize
    }
    
    class TiktokenCounter {
        -model_name: String
        -encoder: CoreBPE
        -max_tokens: usize
        +new(model_name) Self
    }
    
    class HeuristicCounter {
        -name: String
        -max_tokens: usize
        -chars_per_token: f64
        -calibration_data: Option~CalibrationCache~
        +new(name, max_tokens) Self
        +calibrate(sample_data) Self
    }
    
    class TokenCounterRegistry {
        -counters: HashMap~String, BoxedCounter~
        +new() Self
        +register(name, counter)
        +get(name) Option~Arc~dyn TokenCounter~~
        +get_or_create(name) Arc~dyn TokenCounter~
    }
    
    class EmbeddingService {
        <<trait>>
        +generate_embeddings(texts) Vec~Vec~f32~~
        +provider() dyn EmbeddingProvider
        +get_stats() EmbeddingStats
    }
    
    class DefaultEmbeddingService {
        -provider: Box~dyn EmbeddingProvider~
        -stats: Arc~RwLock~EmbeddingStats~~
        -batch_size: usize
        +new(model) Self
        +process_in_batches(texts) Vec~Vec~f32~~
    }
    
    class EmbeddingProvider {
        <<trait>>
        +embed_batch(texts) Vec~Vec~f32~~
        +embed(text) Vec~f32~
        +embedding_dimension() usize
        +max_tokens() usize
        +model_name() str
        +is_ready() bool
        +ensure_ready()
    }
    
    class VectorStorage {
        <<trait>>
        +store_chunks(chunks) usize
        +store_chunks_with_ids(repo, branch, chunks, gen) Vec~Uuid~
        +search(query, limit) Vec~CodeChunk~
        +delete_chunks(ids)
        +collection_exists() bool
        +ensure_collection()
        +drop_collection() bool
        +get_stats() StorageStats
    }
    
    class QdrantStorage {
        -client: Qdrant
        -collection_name: String
        -dimension: usize
        +new(url, collection, dimension) Self
        +create_collection_if_needed()
    }
    
    class MockVectorStorage {
        -chunks: Arc~Mutex~Vec~CodeChunk~~~
        -collection_exists: Arc~Mutex~bool~~
        +new() Self
    }
    
    class FileRepository {
        <<trait>>
        +ensure_project_branch(ctx) ProjectBranch
        +check_file_state(repo, branch, path, hash) FileState
        +record_file_indexing(repo, branch, metadata) IndexedFile
        +insert_chunks(repo, branch, chunks)
        +replace_file_chunks(repo, branch, path, gen) Vec~Uuid~
        +create_indexing_job(repo, branch, sha) IndexingJob
        +update_job_progress(job_id, files, chunks)
        +complete_job(job_id, status, error)
    }
    
    class DbFileRepository {
        -pool: PgPool
        +new(pool) Self
    }
    
    class MockFileRepository {
        -files: Arc~Mutex~HashMap~~
        -chunks: Arc~Mutex~HashMap~~
        +new() Self
    }
    
    %% Relationships
    Indexer --> ContentParser : uses
    Indexer --> EmbeddingService : uses
    Indexer --> VectorStorage : uses
    Indexer --> FileRepository : uses
    
    ContentParser <|.. CodeParser : implements
    
    CodeParser --> TokenCounter : uses for counting
    
    ChunkingService --> TokenCounter : uses
    
    TokenCounter <|.. TiktokenCounter : implements
    TokenCounter <|.. HeuristicCounter : implements
    
    TokenCounterRegistry --> TokenCounter : manages
    
    EmbeddingService <|.. DefaultEmbeddingService : implements
    DefaultEmbeddingService --> EmbeddingProvider : delegates to
    
    VectorStorage <|.. QdrantStorage : implements
    VectorStorage <|.. MockVectorStorage : implements
    
    FileRepository <|.. DbFileRepository : implements
    FileRepository <|.. MockFileRepository : implements
```

## Data Flow

### Indexing Pipeline

```mermaid
sequenceDiagram
    participant API
    participant Indexer
    participant Parser
    participant Chunker as ChunkingService
    participant Counter as TokenCounter
    participant EmbedSvc as EmbeddingService
    participant VecStore as VectorStorage
    participant FileRepo as FileRepository
    participant PG as PostgreSQL
    participant Qdrant
    
    API->>Indexer: index_directory(path)
    Indexer->>EmbedSvc: ensure_ready()
    
    loop For each file
        Indexer->>FileRepo: check_file_state(file, hash)
        FileRepo->>PG: Query indexed_files
        PG-->>FileRepo: FileState
        
        alt File changed or new
            Indexer->>Parser: parse(content, language, file_path)
            
            Parser->>Counter: count_tokens(text)
            Counter-->>Parser: token_count
            
            Parser->>Chunker: chunk_spans(file_path, spans)
            Chunker->>Counter: count(text)
            Counter-->>Chunker: token_count
            Chunker-->>Parser: Vec<CodeChunk>
            
            Parser-->>Indexer: Vec<CodeChunk>
            
            Indexer->>EmbedSvc: generate_embeddings(chunks)
            EmbedSvc-->>Indexer: Vec<Vec<f32>>
            
            Indexer->>VecStore: store_chunks_with_ids(chunks)
            VecStore->>Qdrant: Upsert vectors
            
            Indexer->>FileRepo: record_file_indexing(metadata)
            FileRepo->>PG: Insert/Update records
        end
    end
    
    Indexer-->>API: IndexResult
```

### Search Flow

```mermaid
sequenceDiagram
    participant User
    participant API
    participant Indexer
    participant EmbedSvc as EmbeddingService
    participant VecStore as VectorStorage
    participant Qdrant
    
    User->>API: search(query)
    API->>Indexer: search(query, limit)
    Indexer->>EmbedSvc: generate_embeddings([query])
    EmbedSvc-->>Indexer: query_embedding
    Indexer->>VecStore: search(embedding, limit)
    VecStore->>Qdrant: SearchPoints
    Qdrant-->>VecStore: SearchResults
    VecStore-->>Indexer: Vec<CodeChunk>
    Indexer-->>API: SearchResults
    API-->>User: JSON Response
```

## Storage Architecture

### Dual Storage System

```mermaid
graph TB
    subgraph "Metadata Storage (PostgreSQL)"
        PB[project_branches]
        IF[indexed_files]
        CM[chunk_metadata]
        IJ[indexing_jobs]
        
        PB -->|1:N| IF
        IF -->|1:N| CM
        PB -->|1:N| IJ
    end
    
    subgraph "Vector Storage (Qdrant)"
        COL[Collection: codetriever]
        VEC[Vectors: 768-dim embeddings]
        PAY[Payload: file_path, content, lines, etc]
        
        COL --> VEC
        COL --> PAY
    end
    
    subgraph "Synchronization"
        UUID[UUID v5 Generation]
        UUID -->|deterministic IDs| CM
        UUID -->|same IDs| VEC
    end
```

## Key Abstractions

### Token Counting System

```mermaid
graph LR
    subgraph "Token Counter Hierarchy"
        TC[TokenCounter Trait]
        TC --> TK[TiktokenCounter]
        TC --> HC[HeuristicCounter]
        
        TK -->|OpenAI Models| GPT4[GPT-4/GPT-5]
        HC -->|Fallback| ANY[Any Model]
        
        REG[TokenCounterRegistry]
        REG -->|manages| TC
    end
```

### Embedding Provider System

```mermaid
graph TD
    subgraph "Embedding Providers"
        EP[EmbeddingProvider Trait]
        EP --> LEP[LocalEmbeddingProvider]
        EP --> MEP[MockEmbeddingProvider]
        EP --> OEP[OpenAIProvider*]
        
        LEP --> BERT[BERT Models]
        LEP --> JINA[Jina Models]
        OEP --> ADA[Ada-002*]
    end
    
    note1[*Future implementations]
```

## Configuration

```yaml
# Environment Variables
DATABASE_URL: PostgreSQL connection string
QDRANT_URL: Qdrant server URL
EMBEDDING_MODEL: Model name (e.g., jinaai/jina-embeddings-v2-small-en)
MAX_EMBEDDING_TOKENS: Maximum tokens per chunk (default: 512)
SPLIT_LARGE_SEMANTIC_UNITS: Whether to split large functions/classes
CHUNK_OVERLAP_TOKENS: Token overlap between chunks
```

## Testing Strategy

```mermaid
graph LR
    subgraph "Test Infrastructure"
        UT[Unit Tests]
        IT[Integration Tests]
        
        UT --> MT[Mock Traits]
        MT --> MES[MockEmbeddingService]
        MT --> MVS[MockVectorStorage]
        MT --> MFR[MockFileRepository]
        
        IT --> RT[Real Components]
        RT --> RQ[Test Qdrant]
        RT --> RPG[Test PostgreSQL]
        RT --> REM[Real Embeddings]
    end
```

## Design Principles

1. **Dependency Inversion**: All major components depend on trait abstractions
2. **Single Responsibility**: Each service has one clear purpose
3. **Open/Closed**: Easy to add new providers without modifying existing code
4. **Interface Segregation**: Traits are minimal and focused
5. **Testability**: All components can be tested in isolation using mocks

## Key Features

- **Language Support**: 25+ programming languages via Tree-sitter
- **Smart Chunking**: Token-aware splitting with overlap for context preservation
- **Multiple Token Counters**: Tiktoken for OpenAI models, heuristic fallback for others
- **Flexible Storage**: Trait-based storage supports Qdrant, mock, and future backends
- **State Tracking**: PostgreSQL tracks file versions and indexing history
- **Incremental Indexing**: Only re-indexes changed files
- **Semantic Search**: Vector similarity search using embeddings

## Performance Characteristics

- **Embedding Batch Size**: 32 texts per batch
- **Default Max Tokens**: 512 per chunk
- **Overlap Tokens**: 128 for context preservation
- **Vector Dimension**: 768 (Jina v2) or model-specific
- **Supported File Size**: No hard limit (large files are automatically chunked)