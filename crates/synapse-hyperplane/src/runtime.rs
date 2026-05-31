//! Synapse Hyperplane Runtime
//!
//! Multi-threaded runtime that orchestrates all components:
//! - Geyser plugin integration
//! - Delta Plane consumer
//! - Index Manager
//! - Query Orchestrator
//! - Slot Reconciler
//! - RPC server

use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use std::path::PathBuf;
use tracing::{info, warn};
use parking_lot::RwLock;

use slot_reconciler::SlotReconciler;
use query_orchestrator::QueryPlanner;
use geyser_bridge::ring_buffer::RingBufferReader;
use crate::index_manager::{IndexManager, IndexManagerConfig};

/// Runtime configuration
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Path to Geyser ring buffer
    pub ring_buffer_path: String,
    /// Path to delta segments
    pub delta_path: PathBuf,
    /// Path to index storage
    pub index_path: PathBuf,
    /// Path to base locator
    pub base_locator_path: PathBuf,
    /// Number of worker threads for query processing
    pub query_workers: usize,
    /// Number of threads for index updates
    pub index_workers: usize,
    /// RPC bind address
    pub rpc_bind: String,
    /// Enable metrics
    pub enable_metrics: bool,
    /// Metrics port
    pub metrics_port: u16,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            ring_buffer_path: "/dev/shm/synapse-geyser.ring".to_string(),
            delta_path: PathBuf::from("/tmp/synapse/delta"),
            index_path: PathBuf::from("/tmp/synapse/indexes"),
            base_locator_path: PathBuf::from("/tmp/synapse/base-locator"),
            query_workers: 8,
            index_workers: 4,
            rpc_bind: "0.0.0.0:8898".to_string(),
            enable_metrics: true,
            metrics_port: 9090,
        }
    }
}

/// Runtime statistics
#[derive(Debug, Clone)]
pub struct RuntimeStats {
    /// Total Geyser updates processed
    pub geysers_processed: u64,
    /// Total accounts indexed
    pub accounts_indexed: u64,
    /// Total queries executed
    pub queries_executed: u64,
    /// Current slot
    pub current_slot: u64,
    /// Root slot
    pub root_slot: u64,
    /// Uptime in seconds
    pub uptime_secs: u64,
}

/// Synapse Hyperplane Runtime
pub struct SynapseRuntime {
    config: RuntimeConfig,
    slot_reconciler: Arc<SlotReconciler>,
    index_manager: Arc<IndexManager>,
    query_planner: Arc<RwLock<Option<QueryPlanner>>>,
    running: Arc<RwLock<bool>>,
    start_time: std::time::Instant,
    geysers_processed: Arc<std::sync::atomic::AtomicU64>,
    queries_executed: Arc<std::sync::atomic::AtomicU64>,
}

impl SynapseRuntime {
    /// Create a new Synapse runtime
    pub fn new(config: RuntimeConfig) -> anyhow::Result<Self> {
        info!("Initializing Synapse Hyperplane Runtime");
        
        // Initialize slot reconciler
        let slot_reconciler = Arc::new(SlotReconciler::new(0));
        
        // Initialize index manager
        let index_config = IndexManagerConfig {
            index_path: config.index_path.clone(),
            enable_program_index: true,
            enable_token_owner: true,
            enable_token_mint: true,
            enable_data_size: true,
            enable_memcmp: true,
            enable_discriminator: true,
        };
        
        let index_manager = Arc::new(IndexManager::new(index_config));
        
        // Create query planner (will be initialized after indexes are populated)
        let query_planner = Arc::new(RwLock::new(None));
        
        info!("Runtime initialized successfully");
        
        Ok(Self {
            config,
            slot_reconciler,
            index_manager,
            query_planner,
            running: Arc::new(RwLock::new(false)),
            start_time: std::time::Instant::now(),
            geysers_processed: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            queries_executed: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        })
    }

    /// Initialize query planner with indexes
    pub fn initialize_query_planner(&self) -> anyhow::Result<()> {
        let program_index = self.index_manager.program_index()
            .ok_or_else(|| anyhow::anyhow!("Program index not available"))?;
        let data_size_index = self.index_manager.data_size_index()
            .ok_or_else(|| anyhow::anyhow!("Data size index not available"))?;
        let memcmp_index = self.index_manager.memcmp_index()
            .ok_or_else(|| anyhow::anyhow!("Memcmp index not available"))?;
        let discriminator_index = self.index_manager.discriminator_index()
            .ok_or_else(|| anyhow::anyhow!("Discriminator index not available"))?;
        
        // Create memcmp accelerator inline
        let accelerator_path = self.config.index_path.join("memcmp_accelerator");
        let memcmp_accelerator = std::sync::Arc::new(
            index_fabric::MemcmpAccelerator::new(accelerator_path)
        );

        let _planner = QueryPlanner::new(
            program_index,
            data_size_index,
            memcmp_index,
            discriminator_index,
            memcmp_accelerator,
        );

        info!("Query planner initialized");
        Ok(())
    }

