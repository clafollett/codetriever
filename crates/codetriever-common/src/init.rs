//! Global initialization utilities for the application

use std::sync::Once;

static INIT: Once = Once::new();

/// Initialize the application environment
///
/// This should be called once at the start of the application to:
/// - Load environment variables from .env file
/// - Set up any other global initialization
///
/// Safe to call multiple times - will only run once
pub fn initialize_environment() {
    INIT.call_once(|| {
        // Load .env file if it exists
        // This loads from current directory or searches up the tree
        dotenv::dotenv().ok();

        // Could add other initialization here in the future:
        // - Logging setup
        // - Telemetry initialization
        // - etc.
    });
}

/// Initialize environment for tests
///
/// Similar to `initialize_environment` but tailored for test scenarios
#[cfg(test)]
pub fn initialize_test_environment() {
    INIT.call_once(|| {
        // Try to load .env.test first, then .env
        dotenv::from_filename(".env.test")
            .or_else(|_| dotenv::dotenv())
            .ok();
    });
}
