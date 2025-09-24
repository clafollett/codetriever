//! Main entry point for the generated Axum MCP server

// Internal modules
mod common;
mod config;
mod handlers;
mod server;
mod signal;
mod transport;

// Internal imports (std, crate)
use crate::config::Config;
use crate::transport::Transport;
use std::sync::Arc;
use tokio::sync::Mutex;

// External imports (alphabetized)
use clap::Parser;
use log::debug;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::writer::MakeWriterExt;

/// codetriever MCP Server
///
/// Supports both STDIO and SSE (Server-Sent Events) transports for MCP protocol
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Transport type to use (stdio or sse)
    #[arg(short, long, value_enum, default_value_t = Transport::Stdio)]
    transport: Transport,

    /// SSE server bind address
    #[arg(long, default_value = "127.0.0.1:8080")]
    sse_addr: String,

    /// SSE keep-alive interval in seconds
    #[arg(long, default_value = "30")]
    sse_keep_alive: u64,

    /// Log directory path (defaults to OS-specific location)
    #[arg(long)]
    log_dir: Option<String>,

    /// API URL for backend services
    #[arg(long, default_value = "http://localhost:8080")]
    api_url: String,

    /// Optional configuration file path (TOML format)
    #[arg(long, short = 'c')]
    config_file: Option<String>,
}

// Type alias to simplify return type
type BoxError = Box<dyn std::error::Error>;

#[tokio::main]
async fn main() -> Result<(), BoxError> {
    debug!("[codetriever MCP] main() reached ===");

    // Parse command line arguments
    let args = Args::parse();

    // Build configuration from command-line arguments
    let mut config = if let Some(config_path) = &args.config_file {
        // Load from config file if specified
        let contents = std::fs::read_to_string(config_path)
            .map_err(|e| format!("Failed to read config file '{config_path}': {e}"))?;
        toml::from_str::<Config>(&contents)
            .map_err(|e| format!("Failed to parse config file '{config_path}': {e}"))?
    } else {
        // Use defaults
        Config::default()
    };

    // Command-line arguments always override config file settings
    config.transport = args.transport;
    config.api_url = args.api_url;
    config.log_dir = if let Some(log_dir) = args.log_dir {
        std::path::PathBuf::from(log_dir)
    } else {
        get_default_log_dir()
    };

    // Parse and apply SSE address
    config.sse_addr = args
        .sse_addr
        .parse()
        .map_err(|e| {
            tracing::error!("Invalid SSE address '{}': {}", args.sse_addr, e);
            tracing::warn!("Using default address: 127.0.0.1:8080");
        })
        .unwrap_or_else(|_| {
            "127.0.0.1:8080"
                .parse()
                .expect("Default SSE address should be valid")
        });

    config.sse_keep_alive = std::time::Duration::from_secs(args.sse_keep_alive);

    let cfg = Arc::new(Mutex::new(config));

    // Get log directory from config
    let log_dir = {
        let cfg_guard = cfg.lock().await;
        cfg_guard.log_dir.clone()
    };

    // Create log directory after releasing the lock
    std::fs::create_dir_all(&log_dir)?;

    // === Dual Logging Setup (configurable) ===
    // 1. File logger (daily rotation, async non-blocking)
    let file_appender = RollingFileAppender::new(Rotation::DAILY, &log_dir, "codetriever-mcp.log");
    let (file_writer, file_guard): (NonBlocking, WorkerGuard) =
        tracing_appender::non_blocking(file_appender);

    // 2. Stderr logger (async non-blocking)
    let (stderr_writer, stderr_guard): (NonBlocking, WorkerGuard) =
        tracing_appender::non_blocking(std::io::stderr());
    // IMPORTANT: Keep file_guard and stderr_guard alive for the duration of main() to prevent premature shutdown of logging and stdio, especially in Docker or MCP stdio mode.

    // 3. Combine writers using .and()
    let multi_writer = file_writer.and(stderr_writer);

    tracing_subscriber::fmt()
        .json()
        .with_writer(multi_writer)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    debug!("[codetriever MCP] After tracing_subscriber setup");

    // Run unified server orchestrator (handles transport, hot reload, shutdown)
    server::start(cfg.clone(), file_guard, stderr_guard).await
}

/// Get the default log directory based on the operating system
fn get_default_log_dir() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        // Windows: %LOCALAPPDATA%\codetriever\logs
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            std::path::PathBuf::from(local_app_data)
                .join("codetriever")
                .join("logs")
        } else {
            // Fallback to current directory
            std::path::PathBuf::from("logs")
        }
    }

    #[cfg(target_os = "macos")]
    {
        // macOS: ~/Library/Logs/codetriever
        if let Some(home) = dirs::home_dir() {
            home.join("Library").join("Logs").join("codetriever")
        } else {
            // Fallback to current directory
            std::path::PathBuf::from("logs")
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // Linux and other Unix-like systems
        // Try /var/log first (if we have permissions), otherwise use user directory
        let system_log_dir = std::path::Path::new("/var/log/codetriever");

        if system_log_dir.exists() && is_writable(system_log_dir) {
            system_log_dir.to_path_buf()
        } else if let Some(data_dir) = dirs::data_dir() {
            // Use ~/.local/share/codetriever/logs
            data_dir.join("codetriever").join("logs")
        } else if let Some(home) = dirs::home_dir() {
            // Fallback to ~/.codetriever/logs
            home.join(".codetriever").join("logs")
        } else {
            // Last resort: current directory
            std::path::PathBuf::from("logs")
        }
    }
}

/// Check if a directory is writable
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn is_writable(path: &std::path::Path) -> bool {
    // Try to create a temporary file to test write permissions
    if let Ok(temp_file) = tempfile::tempfile_in(path) {
        // Clean up is automatic when temp_file is dropped
        drop(temp_file);
        true
    } else {
        false
    }
}