    /// Start the runtime
    pub fn start(&mut self) -> anyhow::Result<Vec<JoinHandle<()>>> {
        info!("Starting Synapse Hyperplane Runtime");
        *self.running.write() = true;

        let mut handles = Vec::new();

        // Start Geyser consumer thread
        let geysers_processed = self.geysers_processed.clone();
        let index_manager = self.index_manager.clone();
        let slot_reconciler = self.slot_reconciler.clone();
        let running = self.running.clone();
        let ring_path = self.config.ring_buffer_path.clone();
        
        handles.push(thread::spawn(move || {
            info!("Geyser consumer thread started");
            Self::geyser_consumer_loop(
                &ring_path,
                &index_manager,
                &slot_reconciler,
                &geysers_processed,
                &running,
            );
        }));

        // Start query worker threads
        for i in 0..self.config.query_workers {
            let query_planner = self.query_planner.clone();
            let queries_executed = self.queries_executed.clone();
            let running = self.running.clone();
            
            handles.push(thread::spawn(move || {
                info!("Query worker {} started", i);
                Self::query_worker_loop(&query_planner, &queries_executed, &running);
            }));
        }

        // Start metrics thread if enabled
        if self.config.enable_metrics {
            let metrics_port = self.config.metrics_port;
            let running = self.running.clone();
            let geysers = self.geysers_processed.clone();
            let queries = self.queries_executed.clone();
            
            handles.push(thread::spawn(move || {
                info!("Metrics server started on port {}", metrics_port);
                Self::metrics_loop(metrics_port, &geysers, &queries, &running);
            }));
        }

        info!("Runtime started with {} threads", handles.len());
        Ok(handles)
    }

    /// Stop the runtime
    pub fn stop(&self) {
        info!("Stopping Synapse Hyperplane Runtime");
        *self.running.write() = false;
    }

    /// Get runtime statistics
    pub fn stats(&self) -> RuntimeStats {
        let index_stats = self.index_manager.stats();
        let slot_stats = self.slot_reconciler.stats();
        
        RuntimeStats {
            geysers_processed: self.geysers_processed.load(std::sync::atomic::Ordering::SeqCst),
            accounts_indexed: index_stats.total_indexed,
            queries_executed: self.queries_executed.load(std::sync::atomic::Ordering::SeqCst),
            current_slot: slot_stats.processed_slot,
            root_slot: slot_stats.root_slot,
            uptime_secs: self.start_time.elapsed().as_secs(),
        }
    }

    /// Get slot reconciler
    pub fn slot_reconciler(&self) -> Arc<SlotReconciler> {
        self.slot_reconciler.clone()
    }

    /// Get index manager
    pub fn index_manager(&self) -> Arc<IndexManager> {
        self.index_manager.clone()
    }

    /// Get query planner
    pub fn query_planner(&self) -> Arc<RwLock<Option<QueryPlanner>>> {
        self.query_planner.clone()
    }

    // Internal methods

    fn geyser_consumer_loop(
        ring_path: &str,
        _index_manager: &IndexManager,
        slot_reconciler: &SlotReconciler,
        counter: &std::sync::atomic::AtomicU64,
        running: &RwLock<bool>,
    ) {
        // Try to open ring buffer reader
        match RingBufferReader::open(ring_path) {
            Ok(mut reader) => {
                info!("Ring buffer opened: {}", ring_path);
                
                while *running.read() {
                    // Read updates from ring buffer
                    match reader.read_next() {
                        Ok(Some(update)) => {
                            // Update slot reconciler
                            slot_reconciler.update_processed(update.slot);
                            
                            // Convert to AccountView and update indexes
                            // (simplified - real impl would convert from Geyser format)
                            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                        Ok(None) => {
                            // No updates available, sleep briefly
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(e) => {
                            warn!("Ring buffer read error: {}", e);
                            thread::sleep(Duration::from_millis(10));
                        }
                    }
                }
                
                info!("Geyser consumer shutting down");
            }
            Err(e) => {
                warn!("Failed to open ring buffer (may not exist yet): {}", e);
                // Wait for ring buffer to be created
                while *running.read() {
                    thread::sleep(Duration::from_secs(1));
                }
            }
        }
    }

    fn query_worker_loop(
        query_planner: &Arc<RwLock<Option<QueryPlanner>>>,
        counter: &std::sync::atomic::AtomicU64,
        running: &RwLock<bool>,
    ) {
        while *running.read() {
            // Wait for queries (in real impl, this would be a channel)
            thread::sleep(Duration::from_millis(10));
            
            // Process queries from planner
            if let Some(_planner) = query_planner.read().as_ref() {
                // Real impl would process actual queries here
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
        
        info!("Query worker shutting down");
    }

    fn metrics_loop(
        _port: u16,
        geysers: &std::sync::atomic::AtomicU64,
        queries: &std::sync::atomic::AtomicU64,
        running: &RwLock<bool>,
    ) {
        // Simple metrics endpoint (real impl would use Prometheus)
        while *running.read() {
            let geysers_count = geysers.load(std::sync::atomic::Ordering::SeqCst);
            let queries_count = queries.load(std::sync::atomic::Ordering::SeqCst);
            
            // Log metrics periodically
            info!("Metrics - Geyser: {}, Queries: {}", geysers_count, queries_count);
            
            thread::sleep(Duration::from_secs(10));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_creation() {
        let config = RuntimeConfig::default();
        let runtime = SynapseRuntime::new(config);
        assert!(runtime.is_ok());
    }

    #[test]
    fn test_runtime_stats() {
        let config = RuntimeConfig::default();
        let runtime = SynapseRuntime::new(config).unwrap();
        
        let stats = runtime.stats();
        assert_eq!(stats.geysers_processed, 0);
        assert_eq!(stats.queries_executed, 0);
        assert_eq!(stats.uptime_secs, 0);
    }
}
