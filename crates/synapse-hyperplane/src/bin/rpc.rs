//! Synapse RPC Server - Serves account queries
//!
//! RPC read provider mesh for getAccountInfo, getMultipleAccounts, etc.

use clap::Parser;
use rpc_read_provider::{RpcServer, RpcServerConfig};
use std::net::SocketAddr;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser, Debug)]
#[command(name = "synapse-rpc")]
#[command(about = "Synapse Hyperplane RPC Server")]
struct Args {
    /// Bind address
    #[arg(long, default_value = "0.0.0.0:8898")]
    bind: SocketAddr,

    /// Number of worker threads
    #[arg(long, default_value_t = 32)]
    workers: usize,

    /// Max concurrent requests
    #[arg(long, default_value_t = 1000)]
    max_concurrent: usize,

    /// Rate limit per IP (requests/second)
    #[arg(long, default_value_t = 100)]
    rate_limit: usize,

    /// Path to base locator
    #[arg(long, default_value = "/mnt/nvme/synapse/base-locator")]
    locator_path: String,

    /// DragonflyDB URL (optional L2 cache)
    #[arg(long)]
    dragonfly_url: Option<String>,

    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    let level = match args.log_level.as_str() {
        "error" => Level::ERROR,
        "warn" => Level::WARN,
        "info" => Level::INFO,
        "debug" => Level::DEBUG,
        "trace" => Level::TRACE,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(true)
        .with_thread_ids(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    info!("Synapse RPC Server starting");
    info!("Bind address: {}", args.bind);
    info!("Workers: {}", args.workers);

    // TODO: Initialize Hyperplane engine
    // - Load base locator
    // - Initialize cache plane
    // - Connect to delta plane
    // - Initialize index fabric

    // Create server config
    let config = RpcServerConfig {
        bind: args.bind,
        workers: args.workers,
        max_concurrent: args.max_concurrent,
        rate_limit_per_ip: args.rate_limit,
        enable_cors: true,
        health_endpoint: true,
        metrics_endpoint: true,
    };

    // Create and run server
    let server = RpcServer::new(config);
    
    info!("Starting RPC server...");
    info!("Endpoints:");
    info!("  - getAccountInfo");
    info!("  - getMultipleAccounts");
    info!("  - /health");
    info!("  - /metrics");
    
    // TODO: Replace with actual engine integration
    info!("WARNING: Running in stub mode - actual account fetching not yet implemented");
    
    server.run().await.map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

    Ok(())
}
