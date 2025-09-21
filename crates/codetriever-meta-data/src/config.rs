//! Database configuration

use sqlx::PgPool;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use std::time::Duration;

/// Database configuration using proper connection parameters
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub ssl_mode: PgSslMode,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout: Duration,
    pub idle_timeout: Duration,
}

impl DatabaseConfig {
    /// Create a new `DatabaseConfig` with explicit values
    pub const fn new(
        host: String,
        port: u16,
        database: String,
        username: String,
        password: String,
    ) -> Self {
        Self {
            host,
            port,
            database,
            username,
            password,
            ssl_mode: PgSslMode::Prefer,
            max_connections: 10,
            min_connections: 2,
            connect_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600),
        }
    }

    /// Create a `DatabaseConfig` from environment variables
    ///
    /// Requires individual components: `DB_HOST`, `DB_PORT`, `DB_NAME`, `DB_USER`, `DB_PASSWORD`
    ///
    /// # Panics
    ///
    /// Panics if required environment variables are not set
    #[allow(clippy::expect_used)]
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("DB_HOST").expect("DB_HOST must be set"),
            port: std::env::var("DB_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(5432),
            database: std::env::var("DB_NAME").expect("DB_NAME must be set"),
            username: std::env::var("DB_USER").expect("DB_USER must be set"),
            password: std::env::var("DB_PASSWORD").expect("DB_PASSWORD must be set"),
            ssl_mode: std::env::var("DB_SSLMODE")
                .ok()
                .and_then(|s| match s.as_str() {
                    "disable" => Some(PgSslMode::Disable),
                    "prefer" => Some(PgSslMode::Prefer),
                    "require" => Some(PgSslMode::Require),
                    _ => None,
                })
                .unwrap_or(PgSslMode::Prefer),
            max_connections: std::env::var("DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            min_connections: std::env::var("DB_MIN_CONNECTIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2),
            connect_timeout: Duration::from_secs(
                std::env::var("DB_CONNECT_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
            ),
            idle_timeout: Duration::from_secs(
                std::env::var("DB_IDLE_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(600),
            ),
        }
    }

    /// Build `PostgreSQL` connection options (no URL with password!)
    pub fn connect_options(&self) -> PgConnectOptions {
        PgConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .database(&self.database)
            .username(&self.username)
            .password(&self.password)
            .ssl_mode(self.ssl_mode)
    }

    /// Create a connection pool
    ///
    /// # Errors
    ///
    /// Returns an error if connection to database fails
    pub async fn create_pool(&self) -> Result<PgPool, sqlx::Error> {
        PgPoolOptions::new()
            .max_connections(self.max_connections)
            .min_connections(self.min_connections)
            .acquire_timeout(self.connect_timeout)
            .idle_timeout(self.idle_timeout)
            .connect_with(self.connect_options())
            .await
    }

    /// Get connection info for logging (NO PASSWORD!)
    pub fn safe_connection_string(&self) -> String {
        format!(
            "{}@{}:{}/{} (ssl: {:?})",
            self.username, self.host, self.port, self.database, self.ssl_mode
        )
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_with_explicit_values() {
        let config = DatabaseConfig::new(
            "localhost".to_string(),
            5432,
            "testdb".to_string(),
            "testuser".to_string(),
            "testpass".to_string(),
        );
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 5432);
        assert_eq!(config.database, "testdb");
        assert_eq!(config.username, "testuser");
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.min_connections, 2);
    }

    #[test]
    fn test_safe_connection_string() {
        let config = DatabaseConfig::new(
            "localhost".to_string(),
            5432,
            "testdb".to_string(),
            "testuser".to_string(),
            "super_secret_password".to_string(),
        );
        let safe_str = config.safe_connection_string();
        assert!(safe_str.contains("testuser@localhost:5432/testdb"));
        assert!(!safe_str.contains("super_secret_password"));
    }
}
