//! RocksDB-backed pubkey dictionary compatibility layer.
//!
//! This module maps the generic `PersistentPubkeyDictionary` name
//! to the RocksDB locator implementation.

#![cfg(feature = "rocksdb-backend")]

pub use super::rocksdb_impl::*;

pub type PersistentPubkeyDictionary = super::rocksdb_impl::RocksLocator;