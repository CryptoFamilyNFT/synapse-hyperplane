//! Base Locator - Pluggable storage backend for pubkey -> AccountLocation mapping
//!
//! Provides persistent, fast point lookups for the base layer of accounts.
//! Supports two backends via feature flags:
//! - `redb-backend` (default): Pure Rust, ideal for macOS development
//! - `rocksdb-backend`: Production-grade for Linux/Ubuntu deployments

#[cfg(all(feature = "redb-backend", feature = "rocksdb-backend"))]
compile_error!(
    "base-locator: only one backend can be enabled at a time. Use either `redb-backend` or `rocksdb-backend`, not both."
);
#[cfg(feature = "redb-backend")]
pub mod redb_impl;

#[cfg(feature = "rocksdb-backend")]
pub mod rocksdb_impl;

pub mod checksum;
pub mod rocks_locator;

#[cfg(feature = "redb-backend")]
pub mod pubkey_dict_redb;

#[cfg(feature = "rocksdb-backend")]
pub mod pubkey_dict_rocksdb;

#[cfg(feature = "redb-backend")]
pub use redb_impl::*;

#[cfg(feature = "rocksdb-backend")]
pub use rocksdb_impl::*;

#[cfg(feature = "redb-backend")]
pub use pubkey_dict_redb::PersistentPubkeyDictionary;

#[cfg(feature = "rocksdb-backend")]
pub use pubkey_dict_rocksdb::PersistentPubkeyDictionary;

pub use checksum::*;
pub use rocks_locator::{
    deserialize_location,
    serialize_location,
    LocatorError,
    LocatorStats,
    Result,
};

pub use hyperplane_types::AccountLocation;