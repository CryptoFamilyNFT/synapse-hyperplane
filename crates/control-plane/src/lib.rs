//! Control Plane - Observability and administration
//!
//! Provides:
//! - Prometheus metrics
//! - Health checks
//! - Admin API (rebuild, compaction, stats)

pub mod metrics;
pub mod health;
pub mod admin;
