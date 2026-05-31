//! RocksDB-backed pubkey dictionary (stub for compatibility)
//! 
//! This module provides RocksDB storage for pubkey -> AccountLocation mapping.
//! For production use, use the main rocksdb_impl.rs instead.

#![cfg(feature = "rocksdb-backend")]

// Re-export from rocksdb_impl to avoid duplication
pub use super::rocksdb_impl::*;
