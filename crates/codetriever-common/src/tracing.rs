use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Correlation ID type for tracking operations across service boundaries
///
/// Uses UUID v4 for guaranteed uniqueness across distributed systems
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CorrelationId(Uuid);

impl CorrelationId {
    /// Generate a new correlation ID using UUID v4
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for CorrelationId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<&str> for CorrelationId {
    fn from(id: &str) -> Self {
        Uuid::try_parse(id).map_or_else(|_| Self(Uuid::new_v4()), Self)
    }
}
