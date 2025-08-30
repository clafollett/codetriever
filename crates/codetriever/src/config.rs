//! Configuration module for the generated server

// Internal imports (std, crate)
use crate::transport::Transport;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Server configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Log directory
    #[serde(default = "default_log_dir")]
    pub log_dir: PathBuf,
    /// Base API URL
    #[serde(default = "default_api_url")]
    pub api_url: String,
    /// Transport type (stdio or sse)
    #[serde(default)]
    pub transport: Transport,
    /// SSE server address
    #[serde(default = "default_sse_addr")]
    pub sse_addr: std::net::SocketAddr,
    /// SSE keep alive duration in seconds
    #[serde(
        default = "default_sse_keep_alive",
        deserialize_with = "deserialize_duration_secs"
    )]
    pub sse_keep_alive: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_dir: default_log_dir(),
            api_url: default_api_url(),
            transport: Transport::default(),
            sse_addr: default_sse_addr(),
            sse_keep_alive: Duration::from_secs(30),
        }
    }
}

// Default value functions for serde
fn default_log_dir() -> PathBuf {
    PathBuf::from("logs")
}

fn default_api_url() -> String {
    "http://localhost:8080".to_string()
}

fn default_sse_addr() -> std::net::SocketAddr {
    "127.0.0.1:8080"
        .parse()
        .expect("Default SSE address should be valid")
}

fn default_sse_keep_alive() -> Duration {
    Duration::from_secs(30)
}

fn deserialize_duration_secs<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let secs = u64::deserialize(deserializer)?;
    Ok(Duration::from_secs(secs))
}
