//! Synapse Base Scanner - Scans Agave /accounts files
//!
//! Bootstrap tool for initial account file scanning and locator building.

use account_file_mapper::{AccountFileScanner, ScannerConfig};
use base_locator::{RocksLocator, PersistentPubkeyDictionary};
use clap::Parser;
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser, Debug)]
#[command(name = "synapse-base-scanner")]
#[command(about = "Scan Agave /accounts files and build base locator")]
struct Args {
    /// Path to Agave accounts directory
    #[arg(long, default_value = "/mnt/accounts")]
    accounts_path: PathBuf,

    /// Output path for base locator
    #[arg(long, default_value = "/mnt/nvme/synapse/base-locator")]
    locator_path: PathBuf,

    /// Output path for pubkey dictionary
    #[arg(long, default_value = "/mnt/nvme/synapse/pubkey-dict")]
    dictionary_path: PathBuf,

    /// Number of scan threads
    #[arg(long, default_value_t = num_cpus::get())]
    num_threads: usize,

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

    info!("Synapse Base Scanner starting");
    info!("Accounts path: {:?}", args.accounts_path);
    info!("Locator path: {:?}", args.locator_path);
    info!("Dictionary path: {:?}", args.dictionary_path);

    // Create scanner
    let config = ScannerConfig {
        accounts_path: args.accounts_path.clone(),
        num_threads: args.num_threads,
        detect_appendvec: false,
        validate_accounts: true,
        progress_interval: 100,
    };

    let scanner = AccountFileScanner::new(config);

    // Scan accounts
    info!("Starting account file scan...");
    let result = scanner.scan()?;

    info!(
        "Scan complete: {} accounts from {} files in {}ms",
        result.stats.total_accounts,
        result.stats.files_scanned,
        result.total_duration_ms
    );

    // Build and save locator
    info!("Building base locator...");
    let locator = RocksLocator::open(&args.locator_path)?;
    
    let locations: Vec<(solana_sdk::pubkey::Pubkey, hyperplane_types::AccountLocation)> = result
        .accounts
        .iter()
        .map(|acc| (acc.pubkey, acc.location))
        .collect();

    info!("Inserting {} locations into locator...", locations.len());
    locator.insert_batch(&locations)?;

    let count = locator.count()?;
    info!("Locator contains {} entries", count);

    // Build and save pubkey dictionary
    info!("Building pubkey dictionary...");
    let dict = PersistentPubkeyDictionary::open(&args.dictionary_path)?;
    
    let pubkeys: Vec<_> = result.accounts.iter().map(|acc| acc.pubkey).collect();
    let _ids = dict.insert_batch(&pubkeys);
    
    info!("Dictionary contains {} entries", dict.len());

    info!("Base scanner complete");
    info!("Next steps:");
    info!("  1. Start synapse-delta-plane for live Geyser updates");
    info!("  2. Start synapse-rpc for query serving");

    Ok(())
}

// Helper: get number of CPUs
mod num_cpus {
    pub fn get() -> usize {
        std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4)
    }
}
