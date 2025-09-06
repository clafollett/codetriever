//! Integration tests for content-based indexing functionality
//!
//! These tests verify that the index_file_content method properly:
//! - Accepts file content without filesystem access
//! - Parses content into chunks
//! - Generates embeddings
//! - Stores in Qdrant
//! - Enables semantic search

#[path = "test_utils.rs"]
mod test_utils;

use codetriever_indexer::indexing::{Indexer, service::FileContent};
use test_utils::{cleanup_test_storage, create_test_storage, skip_without_hf_token, test_config};

#[tokio::test]
async fn test_index_file_content_with_multiple_files() {
    if skip_without_hf_token().is_none() {
        return;
    }

    let config = test_config();
    let storage = create_test_storage("content_indexing")
        .await
        .expect("Failed to create storage");

    let mut indexer = Indexer::with_config_and_storage(&config, Box::new(storage.clone()));

    // Create test files with actual code content
    let files = vec![
        FileContent {
            path: "src/main.rs".to_string(),
            content: r#"
/// Main entry point for the application
fn main() {
    println!("Starting application");
    let config = load_config();
    run_server(config);
}

/// Load configuration from environment
fn load_config() -> Config {
    Config::from_env()
}

/// Run the HTTP server
fn run_server(config: Config) {
    println!("Server running on port {}", config.port);
}
"#
            .to_string(),
            hash: "hash_main_rs".to_string(),
        },
        FileContent {
            path: "src/lib.rs".to_string(),
            content: r#"
//! Core library functionality

use std::collections::HashMap;

/// Database connection handler
pub struct Database {
    connection: String,
    cache: HashMap<String, String>,
}

impl Database {
    /// Create a new database connection
    pub fn new(url: &str) -> Self {
        Self {
            connection: url.to_string(),
            cache: HashMap::new(),
        }
    }
    
    /// Query the database
    pub fn query(&self, sql: &str) -> Result<Vec<Record>, Error> {
        // Implementation here
        Ok(vec![])
    }
}
"#
            .to_string(),
            hash: "hash_lib_rs".to_string(),
        },
        FileContent {
            path: "src/utils.py".to_string(),
            content: r#"
"""Utility functions for data processing"""

import json
from typing import Dict, List, Any

def process_data(data: List[Dict[str, Any]]) -> Dict[str, Any]:
    """Process raw data into structured format.
    
    Args:
        data: List of data records
        
    Returns:
        Processed data dictionary
    """
    result = {
        "count": len(data),
        "items": []
    }
    
    for item in data:
        if validate_item(item):
            result["items"].append(transform_item(item))
    
    return result

def validate_item(item: Dict[str, Any]) -> bool:
    """Validate a single data item."""
    return "id" in item and "value" in item

def transform_item(item: Dict[str, Any]) -> Dict[str, Any]:
    """Transform item to output format."""
    return {
        "id": item["id"],
        "value": str(item["value"]).upper(),
        "timestamp": item.get("timestamp", None)
    }
"#
            .to_string(),
            hash: "hash_utils_py".to_string(),
        },
    ];

    // Index the content
    let result = indexer
        .index_file_content("test-project", files)
        .await
        .expect("Failed to index content");

    // Verify results
    assert_eq!(result.files_indexed, 3, "Should index 3 files");
    assert!(result.chunks_created > 0, "Should create chunks");
    assert_eq!(
        result.chunks_stored, result.chunks_created,
        "All chunks should be stored"
    );

    println!("Indexing results:");
    println!("  Files indexed: {}", result.files_indexed);
    println!("  Chunks created: {}", result.chunks_created);
    println!("  Chunks stored: {}", result.chunks_stored);

    cleanup_test_storage(&storage)
        .await
        .expect("Failed to cleanup");
}

