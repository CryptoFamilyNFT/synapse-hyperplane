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

/// Redb-backed locator
pub struct RocksLocator {
    db: Arc<Database>,
    stats: Arc<RwLock<LocatorStats>>,
}

impl RocksLocator {
    /// Open or create locator database
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        
        // Ensure directory exists
        std::fs::create_dir_all(path)?;
        
        info!("Opening locator database at {:?}", path);
        
        // Open/create redb database
        let db_path = path.join("locator.redb");
        let db = Database::create(&db_path)?;
        
        // Initialize tables
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(LOCATIONS_TABLE)?;
        }
        write_txn.commit()?;
        
        info!("Locator database opened successfully");
        
        Ok(Self {
            db: Arc::new(db),
            stats: Arc::new(RwLock::new(LocatorStats::default())),
        })
    }

    /// Get location for pubkey
    pub fn get(&self, pubkey: Pubkey) -> Result<Option<AccountLocation>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(LOCATIONS_TABLE)?;
        
        if let Some(value) = table.get(&pubkey.to_bytes())? {
            let location = crate::deserialize_location(value.value())?;
            Ok(Some(location))
        } else {
            Ok(None)
        }
    }

    /// Batch get locations for multiple pubkeys
    pub fn get_batch(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<AccountLocation>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(LOCATIONS_TABLE)?;
        
        let mut results = Vec::with_capacity(pubkeys.len());
        for pubkey in pubkeys {
            if let Some(value) = table.get(&pubkey.to_bytes())? {
                let location = crate::deserialize_location(value.value())?;
                results.push(Some(location));
            } else {
                results.push(None);
            }
        }
        
        Ok(results)
    }

    /// Insert location
    pub fn insert(&self, pubkey: Pubkey, location: AccountLocation) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(LOCATIONS_TABLE)?;
            let bytes = crate::serialize_location(&location)?;
            table.insert(&pubkey.to_bytes(), bytes.as_slice())?;
        }
        write_txn.commit()?;
        
        let mut stats = self.stats.write();
        stats.writes += 1;
        
        Ok(())
    }

    /// Batch insert locations
    pub fn insert_batch(&self, locations: &[(Pubkey, AccountLocation)]) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(LOCATIONS_TABLE)?;
            for (pubkey, location) in locations {
                let bytes = crate::serialize_location(location)?;
                let _ = table.insert(&pubkey.to_bytes(), bytes.as_slice());
            }
        }
        write_txn.commit()?;
        
        let mut stats = self.stats.write();
        stats.writes += locations.len() as u64;
        stats.batch_writes += 1;
        
        info!("Batch inserted {} locations", locations.len());
        
        Ok(())
    }

    /// Delete location
    pub fn delete(&self, pubkey: Pubkey) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(LOCATIONS_TABLE)?;
            let _ = table.remove(&pubkey.to_bytes());
        }
        write_txn.commit()?;
        
        Ok(())
    }

    /// Get location count
    pub fn count(&self) -> Result<u64> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(LOCATIONS_TABLE)?;
        
        let mut count = 0u64;
        for _ in table.iter()? {
            count += 1;
        }
        
        let mut stats = self.stats.write();
        stats.total_keys = count;
        
        Ok(count)
    }

    /// Get stats
    pub fn stats(&self) -> LocatorStats {
        self.stats.read().clone()
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
