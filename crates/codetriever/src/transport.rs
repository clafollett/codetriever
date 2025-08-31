//! Transport types for codetriever server
//!
//! This module defines the Transport enum used for configuring
//! MCP protocol transport mechanisms.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Transport mechanism for MCP protocol communication
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// Standard input/output (STDIO) transport - default for local processes
    #[value(name = "stdio")]
    #[default]
    Stdio,

    /// Server-Sent Events (SSE) transport - for HTTP-based communication
    #[value(name = "sse")]
    Sse,
}

impl std::fmt::Display for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Transport::Stdio => write!(f, "stdio"),
            Transport::Sse => write!(f, "sse"),
        }
    }
}

impl std::str::FromStr for Transport {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "stdio" => Ok(Transport::Stdio),
            "sse" => Ok(Transport::Sse),
            _ => Err(format!(
                "Invalid transport: '{s}'. Valid options are: stdio, sse"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_from_str() {
        assert_eq!("stdio".parse::<Transport>().unwrap(), Transport::Stdio);
        assert_eq!("sse".parse::<Transport>().unwrap(), Transport::Sse);
        assert_eq!("STDIO".parse::<Transport>().unwrap(), Transport::Stdio);
        assert_eq!("SSE".parse::<Transport>().unwrap(), Transport::Sse);
        assert!("invalid".parse::<Transport>().is_err());
    }

    #[test]
    fn test_transport_display() {
        assert_eq!(Transport::Stdio.to_string(), "stdio");
        assert_eq!(Transport::Sse.to_string(), "sse");
    }
}