#[tokio::test]
async fn test_index_file_content_creates_searchable_chunks() {
    if skip_without_hf_token().is_none() {
        return;
    }

    let config = test_config();
    let storage = create_test_storage("content_search")
        .await
        .expect("Failed to create storage");

    let mut indexer = Indexer::with_config_and_storage(&config, Box::new(storage.clone()));

    // Index content with specific searchable terms
    let files = vec![FileContent {
        path: "database.rs".to_string(),
        content: r#"
/// PostgreSQL connection manager
pub struct PostgresConnection {
    pool: ConnectionPool,
}

impl PostgresConnection {
    /// Execute a SQL query on the postgres database
    pub async fn execute_query(&self, sql: &str) -> Result<QueryResult> {
        let conn = self.pool.get().await?;
        conn.execute(sql).await
    }
    
    /// Insert data into postgres table
    pub async fn insert_record(&self, table: &str, data: &Record) -> Result<()> {
        let sql = format!("INSERT INTO {} VALUES ($1, $2)", table);
        self.execute_query(&sql).await?;
        Ok(())
    }
}
"#
        .to_string(),
        hash: "hash_db".to_string(),
    }];

    // Index the content
    let index_result = indexer
        .index_file_content("search-test", files)
        .await
        .expect("Failed to index");

    assert!(index_result.chunks_created > 0, "Should create chunks");

    // Search for indexed content
    let results = indexer
        .search("postgres database query", 5)
        .await
        .expect("Failed to search");

    assert!(
        !results.is_empty(),
        "Should find results for postgres query"
    );

    // Verify the result contains our content
    let first_result = &results[0];
    assert!(
        first_result.content.contains("PostgreSQL")
            || first_result.content.contains("execute_query")
            || first_result.content.contains("postgres"),
        "Search should return relevant content"
    );

    println!("Search returned {} results", results.len());
    println!(
        "First result: {}",
        &first_result.content[..100.min(first_result.content.len())]
    );

    cleanup_test_storage(&storage)
        .await
        .expect("Failed to cleanup");
}

#[tokio::test]
async fn test_index_file_content_handles_different_languages() {
    if skip_without_hf_token().is_none() {
        return;
    }

    let config = test_config();
    let storage = create_test_storage("languages")
        .await
        .expect("Failed to create storage");

    let mut indexer = Indexer::with_config_and_storage(&config, Box::new(storage.clone()));

    let files = vec![
        // JavaScript/TypeScript
        FileContent {
            path: "app.js".to_string(),
            content: r#"
class UserController {
    constructor(database) {
        this.db = database;
    }
    
    async getUser(id) {
        const user = await this.db.query('SELECT * FROM users WHERE id = ?', [id]);
        return user;
    }
}

module.exports = UserController;
"#
            .to_string(),
            hash: "hash_js".to_string(),
        },
        // Go
        FileContent {
            path: "handler.go".to_string(),
            content: r#"
package main

import (
    "fmt"
    "net/http"
)

func HandleRequest(w http.ResponseWriter, r *http.Request) {
    fmt.Fprintf(w, "Hello from Go handler")
}

func main() {
    http.HandleFunc("/", HandleRequest)
    http.ListenAndServe(":8080", nil)
}
"#
            .to_string(),
            hash: "hash_go".to_string(),
        },
        // Python
        FileContent {
            path: "api.py".to_string(),
            content: r#"
from flask import Flask, jsonify

app = Flask(__name__)

@app.route('/api/data')
def get_data():
    """Return JSON data from the API."""
    return jsonify({
        'status': 'success',
        'data': fetch_from_database()
    })

def fetch_from_database():
    # Database logic here
    return []
"#
            .to_string(),
            hash: "hash_py".to_string(),
        },
    ];

    let result = indexer
        .index_file_content("multi-lang", files)
        .await
        .expect("Failed to index");

    assert_eq!(result.files_indexed, 3, "Should index all language files");
    assert!(
        result.chunks_created >= 3,
        "Should create at least one chunk per file"
    );

    println!("Multi-language indexing:");
    println!("  Files: {}", result.files_indexed);
    println!("  Chunks: {}", result.chunks_created);

    cleanup_test_storage(&storage)
        .await
        .expect("Failed to cleanup");
}

