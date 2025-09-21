pub mod indexer;
pub mod service;
pub mod watcher;

pub use indexer::{IndexResult, Indexer};
pub use service::{FileContent, IndexerService};
pub use watcher::FileWatcher;

#[cfg(any(test, feature = "test-utils"))]
pub use service::test_utils;
