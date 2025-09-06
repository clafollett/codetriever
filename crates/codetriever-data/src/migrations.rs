//! Database migration utilities

use sqlx::{PgPool, Postgres, migrate::MigrateDatabase};

/// Run all pending database migrations
pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    println!("Running database migrations...");

    // Use embedded migrations from the migrations directory
    sqlx::migrate!("./migrations").run(pool).await?;

    println!("Database migrations completed successfully");
    Ok(())
}

/// Create database if it doesn't exist and run migrations
pub async fn setup_database(database_url: &str) -> anyhow::Result<PgPool> {
    // Create database if it doesn't exist
    if !Postgres::database_exists(database_url).await? {
        println!("Creating database...");
        Postgres::create_database(database_url).await?;
    }

    // Connect to the database
    let pool = PgPool::connect(database_url).await?;

    // Run migrations
    run_migrations(&pool).await?;

    Ok(pool)
}

/// Wait for database to be ready and run migrations
pub async fn wait_for_migrations(database_url: &str) -> anyhow::Result<PgPool> {
    use std::time::Duration;
    use tokio::time::sleep;

    let mut attempts = 0;
    const MAX_ATTEMPTS: u32 = 30;

    loop {
        match setup_database(database_url).await {
            Ok(pool) => return Ok(pool),
            Err(e) if attempts < MAX_ATTEMPTS => {
                attempts += 1;
                eprintln!("Database not ready (attempt {attempts}/{MAX_ATTEMPTS}): {e}");
                sleep(Duration::from_secs(2)).await;
            }
            Err(e) => return Err(e),
        }
    }
}
