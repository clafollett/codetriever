//! Common test utilities for API tests

/// Standard test result type for all test functions
pub type TestResult = Result<(), Box<dyn std::error::Error>>;
