//! Configuration profiles for different environments

/// Configuration profiles for different deployment environments
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Profile {
    /// Development environment - debug settings, fast iteration
    #[serde(rename = "development")]
    Development,

    /// Staging environment - production-like but with debug features
    #[serde(rename = "staging")]
    Staging,

    /// Production environment - optimized for performance and reliability
    #[serde(rename = "production")]
    Production,

    /// Test environment - minimal setup for fast testing
    #[serde(rename = "test")]
    Test,
}

impl Default for Profile {
    fn default() -> Self {
        Self::Development
    }
}

impl std::fmt::Display for Profile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Development => "development",
            Self::Staging => "staging",
            Self::Production => "production",
            Self::Test => "test",
        };
        write!(f, "{name}")
    }
}

impl std::str::FromStr for Profile {
    type Err = crate::ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "development" | "dev" => Ok(Self::Development),
            "staging" => Ok(Self::Staging),
            "production" | "prod" => Ok(Self::Production),
            "test" => Ok(Self::Test),
            _ => Err(crate::ConfigError::MissingField {
                field: format!("Invalid profile: {s}"),
            }),
        }
    }
}
