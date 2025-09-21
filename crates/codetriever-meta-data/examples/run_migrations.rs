//! Run database migrations for codetriever-meta-data
//!
//! Usage: cargo run --example `run_migrations`

use codetriever_config::{DatabaseConfig, Profile};
use codetriever_meta_data::pool::initialize_database;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Get database configuration from environment variables
    // Use unified configuration system with development profile
    let config = DatabaseConfig::for_profile(Profile::Development);

    // Log connection info WITHOUT password
    println!(
        "Setting up database at: {}",
        config.safe_connection_string()
    );

    let pool = initialize_database(&config).await?;

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
