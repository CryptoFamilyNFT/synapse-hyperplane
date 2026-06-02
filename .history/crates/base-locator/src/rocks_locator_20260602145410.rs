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

    /// Compact database (redb auto-compacts)
    pub fn compact(&self) -> Result<()> {
        info!("Locator database auto-compaction managed by redb");
        Ok(())
    }
}

/// Serialize AccountLocation to bytes
pub fn serialize_location(location: &AccountLocation) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(45);
    
    // file_id (u64, 8 bytes)
    bytes.extend_from_slice(&location.file_id.to_le_bytes());
    // offset (u64, 8 bytes)
    bytes.extend_from_slice(&location.offset.to_le_bytes());
    // stored_size (u32, 4 bytes)
    bytes.extend_from_slice(&location.stored_size.to_le_bytes());
    // data_offset (u32, 4 bytes)
    bytes.extend_from_slice(&location.data_offset.to_le_bytes());
    // data_len (u32, 4 bytes)
    bytes.extend_from_slice(&location.data_len.to_le_bytes());
    // slot (u64, 8 bytes)
    bytes.extend_from_slice(&location.slot.to_le_bytes());
    // write_version (u64, 8 bytes)
    bytes.extend_from_slice(&location.write_version.to_le_bytes());
    // storage_type (u8, 1 byte)
    let storage_type_byte = match location.storage_type {
        hyperplane_types::StorageType::Base => 0u8,
        hyperplane_types::StorageType::Delta => 1u8,
        hyperplane_types::StorageType::Compacted => 2u8,
    };
    bytes.push(storage_type_byte);
    
    Ok(bytes)
}

/// Deserialize AccountLocation from bytes
pub fn deserialize_location(bytes: &[u8]) -> Result<AccountLocation> {
    if bytes.len() < 45 {
        return Err(LocatorError::SerializationError(format!(
            "Invalid location bytes: expected >= 45, got {}",
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
        0 => hyperplane_types::StorageType::Base,
        1 => hyperplane_types::StorageType::Delta,
        2 => hyperplane_types::StorageType::Compacted,
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_locator_insert_and_get() {
        let temp_dir = TempDir::new().unwrap();
        let locator = RocksLocator::open(temp_dir.path()).unwrap();
        
        let pubkey = Pubkey::new_unique();
        let location = AccountLocation::new_base(1, 100, 200, 0, 200, 100, 1);
        
        // Insert
        locator.insert(pubkey, location).unwrap();
        
        // Get
        let retrieved = locator.get(pubkey).unwrap().unwrap();
        assert_eq!(retrieved.file_id, 1);
        assert_eq!(retrieved.offset, 100);
        assert_eq!(retrieved.slot, 100);
    }

    #[test]
    fn test_locator_batch_insert() {
        let temp_dir = TempDir::new().unwrap();
        let locator = RocksLocator::open(temp_dir.path()).unwrap();
        
        let locations: Vec<(Pubkey, AccountLocation)> = (0..100)
            .map(|i| {
                (
                    Pubkey::new_unique(),
                    AccountLocation::new_base(1, i * 1000, 100, 0, 100, 100, i),
                )
            })
            .collect();
        
        locator.insert_batch(&locations).unwrap();
        
        // Verify count
        let count = locator.count().unwrap();
        assert_eq!(count, 100);
        
        // Verify random lookups
        for (pubkey, expected_loc) in &locations {
            let retrieved = locator.get(*pubkey).unwrap().unwrap();
            assert_eq!(retrieved.slot, expected_loc.slot);
        }
    }

    #[test]
    fn test_serialization_roundtrip() {
        let location = AccountLocation::new_base(123, 456, 789, 0, 789, 999, 5);
        
        let bytes = serialize_location(&location).unwrap();
        let deserialized = deserialize_location(&bytes).unwrap();
        
        assert_eq!(location.file_id, deserialized.file_id);
        assert_eq!(location.offset, deserialized.offset);
        assert_eq!(location.slot, deserialized.slot);
        assert_eq!(location.storage_type, deserialized.storage_type);
    }
}
