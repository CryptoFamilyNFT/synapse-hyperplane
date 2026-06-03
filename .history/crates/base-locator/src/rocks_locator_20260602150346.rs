//! Shared types and utilities for locator implementations
//!
//! This module contains common types used by both redb and rocksdb backends.

use hyperplane_types::AccountLocation;
use solana_sdk::pubkey::Pubkey;
use thiserror::Error;

/// Locator errors
#[derive(Debug, Error)]
pub enum LocatorError {
    #[cfg(feature = "redb-backend")]
    #[error("Redb error: {0}")]
    RedbError(#[from] redb::Error),
    
    #[cfg(feature = "redb-backend")]
    #[error("Database error: {0}")]
    DatabaseError(#[from] redb::DatabaseError),
    
    #[cfg(feature = "redb-backend")]
    #[error("Table error: {0}")]
    TableError(#[from] redb::TableError),
    
    #[cfg(feature = "redb-backend")]
    #[error("Storage error: {0}")]
    StorageError(#[from] redb::StorageError),
    
    #[cfg(feature = "redb-backend")]
    #[error("Transaction error: {0}")]
    TransactionError(#[from] redb::TransactionError),
    
    #[cfg(feature = "redb-backend")]
    #[error("Commit error: {0}")]
    CommitError(#[from] redb::CommitError),
    
    #[cfg(feature = "rocksdb-backend")]
    #[error("RocksDB error: {0}")]
    RocksDbError(#[from] rocksdb::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Location not found for pubkey {0}")]
    NotFound(Pubkey),
    
    #[error("Database not initialized")]
    NotInitialized,
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result type
pub type Result<T> = std::result::Result<T, LocatorError>;

/// Locator statistics
#[derive(Debug, Clone, Default)]
pub struct LocatorStats {
    pub total_keys: u64,
    pub reads: u64,
    pub writes: u64,
    pub batch_writes: u64,
}

/// Serialize AccountLocation to bytes (45 bytes total)
pub fn serialize_location(location: &AccountLocation) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(45);
    bytes.extend_from_slice(&location.slot.to_le_bytes()); // 8 bytes
    bytes.extend_from_slice(&location.offset.to_le_bytes()); // 8 bytes
    bytes.extend_from_slice(&location.file_id.to_le_bytes()); // 8 bytes
    bytes.extend_from_slice(&location.data_size.to_le_bytes()); // 8 bytes
    bytes.extend_from_slice(&location.owner_index.to_le_bytes()); // 4 bytes
    bytes.extend_from_slice(&location.program_index.to_le_bytes()); // 4 bytes
    bytes.push(location.flags); // 1 byte
    Ok(bytes)
}

/// Deserialize AccountLocation from bytes
pub fn deserialize_location(bytes: &[u8]) -> Result<AccountLocation> {
    if bytes.len() != 45 {
        return Err(LocatorError::SerializationError(format!(
            "Expected 45 bytes, got {}",
            bytes.len()
        )));
    }
    
    let slot = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let offset = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let file_id = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let data_size = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
    let owner_index = u32::from_le_bytes(bytes[32..36].try_into().unwrap());
    let program_index = u32::from_le_bytes(bytes[36..40].try_into().unwrap());
    let flags = bytes[40];
    
    Ok(AccountLocation {
        slot,
        offset,
        file_id,
        data_size,
        owner_index,
        program_index,
        flags,
    })
}

