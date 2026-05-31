//! Query Orchestrator - getProgramAccounts Query Planner
//!
//! Orchestrates complex getProgramAccounts queries using bitmap indexes:
//! - Bitmap intersection for filtering
//! - DataSize index filtering
//! - Memcmp index filtering
//! - Discriminator index (Anchor types)
//! - Query cost estimation con cardinalità
//! - Optimized execution order (più selettivi prima)
//! - Pagination support

pub mod query_planner;
pub mod bitmap_intersection;
pub mod cost_estimator;
pub mod pagination;
pub mod cost_model;  // NEW: Cost model con statistics e cardinalità
pub mod types;  // NEW: Tipi condivisi (GpaFilter, etc.)

pub use query_planner::*;
pub use bitmap_intersection::*;
pub use cost_estimator::*;
pub use pagination::*;
pub use cost_model::*;  // NEW: Export cost model types
pub use types::*;  // NEW: Export tipi condivisi
