//! Run database migrations for codetriever-data
//!
//! Usage: cargo run --example `run_migrations`

use codetriever_data::migrations::setup_database;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Get database URL from env or use default
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://codetriever:codetriever@localhost:5433/codetriever".to_string()
    });

    println!("Setting up database at: {database_url}");

    let pool = setup_database(&database_url).await?;

    // Query to verify tables exist
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables 
         WHERE table_schema = 'public' 
         ORDER BY table_name",
    )
    .fetch_all(&pool)
    .await?;

    println!("\nCreated tables:");
    for table in tables {
        println!("  - {table}");
    }

    Ok(())
}
