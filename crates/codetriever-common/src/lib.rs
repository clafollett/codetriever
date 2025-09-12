//! Common utilities and patterns shared across Codetriever crates
//!
//! This crate provides shared functionality to reduce duplication across
//! the various Codetriever components.

pub mod error;
pub mod error_sanitizer;
pub mod init;

pub use error::{CommonError, ErrorContext};
pub use init::initialize_environment;
