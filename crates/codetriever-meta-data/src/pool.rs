//! Database connection pool management

use anyhow::{Context, Result};
use sqlx::PgPool;

use crate::config::DatabaseConfig;
use crate::migrations::run_migrations;

/// Create database connection pool
///
/// # Errors
///
/// Returns an error if:
/// - Database connection parameters are invalid
/// - Database server is unreachable or refuses connections
/// - Authentication credentials are invalid
/// - Connection timeout is exceeded
/// - Pool configuration parameters are invalid
pub async fn create_pool(config: &DatabaseConfig) -> Result<PgPool> {
    config
        .create_pool()
        .await
        .context("Failed to create database pool")
}

/// Initialize database (create pool and run migrations)
///
/// # Errors
///
/// Returns an error if:
/// - Pool creation fails (see `create_pool` errors)
/// - Database migrations fail to run
/// - Migration advisory lock cannot be acquired
pub async fn initialize_database(config: &DatabaseConfig) -> Result<PgPool> {
    let pool = create_pool(config).await?;

    // Run migrations with advisory lock
    run_migrations(&pool)
        .await
        .context("Failed to run database migrations")?;

    Ok(pool)
}
