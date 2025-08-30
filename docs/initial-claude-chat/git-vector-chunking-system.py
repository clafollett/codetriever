#!/usr/bin/env python3
"""
Git Repository Vectorization System for Claude Code
Handles intelligent chunking, branch tracking, and incremental updates
"""

import asyncio
import hashlib
import json
import os
from dataclasses import dataclass, asdict
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple
import subprocess

import git
import tiktoken
from openai import OpenAI
import chromadb
from chromadb.utils import embedding_functions
import tree_sitter_python as tspython
import tree_sitter_javascript as tsjavascript
import tree_sitter_rust as tsrust
import tree_sitter as ts

@dataclass
class CodeChunk:
    """Represents a semantic code chunk with metadata"""
    id: str
    content: str
    file_path: str
    start_line: int
    end_line: int
    chunk_type: str  # 'function', 'class', 'module', 'comment_block'
    language: str
    repo_name: str
    branch: str
    commit_hash: str
    created_at: datetime
    symbols: List[str]  # function names, class names, etc.
    dependencies: List[str]  # imports, requires
    docstring: Optional[str] = None
    complexity_score: int = 0
    token_count: int = 0

class GitVectorizer:
    """Main class for vectorizing Git repositories with branch awareness"""
    
    def __init__(self, vector_db_path: str = "./code_vectors"):
        self.client = chromadb.PersistentClient(path=vector_db_path)
        self.openai_client = OpenAI()  # For embeddings
        self.tokenizer = tiktoken.encoding_for_model("gpt-4")
        
        # Initialize tree-sitter parsers
        self.parsers = self._init_parsers()
        
        # File extensions to process
        self.supported_extensions = {
            '.py': 'python',
            '.js': 'javascript', 
            '.ts': 'typescript',
            '.jsx': 'javascript',
            '.tsx': 'typescript',
            '.rs': 'rust',
            '.go': 'go',
            '.java': 'java',
            '.cpp': 'cpp', '.cc': 'cpp', '.cxx': 'cpp',
            '.c': 'c',
            '.h': 'c', '.hpp': 'cpp'
        }

    def _init_parsers(self) -> Dict[str, ts.Parser]:
        """Initialize Tree-sitter parsers for supported languages"""
        parsers = {}
        
        # Python
        python_parser = ts.Parser()
        python_parser.set_language(tspython.language())
        parsers['python'] = python_parser
        
        # JavaScript/TypeScript  
        js_parser = ts.Parser()
        js_parser.set_language(tsjavascript.language())
        parsers['javascript'] = parsers['typescript'] = js_parser
        
        # Rust
        rust_parser = ts.Parser()
        rust_parser.set_language(tsrust.language())
        parsers['rust'] = rust_parser
        
        return parsers

    async def vectorize_repository(self, repo_path: str, branches: List[str] = None) -> Dict[str, int]:
        """
        Vectorize entire repository with branch awareness
        Returns: Dict of branch -> chunk count
        """
        repo = git.Repo(repo_path)
        repo_name = Path(repo_path).name
        results = {}
        
        # Default to all branches if none specified
        if branches is None:
            branches = [ref.name for ref in repo.refs if not ref.name.startswith('origin/')]
            
        for branch in branches:
            print(f"🚀 Processing branch: {branch}")
            try:
                repo.git.checkout(branch)
                chunk_count = await self._process_branch(repo, repo_name, branch)
                results[branch] = chunk_count
                print(f"✅ {branch}: {chunk_count} chunks processed")
            except Exception as e:
                print(f"❌ Error processing {branch}: {e}")
                results[branch] = 0
                
        return results

    async def _process_branch(self, repo: git.Repo, repo_name: str, branch: str) -> int:
        """Process all files in a specific branch"""
        commit_hash = repo.head.commit.hexsha
        collection_name = f"{repo_name}_{branch}".replace("-", "_").replace(".", "_")
        
        # Get or create collection for this repo+branch
        try:
            collection = self.client.get_collection(collection_name)
            # Check if we need to update (new commits)
            if await self._is_branch_current(collection, commit_hash):
                print(f"📋 {branch} is up to date")
                return len(collection.get()['ids'])
        except:
            collection = self.client.create_collection(
                name=collection_name,
                embedding_function=embedding_functions.OpenAIEmbeddingFunction(
                    api_key=os.getenv("OPENAI_API_KEY"),
                    model_name="text-embedding-3-small"
                )
            )
        
        chunks = []
        repo_root = Path(repo.working_dir)
        
        # Walk through all files
        for file_path in repo_root.rglob('*'):
            if (file_path.is_file() and 
                file_path.suffix in self.supported_extensions and
                not self._should_ignore_file(file_path)):
                
                try:
                    file_chunks = await self._chunk_file(
                        file_path, repo_name, branch, commit_hash, repo_root
                    )
                    chunks.extend(file_chunks)
                except Exception as e:
                    print(f"⚠️  Error processing {file_path}: {e}")
        
        # Batch insert chunks
        if chunks:
            await self._insert_chunks(collection, chunks)
            
        return len(chunks)

    async def _chunk_file(self, file_path: Path, repo_name: str, 
                         branch: str, commit_hash: str, repo_root: Path) -> List[CodeChunk]:
        """Intelligently chunk a single file using Tree-sitter"""
        
        try:
            content = file_path.read_text(encoding='utf-8')
        except UnicodeDecodeError:
            return []  # Skip binary files
            
        language = self.supported_extensions[file_path.suffix]
        rel_path = file_path.relative_to(repo_root)
        
        if language not in self.parsers:
            # Fallback to simple text chunking
            return await self._chunk_text_fallback(
                content, str(rel_path), language, repo_name, branch, commit_hash
            )
            
        parser = self.parsers[language]
        tree = parser.parse(content.encode())
        chunks = []
        
        # Extract semantic units
        if language == 'python':
            chunks.extend(self._extract_python_chunks(
                tree.root_node, content, str(rel_path), repo_name, branch, commit_hash
            ))
        elif language in ['javascript', 'typescript']:
            chunks.extend(self._extract_js_chunks(
                tree.root_node, content, str(rel_path), repo_name, branch, commit_hash
            ))
        elif language == 'rust':
            chunks.extend(self._extract_rust_chunks(
                tree.root_node, content, str(rel_path), repo_name, branch, commit_hash
            ))
            
        return chunks

    def _extract_python_chunks(self, root_node: ts.Node, content: str, 
                              file_path: str, repo_name: str, branch: str, 
                              commit_hash: str) -> List[CodeChunk]:
        """Extract Python functions, classes, and imports"""
        chunks = []
        lines = content.split('\n')
        
        def extract_node_content(node: ts.Node) -> str:
            start_byte = node.start_byte
            end_byte = node.end_byte
            return content[start_byte:end_byte]
        
        def walk_tree(node: ts.Node, parent_context: str = ""):
            if node.type in ['function_definition', 'class_definition']:
                # Extract function/class with context
                chunk_content = extract_node_content(node)
                
                # Get function/class name
                name_node = node.child_by_field_name('name')
                symbol_name = extract_node_content(name_node) if name_node else "anonymous"
                
                # Extract docstring if present
                docstring = None
                if (node.children and len(node.children) > 2 and 
                    node.children[2].type == 'block'):
                    first_stmt = node.children[2].children[1] if len(node.children[2].children) > 1 else None
                    if (first_stmt and first_stmt.type == 'expression_statement' and
                        first_stmt.children[0].type == 'string'):
                        docstring = extract_node_content(first_stmt.children[0]).strip('"""\'\'\'')
                
                chunk_id = hashlib.md5(
                    f"{repo_name}:{branch}:{file_path}:{node.start_point}".encode()
                ).hexdigest()
                
                chunks.append(CodeChunk(
                    id=chunk_id,
                    content=chunk_content,
                    file_path=file_path,
                    start_line=node.start_point.row + 1,
                    end_line=node.end_point.row + 1,
                    chunk_type=node.type.replace('_definition', ''),
                    language='python',
                    repo_name=repo_name,
                    branch=branch,
                    commit_hash=commit_hash,
                    created_at=datetime.now(),
                    symbols=[f"{parent_context}.{symbol_name}" if parent_context else symbol_name],
                    dependencies=[],  # TODO: extract imports
                    docstring=docstring,
                    token_count=len(self.tokenizer.encode(chunk_content))
                ))
                
                # Recursively process nested definitions
                new_context = f"{parent_context}.{symbol_name}" if parent_context else symbol_name
                for child in node.children:
                    walk_tree(child, new_context)
            else:
                # Continue walking for other node types
                for child in node.children:
                    walk_tree(child, parent_context)
        
        walk_tree(root_node)
        
        # Also extract module-level imports and docstrings
        module_imports = self._extract_imports(root_node, content)
        if module_imports:
            chunk_id = hashlib.md5(f"{repo_name}:{branch}:{file_path}:imports".encode()).hexdigest()
            chunks.append(CodeChunk(
                id=chunk_id,
                content="\n".join(module_imports),
                file_path=file_path,
                start_line=1,
                end_line=10,  # Typically at top
                chunk_type='imports',
                language='python',
                repo_name=repo_name,
                branch=branch,
                commit_hash=commit_hash,
                created_at=datetime.now(),
                symbols=[],
                dependencies=module_imports,
                token_count=len(self.tokenizer.encode("\n".join(module_imports)))
            ))
        
        return chunks

    def _extract_js_chunks(self, root_node: ts.Node, content: str,
                          file_path: str, repo_name: str, branch: str,
                          commit_hash: str) -> List[CodeChunk]:
        """Extract JavaScript/TypeScript functions and classes"""
        # Similar to Python extraction but for JS/TS syntax
        # Implementation details...
        chunks = []
        # TODO: Implement JS-specific extraction
        return chunks

    def _extract_rust_chunks(self, root_node: ts.Node, content: str,
                           file_path: str, repo_name: str, branch: str, 
                           commit_hash: str) -> List[CodeChunk]:
        """Extract Rust functions, structs, and impls"""
        # Similar extraction for Rust
        chunks = []
        # TODO: Implement Rust-specific extraction  
        return chunks

    def _extract_imports(self, root_node: ts.Node, content: str) -> List[str]:
        """Extract import statements from AST"""
        imports = []
        
        def find_imports(node: ts.Node):
            if node.type in ['import_statement', 'import_from_statement', 'future_import_statement']:
                start_byte = node.start_byte
                end_byte = node.end_byte
                import_text = content[start_byte:end_byte]
                imports.append(import_text)
            
            for child in node.children:
                find_imports(child)
        
        find_imports(root_node)
        return imports

    async def _chunk_text_fallback(self, content: str, file_path: str,
                                  language: str, repo_name: str, branch: str,
                                  commit_hash: str) -> List[CodeChunk]:
        """Fallback chunking for unsupported languages"""
        chunks = []
        lines = content.split('\n')
        chunk_size = 100  # lines per chunk
        
        for i in range(0, len(lines), chunk_size):
            chunk_lines = lines[i:i + chunk_size]
            chunk_content = '\n'.join(chunk_lines)
            
            chunk_id = hashlib.md5(
                f"{repo_name}:{branch}:{file_path}:{i}".encode()
            ).hexdigest()
            
            chunks.append(CodeChunk(
                id=chunk_id,
                content=chunk_content,
                file_path=file_path,
                start_line=i + 1,
                end_line=min(i + chunk_size, len(lines)),
                chunk_type='text_block',
                language=language,
                repo_name=repo_name,
                branch=branch,
                commit_hash=commit_hash,
                created_at=datetime.now(),
                symbols=[],
                dependencies=[],
                token_count=len(self.tokenizer.encode(chunk_content))
            ))
            
        return chunks

    async def _insert_chunks(self, collection, chunks: List[CodeChunk]):
        """Batch insert chunks into vector database"""
        if not chunks:
            return
            
        # Prepare data for ChromaDB
        ids = [chunk.id for chunk in chunks]
        documents = [chunk.content for chunk in chunks]
        metadatas = []
        
        for chunk in chunks:
            metadata = asdict(chunk)
            # Convert datetime to string for JSON serialization
            metadata['created_at'] = chunk.created_at.isoformat()
            # Remove content from metadata (it's stored separately)
            del metadata['content']
            del metadata['id']
            metadatas.append(metadata)
        
        # Insert in batches to avoid API limits
        batch_size = 100
        for i in range(0, len(chunks), batch_size):
            batch_ids = ids[i:i + batch_size]
            batch_docs = documents[i:i + batch_size]
            batch_meta = metadatas[i:i + batch_size]
            
            collection.add(
                ids=batch_ids,
                documents=batch_docs,
                metadatas=batch_meta
            )

    async def _is_branch_current(self, collection, commit_hash: str) -> bool:
        """Check if branch is already processed for this commit"""
        try:
            results = collection.get(limit=1)
            if results['metadatas']:
                return results['metadatas'][0].get('commit_hash') == commit_hash
        except:
            pass
        return False

    def _should_ignore_file(self, file_path: Path) -> bool:
        """Check if file should be ignored (node_modules, .git, etc.)"""
        ignore_patterns = [
            'node_modules', '.git', '__pycache__', '.pytest_cache',
            'target', 'build', 'dist', '.venv', 'venv',
            '.DS_Store', '*.pyc', '*.log'
        ]
        
        path_str = str(file_path)
        return any(pattern in path_str for pattern in ignore_patterns)

    async def search_similar_code(self, query: str, repo_name: str = None,
                                 branch: str = None, language: str = None,
                                 chunk_type: str = None, limit: int = 10) -> List[Dict]:
        """Search for similar code chunks"""
        collection_names = []
        
        if repo_name and branch:
            collection_names = [f"{repo_name}_{branch}".replace("-", "_").replace(".", "_")]
        else:
            # Search across all collections
            collection_names = [col.name for col in self.client.list_collections()]
        
        all_results = []
        
        for collection_name in collection_names:
            try:
                collection = self.client.get_collection(collection_name)
                
                # Build filter
                where_clause = {}
                if language:
                    where_clause['language'] = language
                if chunk_type:
                    where_clause['chunk_type'] = chunk_type
                
                results = collection.query(
                    query_texts=[query],
                    n_results=limit,
                    where=where_clause if where_clause else None
                )
                
                # Format results
                for i, doc in enumerate(results['documents'][0]):
                    metadata = results['metadatas'][0][i]
                    all_results.append({
                        'content': doc,
                        'score': 1 - results['distances'][0][i],  # Convert distance to similarity
                        'metadata': metadata,
                        'collection': collection_name
                    })
                    
            except Exception as e:
                print(f"Error searching collection {collection_name}: {e}")
        
        # Sort by score and return top results
        all_results.sort(key=lambda x: x['score'], reverse=True)
        return all_results[:limit]

    async def get_branch_diff_vectors(self, repo_path: str, base_branch: str, 
                                    compare_branch: str) -> Dict:
        """Get vector differences between branches for focused updates"""
        repo = git.Repo(repo_path)
        
        # Get diff between branches
        diff = repo.git.diff(f"{base_branch}..{compare_branch}", name_only=True)
        changed_files = diff.strip().split('\n') if diff.strip() else []
        
        base_collection_name = f"{Path(repo_path).name}_{base_branch}".replace("-", "_").replace(".", "_")
        compare_collection_name = f"{Path(repo_path).name}_{compare_branch}".replace("-", "_").replace(".", "_")
        
        try:
            base_collection = self.client.get_collection(base_collection_name)
            compare_collection = self.client.get_collection(compare_collection_name)
            
            # Get chunks for changed files only
            base_chunks = base_collection.get(
                where={"file_path": {"$in": changed_files}}
            )
            compare_chunks = compare_collection.get(
                where={"file_path": {"$in": changed_files}}
            )
            
            return {
                'changed_files': changed_files,
                'base_chunks': len(base_chunks['ids']),
                'compare_chunks': len(compare_chunks['ids']),
                'base_collection': base_collection_name,
                'compare_collection': compare_collection_name
            }
            
        except Exception as e:
            return {'error': str(e)}


# Example usage and MCP server integration
async def main():
    """Example usage of the Git vectorization system"""
    vectorizer = GitVectorizer("./git_code_vectors")
    
    # Vectorize a repository
    results = await vectorizer.vectorize_repository(
        repo_path="./my-project",
        branches=["main", "develop", "feature/auth"]
    )
    
    print("Vectorization Results:", results)
    
    # Search for similar code
    search_results = await vectorizer.search_similar_code(
        query="authentication middleware implementation",
        language="python",
        chunk_type="function",
        limit=5
    )
    
    for result in search_results:
        print(f"Score: {result['score']:.3f}")
        print(f"File: {result['metadata']['file_path']}")
        print(f"Branch: {result['metadata']['branch']}")
        print(f"Content: {result['content'][:200]}...")
        print("-" * 50)

if __name__ == "__main__":
    asyncio.run(main())
