//! Database migration utilities

use sqlx::{PgPool, Postgres, migrate::MigrateDatabase};

/// Run all pending database migrations
///
/// # Errors
///
/// Returns an error if:
/// - Migration files cannot be read or are corrupted
/// - Database connection is lost during migration
/// - Migration SQL contains invalid statements
/// - Database schema conflicts prevent migration
pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    println!("Running database migrations...");

    // Use embedded migrations from the migrations directory
    sqlx::migrate!("./migrations").run(pool).await?;

    println!("Database migrations completed successfully");
    Ok(())
}

/// Create database if it doesn't exist and run migrations
///
/// # Errors
///
/// Returns an error if:
/// - Database URL is malformed or invalid
/// - Database server is unreachable or authentication fails
/// - Insufficient privileges to create database or run migrations
/// - Database creation fails for reasons other than "already exists"
/// - Connection pool creation fails
pub async fn setup_database(database_url: &str) -> anyhow::Result<PgPool> {
    // Check if database exists, create if not
    match Postgres::database_exists(database_url).await {
        Ok(false) => {
            println!("Creating database...");
            Postgres::create_database(database_url).await?;
        }
        Ok(true) => {
            // Database already exists, that's fine
        }
        Err(e) => {
            // If we can't check existence (e.g., connection error), log it
            eprintln!("Warning: Could not check if database exists: {e}");

            // Try to create database - if it fails with "already exists", that's OK
            println!("Attempting to create database...");
            if let Err(create_err) = Postgres::create_database(database_url).await {
                // Ignore error if database already exists
                let err_msg = create_err.to_string();
                if !err_msg.contains("already exists")
                    && !err_msg.contains("duplicate")
                    && !err_msg.contains("42P04")
                // PostgreSQL error code for duplicate database
                {
                    // This is a real error, not just "database exists"
                    return Err(anyhow::anyhow!(
                        "Failed to create database: {}. Original check error: {}",
                        create_err,
                        e
                    ));
                }
                // Database exists, continue to connect
                println!("Database already exists, proceeding...");
            }
        }
    }

    // Connect to the database
    let pool = PgPool::connect(database_url).await?;

    // Run migrations
    run_migrations(&pool).await?;

    Ok(pool)
}

/// Wait for database to be ready and run migrations
///
/// # Errors
///
/// Returns an error if:
/// - Database remains unreachable after maximum retry attempts
/// - All retry attempts fail due to persistent database issues
/// - Final setup attempt fails after successful connection attempts
pub async fn wait_for_migrations(database_url: &str) -> anyhow::Result<PgPool> {
    use std::time::Duration;
    use tokio::time::sleep;

    const MAX_ATTEMPTS: u32 = 30;
    let mut attempts = 0;

    loop {
        match setup_database(database_url).await {
            Ok(pool) => return Ok(pool),
            Err(e) if attempts < MAX_ATTEMPTS => {
                // Use saturating_add for retry counter - if we somehow overflow, we're way past MAX_ATTEMPTS
                attempts = attempts.saturating_add(1);
                eprintln!("Database not ready (attempt {attempts}/{MAX_ATTEMPTS}): {e}");
                sleep(Duration::from_secs(2)).await;
            }
            Err(e) => return Err(e),
        }
    }
}
