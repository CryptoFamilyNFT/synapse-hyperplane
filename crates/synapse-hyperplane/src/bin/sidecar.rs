//! Synapse Hyperplane Sidecar - Agave 4.0.0 Integration
//!
//! Main entry point for running Synapse as a sidecar process alongside Agave validator.
//! Provides:
//! - Geyser plugin integration via shared memory ring buffer
//! - Real-time index updates
//! - High-performance getProgramAccounts queries
//! - Multi-threaded query execution
//! - Metrics and monitoring

use clap::Parser;
use synapse_hyperplane::{SynapseRuntime, RuntimeConfig};
use std::net::SocketAddr;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser, Debug)]
#[command(name = "synapse-sidecar")]
#[command(about = "Synapse Hyperplane Sidecar for Agave 4.0.0")]
#[command(long_about = None)]
struct Args {
    /// Path to Geyser ring buffer (shared memory)
    #[arg(long, default_value = "/dev/shm/synapse-geyser.ring")]
    ring_path: String,

    /// Path to delta segment storage
    #[arg(long, default_value = "/mnt/nvme/synapse/delta")]
    delta_path: String,

    /// Path to index storage
    #[arg(long, default_value = "/mnt/nvme/synapse/indexes")]
    index_path: String,

    /// Path to base locator (Agave /accounts)
    #[arg(long, default_value = "/mnt/nvme/synapse/base-locator")]
    base_locator_path: String,

    /// RPC bind address
    #[arg(long, default_value = "0.0.0.0:8898")]
    rpc_bind: SocketAddr,

    /// Number of query worker threads
    #[arg(long, default_value_t = 8)]
    query_workers: usize,

    /// Number of index update threads
    #[arg(long, default_value_t = 4)]
    index_workers: usize,

    /// Enable metrics endpoint
    #[arg(long, default_value_t = true)]
    enable_metrics: bool,

    /// Metrics port (Prometheus-compatible)
    #[arg(long, default_value_t = 9090)]
    metrics_port: u16,

    /// Log level (error, warn, info, debug, trace)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Run in development mode (uses /tmp paths)
    #[arg(long)]
    dev: bool,
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
        .with_thread_names(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    info!("Synapse Hyperplane Sidecar starting...");
    info!("Version: 0.1.0");
    info!("Agave Compatibility: 4.0.0");

    // Create runtime config
    let config = if args.dev {
        info!("Running in development mode");
        RuntimeConfig {
            ring_buffer_path: "/tmp/synapse/ring.bin".to_string(),
            delta_path: std::path::PathBuf::from("/tmp/synapse/delta"),
            index_path: std::path::PathBuf::from("/tmp/synapse/indexes"),
            base_locator_path: std::path::PathBuf::from("/tmp/synapse/base-locator"),
            query_workers: args.query_workers,
            index_workers: args.index_workers,
            rpc_bind: args.rpc_bind.to_string(),
            enable_metrics: args.enable_metrics,
            metrics_port: args.metrics_port,
        }
    } else {
        RuntimeConfig {
            ring_buffer_path: args.ring_path,
            delta_path: std::path::PathBuf::from(args.delta_path),
            index_path: std::path::PathBuf::from(args.index_path),
            base_locator_path: std::path::PathBuf::from(args.base_locator_path),
            query_workers: args.query_workers,
            index_workers: args.index_workers,
            rpc_bind: args.rpc_bind.to_string(),
            enable_metrics: args.enable_metrics,
            metrics_port: args.metrics_port,
        }
    };

    info!("Configuration:");
    info!("  Ring buffer: {}", config.ring_buffer_path);
    info!("  Delta path: {:?}", config.delta_path);
    info!("  Index path: {:?}", config.index_path);
    info!("  Base locator: {:?}", config.base_locator_path);
    info!("  RPC bind: {}", config.rpc_bind);
    info!("  Query workers: {}", config.query_workers);
    info!("  Index workers: {}", config.index_workers);
    info!("  Metrics: {} (port {})", config.enable_metrics, config.metrics_port);

    // Create runtime
    let mut runtime = SynapseRuntime::new(config.clone())?;

    // Initialize query planner
    runtime.initialize_query_planner()?;

    // Start runtime
    let handles = runtime.start()?;

    info!("Synapse Hyperplane Sidecar is ready!");
    info!("Listening for Geyser updates on: {}", config.ring_buffer_path);
    info!("RPC endpoint available at: http://{}", config.rpc_bind);

    if config.enable_metrics {
        info!("Metrics endpoint: http://{}:{}/metrics", config.rpc_bind.split(':').next().unwrap(), config.metrics_port);
    }

    // Wait for shutdown signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
        _ = async {
            // Monitor worker threads
            for handle in &handles {
                if handle.is_finished() {
                    return;
                }
            }
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                
                // Log periodic stats
                let stats = runtime.stats();
                info!("Runtime stats:");
                info!("  Geyser updates: {}", stats.geysers_processed);
                info!("  Accounts indexed: {}", stats.accounts_indexed);
                info!("  Queries executed: {}", stats.queries_executed);
                info!("  Current slot: {}", stats.current_slot);
                info!("  Root slot: {}", stats.root_slot);
                info!("  Uptime: {}s", stats.uptime_secs);
            }
        } => {
            info!("Worker thread finished unexpectedly");
        }
    }

    // Shutdown
    info!("Shutting down Synapse Hyperplane Sidecar...");
    runtime.stop();

    // Wait for threads to finish
    for handle in handles {
        handle.join().ok();
    }

    info!("Synapse Hyperplane Sidecar stopped");

    Ok(())
}
