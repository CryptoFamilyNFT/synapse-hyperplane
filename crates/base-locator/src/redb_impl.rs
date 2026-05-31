//! Redb backend implementation (pure Rust, macOS-friendly)

use super::{deserialize_location, serialize_location, LocatorError, LocatorStats};
use hyperplane_types::AccountLocation;
use parking_lot::RwLock;
use redb::{Database, ReadableTable, TableDefinition};
use solana_sdk::pubkey::Pubkey;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

/// Table definition: pubkey (32 bytes) -> serialized AccountLocation
const LOCATIONS_TABLE: TableDefinition<[u8; 32], &[u8]> = TableDefinition::new("locations");

/// Type alias per comodità
pub type Result<T> = std::result::Result<T, LocatorError>;

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
        
        info!("Opening locator database (redb backend) at {:?}", path);
        
        // Open/create redb database
        let db_path = path.join("locator.redb");
        let db: Database = Database::create(&db_path)?;
        
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
            let location = deserialize_location(value.value())?;
            let mut stats = self.stats.write();
            stats.reads += 1;
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
                let location = deserialize_location(value.value())?;
                results.push(Some(location));
            } else {
                results.push(None);
            }
        }
        
        let mut stats = self.stats.write();
        stats.reads += pubkeys.len() as u64;
        
        Ok(results)
    }

    /// Insert location
    pub fn insert(&self, pubkey: Pubkey, location: AccountLocation) -> Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(LOCATIONS_TABLE)?;
            let bytes = serialize_location(&location)?;
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
                let bytes = serialize_location(location)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use hyperplane_types::StorageType;
    use tempfile::tempdir;

    #[test]
    fn test_basic_operations() {
        let temp_dir = tempdir().unwrap();
        let locator = RocksLocator::open(temp_dir.path()).unwrap();
        
        let pubkey = Pubkey::new_unique();
        let location = AccountLocation {
            file_id: 1,
            offset: 100,
            stored_size: 200,
            data_offset: 50,
            data_len: 150,
            slot: 1000,
            write_version: 1,
            storage_type: StorageType::Base,
        };
        
        // Insert
        locator.insert(pubkey, location).unwrap();
        
        // Get
        let retrieved = locator.get(pubkey).unwrap().unwrap();
        assert_eq!(retrieved.file_id, location.file_id);
        assert_eq!(retrieved.slot, location.slot);
        
        // Count
        let count = locator.count().unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_batch_insert() {
        let temp_dir = tempdir().unwrap();
        let locator = RocksLocator::open(temp_dir.path()).unwrap();
        
        let locations: Vec<_> = (0..100)
            .map(|i| {
                (
                    Pubkey::new_unique(),
                    AccountLocation {
                        file_id: i,
                        offset: i * 100,
                        stored_size: 200,
                        data_offset: 50,
                        data_len: 150,
                        slot: 1000 + i,
                        write_version: i,
                        storage_type: StorageType::Base,
                    },
                )
            })
            .collect();
        
        locator.insert_batch(&locations).unwrap();
        
        let count = locator.count().unwrap();
        assert_eq!(count, 100);
    }
}
