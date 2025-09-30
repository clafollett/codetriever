//! Test data constants and utilities for consistent testing
//!
//! Provides standardized test data to reduce hard-coded values and improve maintainability

#![allow(clippy::unwrap_used, clippy::expect_used)] // Test code

use serde_json::json;

// Performance thresholds as constants
pub const SEARCH_MAX_MS: u128 = 2000;
pub const INDEX_MAX_MS: u128 = 5000;
pub const SMALL_SEARCH_MAX_MS: u128 = 500;

// Standard test data
pub const TEST_PROJECT_ID: &str = "test-project";
pub const TEST_RUST_CONTENT: &str = r"
fn authenticate(user: &str, password: &str) -> Result<Token, AuthError> {
    if user.is_empty() || password.is_empty() {
        return Err(AuthError::InvalidCredentials);
    }

    // Validate credentials
    let token = generate_token(user)?;
    Ok(token)
}

fn generate_token(user: &str) -> Result<Token, AuthError> {
    // Token generation logic
    Ok(Token::new(user))
}
";

pub const TEST_JAVASCRIPT_CONTENT: &str = r"
function authenticate(user, password) {
    if (!user || !password) {
        throw new Error('Invalid credentials');
    }

    // Generate JWT token
    const token = jwt.sign({ user }, process.env.JWT_SECRET);
    return token;
}

async function validateToken(token) {
    try {
        const payload = jwt.verify(token, process.env.JWT_SECRET);
        return payload;
    } catch (error) {
        throw new Error('Invalid token');
    }
}
";

/// Creates a standard test index request with multiple file types
pub fn create_test_index_request() -> serde_json::Value {
    json!({
        "project_id": TEST_PROJECT_ID,
        "files": [
            {
                "path": "src/auth.rs",
                "content": TEST_RUST_CONTENT
            },
            {
                "path": "src/auth.js",
                "content": TEST_JAVASCRIPT_CONTENT
            },
            {
                "path": "src/utils.rs",
                "content": "pub fn format_error(msg: &str) -> String { format!(\"Error: {}\", msg) }"
            }
        ]
    })
}

/// Creates a standard search request
pub fn create_test_search_request(query: &str, limit: usize) -> serde_json::Value {
    json!({
        "query": query,
        "limit": limit
    })
}

/// Common test queries for consistent testing
pub const TEST_QUERIES: &[&str] = &[
    "authentication logic",
    "token generation",
    "error handling",
    "async function",
    "validate credentials",
    "jwt token",
    "database connection",
    "format string",
];

/// Unicode test strings for international support testing
pub const UNICODE_TEST_QUERIES: &[&str] = &[
    "函数 authentication",    // Chinese + English
    "función autenticación",  // Spanish
    "функция аутентификация", // Russian
    "🔍 search function",     // Emoji
    "测试 test テスト",       // Multi-language
];

/// Large content for boundary testing
pub fn create_large_file_content(size_lines: usize) -> String {
    let mut content = String::new();
    content.push_str("// Large file for boundary testing\n");
    content.push_str("fn large_function() {\n");

    for i in 0..size_lines {
        use std::fmt::Write;
        writeln!(content, "    println!(\"Line {i} of large function\");")
            .expect("Writing to String cannot fail");
    }

    content.push_str("}\n");
    content
}

/// Creates test data with cleanup helper
pub struct TestDataCleanup {
    pub project_id: String,
}

impl TestDataCleanup {
    pub const fn new(project_id: String) -> Self {
        Self { project_id }
    }
}

impl Drop for TestDataCleanup {
    fn drop(&mut self) {
        // This could be extended to actually clean up test data
        // For now, just log cleanup
        println!("🧹 Cleaning up test data for project: {}", self.project_id);
    }
}
