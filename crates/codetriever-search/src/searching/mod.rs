//! Search service module for querying indexed code
pub mod search;
pub mod service;

pub use search::{SearchMatch, SearchResult};
pub use service::{ContextResult, SearchService};

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
