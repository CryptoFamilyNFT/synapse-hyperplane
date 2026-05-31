//! Account File Mapper - Scans Agave /accounts files in read-only mmap mode
//!
//! This crate provides the foundation for reading account data directly from
//! Agave's account storage files without duplicating the data.
//!
//! # Architecture
//!
//! ```text
//! /accounts/*.0, *.1, *.2, ...
//!     ↓
//! MmapAccountFile (zero-copy read)
//!     ↓
//! AccountParser (extract pubkey, owner, data, metadata)
//!     ↓
//! AccountLocation (file_id, offset, size, slot, write_version)
//! ```

pub mod mmap_reader;
pub mod account_parser;
pub mod appendvec_compat;
pub mod scanner;

pub use mmap_reader::*;
pub use account_parser::*;
pub use scanner::*;

/// Re-export core types
pub use hyperplane_types::{AccountLocation, StorageType};
