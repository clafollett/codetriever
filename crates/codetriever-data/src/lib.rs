//! Codetriever data layer for `PostgreSQL` state management

// Module declarations
pub mod chunk_id;
pub mod client;
pub mod config;
pub mod error;
pub mod git;
pub mod migrations;
pub mod models;
pub mod pool;
pub mod pool_builder;
pub mod pool_manager;
pub mod repository;
pub mod traits;

pub mod mock;

// Public exports
pub use chunk_id::{generate_chunk_id, hash_content};
pub use client::DataClient;
pub use config::DatabaseConfig;
pub use error::{ConnectionPoolType, DatabaseError, DatabaseErrorExt, DatabaseOperation};
pub use migrations::{run_migrations, wait_for_migrations};
pub use models::*;
pub use pool::{create_pool, initialize_database};
pub use pool_builder::PoolConfigBuilder;
pub use pool_manager::{PoolConfig, PoolManager};
pub use repository::DbFileRepository;
pub use traits::FileRepository;
