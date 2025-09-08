//! Builder pattern for `PoolConfig`
//!
//! Provides a fluent API for constructing database pool configurations.

use crate::pool_manager::PoolConfig;

/// Builder for creating `PoolConfig` instances with a fluent API
///
/// # Examples
///
/// ```no_run
/// use codetriever_data::pool_builder::PoolConfigBuilder;
///
/// let config = PoolConfigBuilder::new()
///     .write_pool_size(15)
///     .read_pool_size(30)
///     .analytics_pool_size(10)
///     .connect_timeout(10)
///     .build();
/// ```
#[derive(Debug, Clone)]
#[must_use]
pub struct PoolConfigBuilder {
    write_pool_size: Option<u32>,
    read_pool_size: Option<u32>,
    analytics_pool_size: Option<u32>,
    connect_timeout: Option<u64>,
    idle_timeout: Option<u64>,
    max_lifetime: Option<u64>,
}

impl PoolConfigBuilder {
    /// Create a new `PoolConfigBuilder` with no values set
    pub const fn new() -> Self {
        Self {
            write_pool_size: None,
            read_pool_size: None,
            analytics_pool_size: None,
            connect_timeout: None,
            idle_timeout: None,
            max_lifetime: None,
        }
    }

    /// Set the write pool size (default: 10)
    pub const fn write_pool_size(mut self, size: u32) -> Self {
        self.write_pool_size = Some(size);
        self
    }

    /// Set the read pool size (default: 20)
    pub const fn read_pool_size(mut self, size: u32) -> Self {
        self.read_pool_size = Some(size);
        self
    }

    /// Set the analytics pool size (default: 5)
    pub const fn analytics_pool_size(mut self, size: u32) -> Self {
        self.analytics_pool_size = Some(size);
        self
    }

    /// Set the connection timeout in seconds (default: 5)
    pub const fn connect_timeout(mut self, seconds: u64) -> Self {
        self.connect_timeout = Some(seconds);
        self
    }

    /// Set the idle timeout in seconds (default: 300)
    pub const fn idle_timeout(mut self, seconds: u64) -> Self {
        self.idle_timeout = Some(seconds);
        self
    }

    /// Set the maximum connection lifetime in seconds (default: 1800)
    pub const fn max_lifetime(mut self, seconds: u64) -> Self {
        self.max_lifetime = Some(seconds);
        self
    }

    /// Build the `PoolConfig` with specified values or defaults
    pub fn build(self) -> PoolConfig {
        PoolConfig {
            write_pool_size: self.write_pool_size.unwrap_or(10),
            read_pool_size: self.read_pool_size.unwrap_or(20),
            analytics_pool_size: self.analytics_pool_size.unwrap_or(5),
            connect_timeout: self.connect_timeout.unwrap_or(5),
            idle_timeout: self.idle_timeout.unwrap_or(300),
            max_lifetime: self.max_lifetime.unwrap_or(1800),
        }
    }

    /// Create a configuration optimized for development
    ///
    /// Uses smaller pool sizes to conserve resources
    pub fn development() -> PoolConfig {
        Self::new()
            .write_pool_size(2)
            .read_pool_size(4)
            .analytics_pool_size(2)
            .connect_timeout(10)
            .build()
    }

    /// Create a configuration optimized for production
    ///
    /// Uses larger pool sizes for better performance
    pub fn production() -> PoolConfig {
        Self::new()
            .write_pool_size(20)
            .read_pool_size(40)
            .analytics_pool_size(10)
            .connect_timeout(5)
            .idle_timeout(600)
            .max_lifetime(3600)
            .build()
    }
}

impl Default for PoolConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config_builder_defaults() {
        let config = PoolConfigBuilder::new().build();
        assert_eq!(config.write_pool_size, 10);
        assert_eq!(config.read_pool_size, 20);
        assert_eq!(config.analytics_pool_size, 5);
        assert_eq!(config.connect_timeout, 5);
    }

    #[test]
    fn test_pool_config_builder_custom() {
        let config = PoolConfigBuilder::new()
            .write_pool_size(15)
            .read_pool_size(30)
            .connect_timeout(10)
            .build();

        assert_eq!(config.write_pool_size, 15);
        assert_eq!(config.read_pool_size, 30);
        assert_eq!(config.connect_timeout, 10);
    }

    #[test]
    fn test_development_config() {
        let config = PoolConfigBuilder::development();
        assert_eq!(config.write_pool_size, 2);
        assert_eq!(config.read_pool_size, 4);
    }

    #[test]
    fn test_production_config() {
        let config = PoolConfigBuilder::production();
        assert_eq!(config.write_pool_size, 20);
        assert_eq!(config.read_pool_size, 40);
    }
}
