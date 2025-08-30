#!/usr/bin/env python3
"""
Git Repository Vectorizer using LanceDB (SQLite-style vector storage)
Lightweight, file-based, zero-server vector database for code chunks
"""

import asyncio
import hashlib
import json
import os
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Optional, Union
import subprocess

import git
import lancedb
import pandas as pd
import numpy as np
from openai import OpenAI
import tiktoken
import tree_sitter_python as tspython
import tree_sitter as ts

@dataclass
class CodeChunk:
    """Code chunk with vector embedding"""
    id: str
    content: str
    file_path: str
    start_line: int
    end_line: int
    chunk_type: str  # 'function', 'class', 'module', 'imports'
    language: str
    repo_name: str
    branch: str
    commit_hash: str
    created_at: str
    symbols: List[str]
    dependencies: List[str]
    docstring: Optional[str] = None
    token_count: int = 0
    vector: Optional[List[float]] = None

class LanceGitVectorizer:
    """Git vectorizer using LanceDB for SQLite-like simplicity"""
    
    def __init__(self, db_path: str = "./git_code_vectors.lance"):
        self.db_path = db_path
        self.db = lancedb.connect(db_path)
        self.openai_client = OpenAI()
        self.tokenizer = tiktoken.encoding_for_model("gpt-4")
        
        # Initialize tree-sitter for Python (expand as needed)
        self.python_parser = ts.Parser()
        self.python_parser.set_language(tspython.language())
        
        # Supported file types
        self.supported_extensions = {
            '.py': 'python',
            '.js': 'javascript',
            '.ts': 'typescript', 
            '.jsx': 'javascript',
            '.tsx': 'typescript',
            '.rs': 'rust',
            '.go': 'go',
        }

    async def vectorize_repository(self, repo_path: str, branches: List[str] = None) -> Dict[str, int]:
        """Vectorize repository with branch-aware storage"""
        repo = git.Repo(repo_path)
        repo_name = Path(repo_path).name
        results = {}
        
        if branches is None:
            branches = [ref.name for ref in repo.refs if not ref.name.startswith('origin/')]
        
        all_chunks = []
        
        for branch in branches:
            print(f"🚀 Processing branch: {branch}")
            try:
                repo.git.checkout(branch)
                branch_chunks = await self._process_branch(repo, repo_name, branch)
                all_chunks.extend(branch_chunks)
                results[branch] = len(branch_chunks)
                print(f"✅ {branch}: {len(branch_chunks)} chunks")
            except Exception as e:
                print(f"❌ Error processing {branch}: {e}")
                results[branch] = 0
        
        # Store all chunks in single table with branch metadata
        if all_chunks:
            await self._store_chunks(all_chunks)
            
        return results

    async def _process_branch(self, repo: git.Repo, repo_name: str, branch: str) -> List[CodeChunk]:
        """Extract chunks from all files in branch"""
        commit_hash = repo.head.commit.hexsha
        chunks = []
        repo_root = Path(repo.working_dir)
        
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
        
        return chunks

    async def _chunk_file(self, file_path: Path, repo_name: str, branch: str, 
                         commit_hash: str, repo_root: Path) -> List[CodeChunk]:
        """Chunk a single file using Tree-sitter"""
        try:
            content = file_path.read_text(encoding='utf-8')
        except UnicodeDecodeError:
            return []
            
        language = self.supported_extensions[file_path.suffix]
        rel_path = str(file_path.relative_to(repo_root))
        
        if language == 'python':
            return await self._chunk_python_file(
                content, rel_path, repo_name, branch, commit_hash
            )
        else:
            # Fallback to simple text chunking
            return await self._chunk_text_fallback(
                content, rel_path, language, repo_name, branch, commit_hash
            )

    async def _chunk_python_file(self, content: str, file_path: str, 
                                repo_name: str, branch: str, commit_hash: str) -> List[CodeChunk]:
        """Extract Python functions and classes with Tree-sitter"""
        tree = self.python_parser.parse(content.encode())
        chunks = []
        
        def extract_node_content(node: ts.Node) -> str:
            return content[node.start_byte:node.end_byte]
        
        def get_docstring(node: ts.Node) -> Optional[str]:
            """Extract docstring from function/class node"""
            if node.children and len(node.children) > 2:
                body = node.children[2]  # function body
                if (body.type == 'block' and len(body.children) > 1):
                    first_stmt = body.children[1]
                    if (first_stmt.type == 'expression_statement' and 
                        first_stmt.children[0].type == 'string'):
                        doc = extract_node_content(first_stmt.children[0])
                        return doc.strip('"""\'\'\'').strip()
            return None
        
        def walk_node(node: ts.Node, parent_context: str = ""):
            if node.type in ['function_definition', 'class_definition']:
                chunk_content = extract_node_content(node)
                
                # Get symbol name
                name_node = node.child_by_field_name('name')
                symbol_name = extract_node_content(name_node) if name_node else "anonymous"
                full_symbol = f"{parent_context}.{symbol_name}" if parent_context else symbol_name
                
                # Create chunk
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
                    created_at=datetime.now().isoformat(),
                    symbols=[full_symbol],
                    dependencies=self._extract_imports(tree.root_node, content),
                    docstring=get_docstring(node),
                    token_count=len(self.tokenizer.encode(chunk_content))
                ))
                
                # Process nested definitions
                new_context = full_symbol
                for child in node.children:
                    walk_node(child, new_context)
            else:
                for child in node.children:
                    walk_node(child, parent_context)
        
        # Extract all functions and classes
        walk_node(tree.root_node)
        
        # Also extract module-level imports as a chunk
        imports = self._extract_imports(tree.root_node, content)
        if imports:
            import_content = '\n'.join(imports)
            chunk_id = hashlib.md5(f"{repo_name}:{branch}:{file_path}:imports".encode()).hexdigest()
            
            chunks.append(CodeChunk(
                id=chunk_id,
                content=import_content,
                file_path=file_path,
                start_line=1,
                end_line=len(imports),
                chunk_type='imports',
                language='python',
                repo_name=repo_name,
                branch=branch,
                commit_hash=commit_hash,
                created_at=datetime.now().isoformat(),
                symbols=[],
                dependencies=imports,
                token_count=len(self.tokenizer.encode(import_content))
            ))
        
        return chunks

    def _extract_imports(self, root_node: ts.Node, content: str) -> List[str]:
        """Extract import statements"""
        imports = []
        
        def find_imports(node: ts.Node):
            if node.type in ['import_statement', 'import_from_statement']:
                imports.append(content[node.start_byte:node.end_byte])
            for child in node.children:
                find_imports(child)
        
        find_imports(root_node)
        return imports

    async def _chunk_text_fallback(self, content: str, file_path: str, language: str,
                                  repo_name: str, branch: str, commit_hash: str) -> List[CodeChunk]:
        """Simple text-based chunking for non-Python files"""
        chunks = []
        lines = content.split('\n')
        chunk_size = 50  # lines per chunk
        
        for i in range(0, len(lines), chunk_size):
            chunk_lines = lines[i:i + chunk_size]
            chunk_content = '\n'.join(chunk_lines)
            
            if not chunk_content.strip():
                continue
                
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
                created_at=datetime.now().isoformat(),
                symbols=[],
                dependencies=[],
                token_count=len(self.tokenizer.encode(chunk_content))
            ))
        
        return chunks

    async def _generate_embeddings(self, texts: List[str]) -> List[List[float]]:
        """Generate embeddings for text chunks"""
        response = self.openai_client.embeddings.create(
            model="text-embedding-3-small",
            input=texts
        )
        return [embedding.embedding for embedding in response.data]

    async def _store_chunks(self, chunks: List[CodeChunk]):
        """Store chunks in LanceDB with vectors"""
        if not chunks:
            return
            
        print(f"🧮 Generating embeddings for {len(chunks)} chunks...")
        
        # Generate embeddings in batches
        batch_size = 100
        all_embeddings = []
        
        for i in range(0, len(chunks), batch_size):
            batch = chunks[i:i + batch_size]
            texts = [chunk.content for chunk in batch]
            embeddings = await self._generate_embeddings(texts)
            all_embeddings.extend(embeddings)
        
        # Prepare data for LanceDB
        chunk_data = []
        for chunk, embedding in zip(chunks, all_embeddings):
            chunk_dict = {
                'id': chunk.id,
                'content': chunk.content,
                'file_path': chunk.file_path,
                'start_line': chunk.start_line,
                'end_line': chunk.end_line,
                'chunk_type': chunk.chunk_type,
                'language': chunk.language,
                'repo_name': chunk.repo_name,
                'branch': chunk.branch,
                'commit_hash': chunk.commit_hash,
                'created_at': chunk.created_at,
                'symbols': json.dumps(chunk.symbols),
                'dependencies': json.dumps(chunk.dependencies),
                'docstring': chunk.docstring or "",
                'token_count': chunk.token_count,
                'vector': embedding
            }
            chunk_data.append(chunk_dict)
        
        # Create or append to table
        table_name = "code_chunks"
        try:
            # Try to get existing table
            table = self.db.open_table(table_name)
            # Convert to DataFrame and add
            df = pd.DataFrame(chunk_data)
            table.add(df)
            print(f"✅ Added {len(chunks)} chunks to existing table")
        except Exception:
            # Create new table
            df = pd.DataFrame(chunk_data)
            table = self.db.create_table(table_name, df)
            print(f"✅ Created table with {len(chunks)} chunks")

    async def search_similar_code(self, query: str, repo_name: str = None, 
                                 branch: str = None, language: str = None,
                                 chunk_type: str = None, limit: int = 10) -> List[Dict]:
        """Search for similar code using vector similarity"""
        
        # Generate query embedding
        query_embedding = await self._generate_embeddings([query])
        query_vector = query_embedding[0]
        
        try:
            table = self.db.open_table("code_chunks")
            
            # Build filter conditions
            filters = []
            if repo_name:
                filters.append(f"repo_name = '{repo_name}'")
            if branch:
                filters.append(f"branch = '{branch}'")
            if language:
                filters.append(f"language = '{language}'")
            if chunk_type:
                filters.append(f"chunk_type = '{chunk_type}'")
            
            # Perform vector search
            query_builder = table.search(query_vector).limit(limit)
            
            if filters:
                filter_string = " AND ".join(filters)
                query_builder = query_builder.where(filter_string)
            
            results = query_builder.to_pandas()
            
            # Format results
            formatted_results = []
            for _, row in results.iterrows():
                formatted_results.append({
                    'content': row['content'],
                    'score': 1 / (1 + row['_distance']),  # Convert distance to similarity score
                    'metadata': {
                        'file_path': row['file_path'],
                        'branch': row['branch'],
                        'repo_name': row['repo_name'],
                        'chunk_type': row['chunk_type'],
                        'language': row['language'],
                        'symbols': json.loads(row['symbols']),
                        'start_line': row['start_line'],
                        'end_line': row['end_line'],
                        'docstring': row['docstring'] if row['docstring'] else None
                    }
                })
            
            return formatted_results
            
        except Exception as e:
            print(f"❌ Search error: {e}")
            return []

    def _should_ignore_file(self, file_path: Path) -> bool:
        """Check if file should be ignored"""
        ignore_patterns = [
            'node_modules', '.git', '__pycache__', '.pytest_cache',
            'target', 'build', 'dist', '.venv', 'venv', '.env',
            '.DS_Store', '*.log'
        ]
        
        path_str = str(file_path)
        return any(pattern in path_str for pattern in ignore_patterns)

    async def get_repo_stats(self) -> Dict:
        """Get statistics about stored repositories"""
        try:
            table = self.db.open_table("code_chunks")
            df = table.to_pandas()
            
            stats = {
                'total_chunks': len(df),
                'repositories': df['repo_name'].nunique(),
                'branches': df['branch'].nunique(),
                'languages': df['language'].value_counts().to_dict(),
                'chunk_types': df['chunk_type'].value_counts().to_dict(),
                'repos_by_branch': df.groupby('repo_name')['branch'].nunique().to_dict()
            }
            
            return stats
        except Exception as e:
            return {'error': str(e)}

    async def delete_repo_branch(self, repo_name: str, branch: str = None):
        """Delete specific repo/branch from database"""
        try:
            table = self.db.open_table("code_chunks")
            
            if branch:
                # Delete specific branch
                condition = f"repo_name = '{repo_name}' AND branch = '{branch}'"
            else:
                # Delete entire repo
                condition = f"repo_name = '{repo_name}'"
                
            # LanceDB delete operation
            table.delete(condition)
            print(f"✅ Deleted {repo_name}" + (f":{branch}" if branch else "") + " from database")
            
        except Exception as e:
            print(f"❌ Delete error: {e}")


# Example usage
async def main():
    """Example usage of LanceDB Git vectorizer"""
    vectorizer = LanceGitVectorizer("./git_vectors.lance")
    
    # Vectorize repository
    results = await vectorizer.vectorize_repository(
        repo_path="./my-project",
        branches=["main", "develop"]
    )
    print("Vectorization results:", results)
    
    # Get database stats
    stats = await vectorizer.get_repo_stats()
    print("Database stats:", json.dumps(stats, indent=2))
    
    # Search for code
    search_results = await vectorizer.search_similar_code(
        query="user authentication function",
        language="python",
        limit=5
    )
    
    print("\n🔍 Search Results:")
    for i, result in enumerate(search_results, 1):
        print(f"\n{i}. Score: {result['score']:.3f}")
        print(f"   File: {result['metadata']['file_path']} ({result['metadata']['branch']})")
        print(f"   Type: {result['metadata']['chunk_type']}")
        if result['metadata']['symbols']:
            print(f"   Symbols: {', '.join(result['metadata']['symbols'])}")
        print(f"   Content preview: {result['content'][:200]}...")

if __name__ == "__main__":
    asyncio.run(main())
