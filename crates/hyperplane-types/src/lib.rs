//! Core types and data structures for Synapse Hyperplane Accounts Engine
//!
//! This crate defines the fundamental data structures used throughout the engine:
//! - AccountLocation: physical location of an account in storage
//! - AccountView: unified view merging delta and base layers
//! - SlotContext: commitment and slot tracking
//! - Bitmap structures for compressed indexes

pub mod account;
pub mod location;
pub mod slot;
pub mod bitmap;
pub mod rpc;

pub use account::*;
pub use location::*;
pub use slot::*;
pub use bitmap::*;
pub use rpc::*;
