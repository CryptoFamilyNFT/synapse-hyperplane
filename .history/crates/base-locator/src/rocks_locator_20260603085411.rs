//! Shared types and utilities for locator implementations
//!
//! This module contains common types used by both redb and rocksdb backends.

use hyperplane_types::{AccountLocation, StorageType};
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
    bytes.extend_from_slice(&location.file_id.to_le_bytes()); // 8 bytes
    bytes.extend_from_slice(&location.offset.to_le_bytes()); // 8 bytes
    bytes.extend_from_slice(&location.stored_size.to_le_bytes()); // 4 bytes
    bytes.extend_from_slice(&location.data_offset.to_le_bytes()); // 4 bytes
    bytes.extend_from_slice(&location.data_len.to_le_bytes()); // 4 bytes
    bytes.extend_from_slice(&location.slot.to_le_bytes()); // 8 bytes
    bytes.extend_from_slice(&location.write_version.to_le_bytes()); // 8 bytes
    let storage_type_byte = match location.storage_type {
        StorageType::Base => 0u8,
        StorageType::Delta => 1u8,
        StorageType::Compacted => 2u8,
    };
    bytes.push(storage_type_byte); // 1 byte
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
    
    let file_id = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let offset = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let stored_size = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let data_offset = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    let data_len = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let slot = u64::from_le_bytes(bytes[28..36].try_into().unwrap());
    let write_version = u64::from_le_bytes(bytes[36..44].try_into().unwrap());
    let storage_type_byte = bytes[44];
    
    let storage_type = match storage_type_byte {
        0 => StorageType::Base,
        1 => StorageType::Delta,
        2 => StorageType::Compacted,
        _ => {
            return Err(LocatorError::SerializationError(format!(
                "Invalid storage type: {}",
                storage_type_byte
            )));
        }
    };
    
    Ok(AccountLocation {
        file_id,
        offset,
        stored_size,
        data_offset,
        data_len,
        slot,
        write_version,
        storage_type,
    })
}

