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

// Moduli PersistentPubkeyDictionary per ogni backend
#[cfg(feature = "redb-backend")]
pub mod pubkey_dict_redb;

#[cfg(feature = "rocksdb-backend")]
pub mod pubkey_dict_rocksdb;

// Re-export backend-specific implementations
#[cfg(feature = "rocksdb-backend")]
pub use rocksdb_impl::*;

#[cfg(feature = "redb-backend")]
pub use redb_impl::*;

// Re-export PersistentPubkeyDictionary dal modulo corretto
#[cfg(feature = "redb-backend")]
pub use pubkey_dict_redb::PersistentPubkeyDictionary;

#[cfg(feature = "rocksdb-backend")]
pub use pubkey_dict_rocksdb::PersistentPubkeyDictionary;

pub use checksum::*;
pub use rocks_locator::{serialize_location, deserialize_location, LocatorError, LocatorStats, Result};

// Common types
pub use hyperplane_types::AccountLocation;
