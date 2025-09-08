//! Shared response types and traits for API consistency

use serde::{Deserialize, Serialize};
use std::fmt;

/// Status enum for API responses
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ResponseStatus {
    #[default]
    Success,
    Error,
    Processing,
    PartialSuccess,
}

impl fmt::Display for ResponseStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Error => write!(f, "error"),
            Self::Processing => write!(f, "processing"),
            Self::PartialSuccess => write!(f, "partial_success"),
        }
    }
}

impl From<ResponseStatus> for String {
    fn from(status: ResponseStatus) -> Self {
        status.to_string()
    }
}

/// Trait for consistent status field across all API responses
pub trait HasStatus {
    fn status(&self) -> ResponseStatus;
    fn set_status(&mut self, status: ResponseStatus);

    fn is_success(&self) -> bool {
        matches!(
            self.status(),
            ResponseStatus::Success | ResponseStatus::PartialSuccess
        )
    }

    fn is_error(&self) -> bool {
        matches!(self.status(), ResponseStatus::Error)
    }
}

/// Helper macro to implement `HasStatus` trait for response types
#[macro_export]
macro_rules! impl_has_status {
    ($type:ty) => {
        impl $crate::routes::response::HasStatus for $type {
            fn status(&self) -> $crate::routes::response::ResponseStatus {
                self.status
            }

            fn set_status(&mut self, status: $crate::routes::response::ResponseStatus) {
                self.status = status;
            }
        }
    };
}
