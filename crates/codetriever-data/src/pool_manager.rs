//! Connection pool management with read/write separation
//!
//! This module provides separated connection pools for different operation types,
//! improving database performance and preventing resource contention.

use crate::config::DatabaseConfig;
use anyhow::{Context, Result};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

/// Extension trait for saturating cast from usize to u32
trait SaturatingCast {
    fn saturating_cast(self) -> u32;
}

impl SaturatingCast for usize {
    fn saturating_cast(self) -> u32 {
        u32::try_from(self).unwrap_or(u32::MAX)
    }
}

/// Configuration for connection pools
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum connections for write pool
    pub write_pool_size: u32,
    /// Maximum connections for read pool
    pub read_pool_size: u32,
    /// Maximum connections for analytics pool
    pub analytics_pool_size: u32,
    /// Connection timeout in seconds
    pub connect_timeout: u64,
    /// Idle timeout in seconds
    pub idle_timeout: u64,
    /// Maximum lifetime in seconds
    pub max_lifetime: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            write_pool_size: 10,
            read_pool_size: 20,
            analytics_pool_size: 5,
            connect_timeout: 30,
            idle_timeout: 600,
            max_lifetime: 1800,
        }
    }
}

/// Manages multiple connection pools for different operation types
///
/// All fields intentionally end with '_pool' as they represent distinct pools
/// for different operation types (write, read, analytics). This naming makes
/// the purpose of each pool immediately clear.
#[derive(Clone)]
#[allow(clippy::struct_field_names)]
pub struct PoolManager {
    /// Pool for write operations (indexing, updates)
    write_pool: PgPool,
    /// Pool for read operations (queries, lookups)
    read_pool: PgPool,
    /// Pool for analytics and heavy queries (search, aggregations)
    analytics_pool: PgPool,
}

impl PoolManager {
    /// Create a new pool manager with the given configuration
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database URL is malformed or contains invalid parameters
    /// - Database server is unreachable or refuses connections
    /// - Authentication fails due to invalid credentials
    /// - Any of the three connection pools (write, read, analytics) fail to connect
    /// - Connection timeout is exceeded for any pool
    pub async fn new(db_config: &DatabaseConfig, config: PoolConfig) -> Result<Self> {
        let base_options = db_config.connect_options().application_name("codetriever");

        // Create write pool - smaller, for transactional operations
        let write_pool = PgPoolOptions::new()
            .max_connections(config.write_pool_size)
            .acquire_timeout(Duration::from_secs(config.connect_timeout))
            .idle_timeout(Duration::from_secs(config.idle_timeout))
            .max_lifetime(Duration::from_secs(config.max_lifetime))
            .connect_with(base_options.clone())
            .await
            .context("Failed to create write pool")?;

        // Create read pool - larger, for concurrent queries
        let read_pool = PgPoolOptions::new()
            .max_connections(config.read_pool_size)
            .acquire_timeout(Duration::from_secs(config.connect_timeout))
            .idle_timeout(Duration::from_secs(config.idle_timeout))
            .max_lifetime(Duration::from_secs(config.max_lifetime))
            .connect_with(base_options.clone())
            .await
            .context("Failed to create read pool")?;

        // Create analytics pool - separate pool for heavy operations
        let analytics_pool = PgPoolOptions::new()
            .max_connections(config.analytics_pool_size)
            .acquire_timeout(Duration::from_secs(
                config.connect_timeout.saturating_mul(2),
            )) // Longer timeout for analytics
            .idle_timeout(Duration::from_secs(config.idle_timeout))
            .max_lifetime(Duration::from_secs(config.max_lifetime))
            .connect_with(base_options)
            .await
            .context("Failed to create analytics pool")?;

        Ok(Self {
            write_pool,
            read_pool,
            analytics_pool,
        })
    }

    /// Get the write pool for indexing and update operations
    pub const fn write_pool(&self) -> &PgPool {
        &self.write_pool
    }

    /// Get the read pool for query operations
    pub const fn read_pool(&self) -> &PgPool {
        &self.read_pool
    }

    /// Get the analytics pool for heavy search and aggregation queries
    pub const fn analytics_pool(&self) -> &PgPool {
        &self.analytics_pool
    }

    /// Create with default configuration
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `DATABASE_URL` environment variable is not set
    /// - Database URL from environment is malformed
    /// - Pool creation fails (see `new` method errors)
    pub async fn from_env() -> Result<Self> {
        let db_config = DatabaseConfig::from_env();
        Self::new(&db_config, PoolConfig::default()).await
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            write_pool: ConnectionStats {
                size: self.write_pool.size(),
                idle: self.write_pool.num_idle().saturating_cast(),
                max: self.write_pool.options().get_max_connections(),
            },
            read_pool: ConnectionStats {
                size: self.read_pool.size(),
                idle: self.read_pool.num_idle().saturating_cast(),
                max: self.read_pool.options().get_max_connections(),
            },
            analytics_pool: ConnectionStats {
                size: self.analytics_pool.size(),
                idle: self.analytics_pool.num_idle().saturating_cast(),
                max: self.analytics_pool.options().get_max_connections(),
            },
        }
    }

    /// Close all pools
    pub async fn close(&self) {
        self.write_pool.close().await;
        self.read_pool.close().await;
        self.analytics_pool.close().await;
    }
}

/// Statistics for a connection pool
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    /// Current number of connections
    pub size: u32,
    /// Number of idle connections
    pub idle: u32,
    /// Maximum connections allowed
    pub max: u32,
}

/// Combined statistics for all pools
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub write_pool: ConnectionStats,
    pub read_pool: ConnectionStats,
    pub analytics_pool: ConnectionStats,
}

impl PoolStats {
    /// Get total connections across all pools
    pub const fn total_connections(&self) -> u32 {
        // Use saturating addition for pool statistics - overflow saturates to max value
        self.write_pool
            .size
            .saturating_add(self.read_pool.size)
            .saturating_add(self.analytics_pool.size)
    }

    /// Get total idle connections
    pub const fn total_idle(&self) -> u32 {
        // Use saturating addition for pool statistics - overflow saturates to max value
        self.write_pool
            .idle
            .saturating_add(self.read_pool.idle)
            .saturating_add(self.analytics_pool.idle)
    }

    /// Get utilization percentage
    #[allow(clippy::cast_precision_loss)] // Acceptable precision loss for utilization percentage
    pub fn utilization(&self) -> f32 {
        let total = self.total_connections() as f32;
        let idle = self.total_idle() as f32;
        if total > 0.0 {
            ((total - idle) / total) * 100.0
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_pool_config() {
        let config = PoolConfig::default();
        assert_eq!(config.write_pool_size, 10);
        assert_eq!(config.read_pool_size, 20);
        assert_eq!(config.analytics_pool_size, 5);
    }

    #[test]
    fn test_pool_stats_calculations() {
        let stats = PoolStats {
            write_pool: ConnectionStats {
                size: 5,
                idle: 2,
                max: 10,
            },
            read_pool: ConnectionStats {
                size: 10,
                idle: 5,
                max: 20,
            },
            analytics_pool: ConnectionStats {
                size: 3,
                idle: 1,
                max: 5,
            },
        };

        assert_eq!(stats.total_connections(), 18);
        assert_eq!(stats.total_idle(), 8);
        assert!((stats.utilization() - 55.55).abs() < 0.1);
    }
}
