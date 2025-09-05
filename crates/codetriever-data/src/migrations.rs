//! Database migration runner with advisory lock support

use anyhow::{Context, Result};
use sqlx::{PgPool, Row};

/// Advisory lock ID for migrations (arbitrary but stable)
const MIGRATION_LOCK_ID: i64 = 1337;

/// Run all pending migrations with advisory locking
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    // Acquire advisory lock for migrations
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(MIGRATION_LOCK_ID)
        .execute(pool)
        .await
        .context("Failed to acquire migration lock")?;

    // Ensure lock is released even on error
    let result = run_migrations_inner(pool).await;

    // Release advisory lock
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(MIGRATION_LOCK_ID)
        .execute(pool)
        .await
        .context("Failed to release migration lock")?;

    result
}

/// Internal migration runner
async fn run_migrations_inner(pool: &PgPool) -> Result<()> {
    // Create migrations table if it doesn't exist
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TIMESTAMPTZ DEFAULT NOW()
        )
    "#,
    )
    .execute(pool)
    .await
    .context("Failed to create migrations table")?;

    // Get list of applied migrations
    let applied: Vec<i32> = sqlx::query("SELECT version FROM schema_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .context("Failed to fetch applied migrations")?
        .iter()
        .map(|row| row.get(0))
        .collect();

    // Migration definitions
    let migrations = vec![
        (
            1,
            "initial_schema",
            include_str!("../migrations/001_initial_schema.sql"),
        ),
        (2, "indexes", include_str!("../migrations/002_indexes.sql")),
        (
            3,
            "functions",
            include_str!("../migrations/003_functions.sql"),
        ),
    ];

    // Apply pending migrations
    for (version, name, sql) in migrations {
        if applied.contains(&version) {
            tracing::debug!("Migration {} ({}) already applied", version, name);
            continue;
        }

        tracing::info!("Applying migration {} ({})", version, name);

        // Execute migration in a transaction
        let mut tx = pool.begin().await.context("Failed to start transaction")?;

        // Run the migration SQL
        sqlx::query(sql)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("Failed to execute migration {version} ({name})"))?;

        // Record the migration
        sqlx::query("INSERT INTO schema_migrations (version, name) VALUES ($1, $2)")
            .bind(version)
            .bind(name)
            .execute(&mut *tx)
            .await
            .context("Failed to record migration")?;

        tx.commit().await.context("Failed to commit migration")?;

        tracing::info!("Migration {} ({}) completed", version, name);
    }

    Ok(())
}

/// Check if migrations are needed
pub async fn needs_migration(pool: &PgPool) -> Result<bool> {
    // Try to query schema_migrations table
    let result = sqlx::query("SELECT COUNT(*) FROM schema_migrations")
        .fetch_one(pool)
        .await;

    match result {
        Ok(row) => {
            let count: i64 = row.get(0);
            // We have 3 migrations total
            Ok(count < 3)
        }
        Err(_) => {
            // Table doesn't exist, migrations needed
            Ok(true)
        }
    }
}

/// Wait for migrations to complete (for concurrent processes)
pub async fn wait_for_migrations(pool: &PgPool, max_wait_secs: u64) -> Result<()> {
    use tokio::time::{Duration, sleep};

    let start = std::time::Instant::now();
    let max_duration = Duration::from_secs(max_wait_secs);

    loop {
        // Try to acquire advisory lock with no wait
        let locked: Option<bool> = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(MIGRATION_LOCK_ID)
            .fetch_one(pool)
            .await
            .context("Failed to check migration lock")?;

        if locked == Some(true) {
            // We got the lock, release it and return
            sqlx::query("SELECT pg_advisory_unlock($1)")
                .bind(MIGRATION_LOCK_ID)
                .execute(pool)
                .await
                .context("Failed to release migration lock")?;
            return Ok(());
        }

        // Check timeout
        if start.elapsed() > max_duration {
            anyhow::bail!("Timeout waiting for migrations to complete");
        }

        // Wait a bit before trying again
        sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_migration_sql_embedded() {
        // Verify migrations are properly embedded
        let sql1 = include_str!("../migrations/001_initial_schema.sql");
        assert!(sql1.contains("CREATE TABLE"));

        let sql2 = include_str!("../migrations/002_indexes.sql");
        assert!(sql2.contains("CREATE INDEX"));

        let sql3 = include_str!("../migrations/003_functions.sql");
        assert!(sql3.contains("CREATE OR REPLACE FUNCTION"));
    }
}
