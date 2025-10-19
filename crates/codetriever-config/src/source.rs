//! Configuration source loading and composition

use crate::validation::Validate;
use crate::{ApplicationConfig, ConfigResult};
use std::path::Path;

/// Trait for loading configuration from different sources
pub trait ConfigurationSource {
    /// Load configuration from this source
    ///
    /// # Errors
    /// Returns configuration loading errors
    fn load(&self) -> ConfigResult<ApplicationConfig>;

    /// Get the name of this configuration source
    fn name(&self) -> &str;

    /// Get the priority of this source (higher number = higher priority)
    fn priority(&self) -> u8;
}

/// Load configuration from environment variables
pub struct EnvironmentSource;

impl ConfigurationSource for EnvironmentSource {
    fn load(&self) -> ConfigResult<ApplicationConfig> {
        // Load configuration from environment variables with safe defaults
        Ok(ApplicationConfig::from_env())
    }

    fn name(&self) -> &'static str {
        "environment" // Human-readable name for debugging/logging
    }

    fn priority(&self) -> u8 {
        100 // Highest priority - environment variables override everything
    }
}

/// Load configuration from TOML file
pub struct TomlFileSource {
    path: std::path::PathBuf,
}

impl TomlFileSource {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl ConfigurationSource for TomlFileSource {
    fn load(&self) -> ConfigResult<ApplicationConfig> {
        // Read TOML file from filesystem with proper error propagation
        let content = std::fs::read_to_string(&self.path)?;

        // Parse TOML content into ApplicationConfig struct
        // Uses serde for type-safe deserialization with validation
        let config: ApplicationConfig = toml::from_str(&content)?;
        Ok(config)
    }

    fn name(&self) -> &'static str {
        "toml_file" // Human-readable name for debugging/logging
    }

    fn priority(&self) -> u8 {
        50 // Medium priority - below env vars, above defaults
    }
}

/// Type alias for configuration sources
type ConfigSources = Vec<Box<dyn ConfigurationSource>>;

/// Configuration loader that combines multiple sources
pub struct ConfigurationLoader {
    sources: ConfigSources,
}

impl ConfigurationLoader {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    #[must_use]
    pub fn add_source(mut self, source: Box<dyn ConfigurationSource>) -> Self {
        self.sources.push(source);
        self
    }

    /// Load configuration from all sources with priority ordering
    ///
    /// # Errors
    /// Returns configuration loading or validation errors
    pub fn load(&self) -> ConfigResult<ApplicationConfig> {
        // Start with default configuration
        let mut config = ApplicationConfig::from_env();

        // Sort sources by priority (lowest first, so highest priority overwrites)
        let mut sorted_sources = self.sources.iter().collect::<Vec<_>>();
        sorted_sources.sort_by_key(|source| source.priority());

        // Apply each source in priority order
        for source in sorted_sources {
            match source.load() {
                Ok(source_config) => {
                    tracing::debug!("Loaded configuration from source: {}", source.name());
                    config = merge_configs(&config, source_config);
                }
                Err(e) => {
                    tracing::warn!("Failed to load from source {}: {}", source.name(), e);
                }
            }
        }

        config.validate()?;
        Ok(config)
    }
}

impl Default for ConfigurationLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Merge two configurations, with the second taking precedence
fn merge_configs(
    _base: &ApplicationConfig,
    override_config: ApplicationConfig,
) -> ApplicationConfig {
    // Merge configurations with override taking precedence
    // This allows partial configs to override only specific fields
    tracing::trace!("Merging configuration with override from environment");

    // For now, we use complete replacement since our environment source
    // loads complete configurations. Future enhancement could implement
    // field-by-field merging for partial TOML files or CLI overrides.
    override_config
}
