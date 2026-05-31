//! Synapse Hyperplane Accounts Engine
//!
//! Ultra-low-latency AccountsDB-compatible read engine for Agave 4.0.0

pub use hyperplane_types as types;
pub use account_file_mapper as mapper;
pub use base_locator as locator;
pub use cache_plane as cache;
pub use rpc_read_provider as rpc;
pub use geyser_bridge as geyser;
pub use delta_plane as delta;
pub use index_fabric as index;
pub use query_orchestrator as orchestrator;
pub use slot_reconciler as reconciler;
pub use control_plane as control;

pub mod index_manager;
pub mod runtime;

pub use index_manager::IndexManager;
pub use runtime::{SynapseRuntime, RuntimeConfig, RuntimeStats};
