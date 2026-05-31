//! RPC Read Provider Mesh
//!
//! High-performance RPC server serving account queries
//! via the Synapse Hyperplane engine.
//!
//! # Supported Methods
//!
//! - `getAccountInfo` - Single account lookup with delta-first merge
//! - `getMultipleAccounts` - Batch account fetch with optimization
//! - `getProgramAccounts` - Bitmap-indexed program account queries
//! - `getTokenAccountsByOwner` - Token account queries via bitmap indexes
//! - `getProgramAccountsV2` - Paginated, cursor-based queries for production use

pub mod server;
pub mod methods;
pub mod middleware;

pub use server::*;
pub use methods::*;
pub use middleware::*;