#[tokio::test]
async fn test_index_file_content_handles_large_files() {
    if skip_without_hf_token().is_none() {
        return;
    }

    let config = test_config();
    let storage = create_test_storage("large_files")
        .await
        .expect("Failed to create storage");

    let mut indexer = Indexer::with_config_and_storage(&config, Box::new(storage.clone()));

    // Create a large file that will need chunking
    let mut large_content = String::new();

    // Add many functions to create a large file
    for i in 0..50 {
        large_content.push_str(&format!(
            r#"
/// Function number {i} documentation
/// This function performs operation {i}
pub fn function_{i}(param: i32) -> i32 {{
    // Complex logic here with lots of comments
    // to make the content larger and require chunking
    let result = param * {i};
    
    // More processing
    if result > 100 {{
        return result + {i};
    }} else {{
        return result - {i};
    }}
}}

"#
        ));
    }

    let files = vec![FileContent {
        path: "large_file.rs".to_string(),
        content: large_content,
        hash: "hash_large".to_string(),
    }];

    let result = indexer
        .index_file_content("large-file-test", files)
        .await
        .expect("Failed to index large file");

    assert_eq!(result.files_indexed, 1, "Should index the file");
    assert!(
        result.chunks_created > 1,
        "Large file should be split into multiple chunks"
    );

    println!("Large file indexing:");
    println!("  Chunks created from 1 file: {}", result.chunks_created);

    cleanup_test_storage(&storage)
        .await
        .expect("Failed to cleanup");
}

#[tokio::test]
async fn test_index_file_content_handles_empty_and_invalid_files() {
    if skip_without_hf_token().is_none() {
        return;
    }

    let config = test_config();
    let storage = create_test_storage("edge_cases")
        .await
        .expect("Failed to create storage");

    let mut indexer = Indexer::with_config_and_storage(&config, Box::new(storage.clone()));

    let files = vec![
        // Empty file
        FileContent {
            path: "empty.rs".to_string(),
            content: "".to_string(),
            hash: "hash_empty".to_string(),
        },
        // File with only whitespace
        FileContent {
            path: "whitespace.py".to_string(),
            content: "   \n\n\t\t  \n   ".to_string(),
            hash: "hash_whitespace".to_string(),
        },
        // File with only comments
        FileContent {
            path: "comments.js".to_string(),
            content: "// Just a comment\n/* Another comment */".to_string(),
            hash: "hash_comments".to_string(),
        },
        // Binary/non-text content (simulated)
        FileContent {
            path: "binary.dat".to_string(),
            content: "\x00\x01\x02\x03\x04".to_string(),
            hash: "hash_binary".to_string(),
        },
        // Valid small file
        FileContent {
            path: "valid.rs".to_string(),
            content: "fn main() { println!(\"test\"); }".to_string(),
            hash: "hash_valid".to_string(),
        },
    ];

    let result = indexer
        .index_file_content("edge-cases", files)
        .await
        .expect("Should handle edge cases gracefully");

    // The indexer counts files as "indexed" if they produce chunks
    // Empty files and whitespace won't produce chunks
    // Comments-only and valid code will produce chunks
    println!("  Files indexed: {}", result.files_indexed);
    println!("  Chunks created: {}", result.chunks_created);

    // We should get chunks from files with actual content
    assert!(
        result.chunks_created >= 1,
        "Should create chunks for files with content"
    );

    // Files indexed should match chunks created (files that produced chunks)
    assert!(
        result.files_indexed > 0,
        "Should index at least the valid file"
    );

    println!("Edge case handling:");
    println!("  Files indexed: {}", result.files_indexed);
    println!("  Chunks created: {}", result.chunks_created);

    cleanup_test_storage(&storage)
        .await
        .expect("Failed to cleanup");
}

#[tokio::test]
async fn test_index_file_content_without_filesystem_access() {
    // This test verifies that index_file_content doesn't access the filesystem
    // even if the paths in FileContent look like real files

    if skip_without_hf_token().is_none() {
        return;
    }

    let config = test_config();
    let storage = create_test_storage("no_filesystem")
        .await
        .expect("Failed to create storage");

    let mut indexer = Indexer::with_config_and_storage(&config, Box::new(storage.clone()));

    // Use paths that definitely don't exist
    let files = vec![FileContent {
        path: "/definitely/does/not/exist/fake_file.rs".to_string(),
        content: r#"
pub fn test_function() -> String {
    "This content is provided directly, not read from filesystem".to_string()
}
"#
        .to_string(),
        hash: "fake_hash".to_string(),
    }];

    // Should succeed even though the path doesn't exist
    let result = indexer
        .index_file_content("no-filesystem-test", files)
        .await
        .expect("Should index content without filesystem access");

    assert_eq!(result.files_indexed, 1, "Should index the content");
    assert!(
        result.chunks_created > 0,
        "Should create chunks from content"
    );

    println!("No filesystem access test passed");

    cleanup_test_storage(&storage)
        .await
        .expect("Failed to cleanup");
}
