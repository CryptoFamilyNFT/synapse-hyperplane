//! Base Locator - Pluggable storage backend for pubkey -> AccountLocation mapping
//!
//! Provides persistent, fast point lookups for the base layer of accounts.
//! Supports two backends via feature flags:
//! - `redb-backend` (default): Pure Rust, ideal for macOS development
//! - `rocksdb-backend`: Production-grade for Linux/Ubuntu deployments

pub mod redb_impl;

#[cfg(feature = "rocksdb-backend")]
pub mod rocksdb_impl;

pub mod checksum;
pub mod rocks_locator;

// Re-export based on active feature
#[cfg(feature = "rocksdb-backend")]
pub use rocksdb_impl::*;

#[cfg(feature = "redb-backend")]
pub use redb_impl::*;

pub use checksum::*;
pub use rocks_locator::{serialize_location, deserialize_location, LocatorError, LocatorStats, Result};

// Common types
pub use hyperplane_types::AccountLocation;
