//! RocksDB backend implementation (production-grade for Linux/Ubuntu)
//! 
//! Note: This module is only compiled when `rocksdb-backend` feature is enabled.

#![cfg(feature = "rocksdb-backend")]

use super::{deserialize_location, serialize_location, LocatorError, LocatorStats, Result};
use hyperplane_types::AccountLocation;
use parking_lot::RwLock;
use rocksdb::{
    BlockBasedOptions, Cache, ColumnFamily, ColumnFamilyDescriptor,
    Options, ReadOptions, WriteBatch, WriteOptions, DB,
};
use solana_sdk::pubkey::Pubkey;
use std::path::Path;
use std::sync::Arc;
use tracing::info;

/// Column family names
#[cfg(feature = "rocksdb-backend")]
const CF_LOCATIONS: &str = "locations";
#[cfg(feature = "rocksdb-backend")]
const CF_METADATA: &str = "metadata";

/// RocksDB-backed locator
#[cfg(feature = "rocksdb-backend")]
pub struct RocksLocator {
    db: Arc<DB>,
    write_opts: WriteOptions,
    read_opts: RwLock<ReadOptions>,
#[cfg(feature = "rocksdb-backend")]
    stats: Arc<RwLock<LocatorStats>>,
}

impl RocksLocator {
    /// Open or create locator database
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        
        info!("Opening locator database (RocksDB backend) at {:?}", path);
        
        // Configure block-based options with bloom filters
        let mut block_opts = BlockBasedOptions::default();
        block_opts.set_bloom_filter(10.0, false); // 10 bits per key, block-based
        block_opts.set_block_size(64 * 1024); // 64 KB blocks
        let cache = Cache::new_lru_cache(256 * 1024 * 1024); // 256 MB cache
        block_opts.set_block_cache(&cache); // 256 MB cache
        
        // Configure DB options
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        db_opts.set_block_based_table_factory(&block_opts);
        db_opts.increase_parallelism(num_cpus::get() as i32);
        db_opts.set_max_background_jobs(4);
        db_opts.set_write_buffer_size(256 * 1024 * 1024); // 256 MB memtable
        db_opts.set_max_write_buffer_number(4);
        db_opts.set_target_file_size_base(64 * 1024 * 1024); // 64 MB SST files
        db_opts.set_bytes_per_sync(1024 * 1024); // 1 MB sync rate
        
        // Column families
        let cf_descriptors = vec![
            ColumnFamilyDescriptor::new(CF_LOCATIONS, db_opts.clone()),
            ColumnFamilyDescriptor::new(CF_METADATA, db_opts.clone()),
        ];
        
        // Open DB
        let db = DB::open_cf_descriptors(&db_opts, path, cf_descriptors)?;
        let db = Arc::new(db);
        
        // Write options (sync for durability)
        let mut write_opts = WriteOptions::default();
        write_opts.set_sync(true);
        
        // Read options
        let mut read_opts = ReadOptions::default();
        read_opts.set_verify_checksums(false);
        read_opts.fill_cache(true);
        
        info!("Locator database opened successfully");
        
        Ok(Self {
            db,
            write_opts,
            read_opts: RwLock::new(read_opts),
            stats: Arc::new(RwLock::new(LocatorStats::default())),
        })
    }

    /// Get location for pubkey
    pub fn get(&self, pubkey: Pubkey) -> Result<Option<AccountLocation>> {
        let cf = self.cf_locations();
        let value = self.db.get_cf_opt(cf, &pubkey.to_bytes(), &self.read_opts.read())?;
        
        if let Some(bytes) = value {
            let location = deserialize_location(&bytes)?;
            let mut stats = self.stats.write();
            stats.reads += 1;
            Ok(Some(location))
        } else {
            Ok(None)
        }
    }

    /// Batch get locations for multiple pubkeys
    pub fn get_batch(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<AccountLocation>>> {
        let cf = self.cf_locations();
        let keys: Vec<Vec<u8>> = pubkeys.iter().map(|pk| pk.to_bytes().to_vec()).collect();
        
        let values = self.db.multi_get_cf(keys.iter().map(|k| (cf, k.as_slice())));
        
        let mut results = Vec::with_capacity(values.len());
        for value_result in values {
            match value_result {
                Ok(Some(bytes)) => {
                    let location = deserialize_location(&bytes)?;
                    results.push(Some(location));
                }
                Ok(None) => {
                    results.push(None);
                }
                Err(e) => {
                    return Err(LocatorError::RocksDbError(e));
                }
            }
        }
        
        let mut stats = self.stats.write();
        stats.reads += pubkeys.len() as u64;
        
        Ok(results)
    }

    /// Insert location
    pub fn insert(&self, pubkey: Pubkey, location: AccountLocation) -> Result<()> {
        let cf = self.cf_locations();
        let value = serialize_location(&location)?;
        
        self.db.put_cf_opt(cf, &pubkey.to_bytes(), &value, &self.write_opts)?;
        
        let mut stats = self.stats.write();
        stats.writes += 1;
        
        Ok(())
    }

    /// Batch insert locations
    pub fn insert_batch(&self, locations: &[(Pubkey, AccountLocation)]) -> Result<()> {
        let cf = self.cf_locations();
        let mut batch = WriteBatch::default();
        
        for (pubkey, location) in locations {
            let value = serialize_location(location)?;
            batch.put_cf(cf, &pubkey.to_bytes(), &value);
        }
        
        self.db.write_opt(batch, &self.write_opts)?;
        
        let mut stats = self.stats.write();
        stats.writes += locations.len() as u64;
        stats.batch_writes += 1;
        
        info!("Batch inserted {} locations", locations.len());
        
        Ok(())
    }

    /// Delete location
    pub fn delete(&self, pubkey: Pubkey) -> Result<()> {
        let cf = self.cf_locations();
        self.db.delete_cf_opt(cf, &pubkey.to_bytes(), &self.write_opts)?;
        
        Ok(())
    }

    /// Iterate over all locations
    pub fn iter(&self) -> Result<LocationIterator> {
        let cf = self.cf_locations();
        drop(self.read_opts.read()); // Release lock
        
        let mut read_opts = ReadOptions::default();
        read_opts.set_verify_checksums(false);
        read_opts.fill_cache(true);
        
        let iter = self.db.iterator_cf_opt(cf, read_opts, rocksdb::IteratorMode::Start);
        
        Ok(LocationIterator {
            iter,
            stats: Arc::clone(&self.stats),
        })
    }

    /// Get location count
    pub fn count(&self) -> Result<u64> {
        let cf = self.cf_locations();
        // Use property for faster count
        let count = self
            .db
            .property_int_value_cf(cf, "rocksdb.estimate-num-keys")?
            .unwrap_or(0);
        
        let mut stats = self.stats.write();
        stats.total_keys = count;
        
        Ok(count)
    }

    /// Get stats
    pub fn stats(&self) -> LocatorStats {
        self.stats.read().clone()
    }

    /// Compact database
    pub fn compact(&self) -> Result<()> {
        info!("Compacting locator database");
        let cf = self.cf_locations();
        self.db.compact_range_cf(cf, None::<&[u8]>, None::<&[u8]>);
        info!("Compaction complete");
        Ok(())
    }

    /// Get column family handle
    fn cf_locations(&self) -> &ColumnFamily {
        self.db
            .cf_handle(CF_LOCATIONS)
            .expect("locations CF should exist")
    }

    fn cf_metadata(&self) -> &ColumnFamily {
        self.db
            .cf_handle(CF_METADATA)
            .expect("metadata CF should exist")
    }
}

/// Location iterator for RocksDB
#[cfg(feature = "rocksdb-backend")]
pub struct LocationIterator {
    iter: rocksdb::DBIteratorWithThreadMode<'static, DB>,
    stats: Arc<RwLock<LocatorStats>>,
}

impl Iterator for LocationIterator {
    type Item = Result<(Pubkey, AccountLocation)>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(result) = self.iter.next() {
            match result {
                Ok((key_bytes, value_bytes)) => {
                    let key_slice: &[u8] = &key_bytes;
                    let value_slice: &[u8] = &value_bytes;
                    
                    if key_slice.len() == 32 {
                        let mut pubkey_bytes = [0u8; 32];
                        pubkey_bytes.copy_from_slice(key_slice);
                        let pubkey = Pubkey::from(pubkey_bytes);
                        
                        match deserialize_location(value_slice) {
                            Ok(location) => {
                                let mut stats = self.stats.write();
                                stats.reads += 1;
                                Some(Ok((pubkey, location)))
                            }
                            Err(e) => Some(Err(e)),
                        }
                    } else {
                        Some(Err(LocatorError::SerializationError(format!(
                            "Invalid pubkey length: {}",
                            key_bytes.len()
                        ))))
                    }
                }
                Err(e) => Some(Err(LocatorError::RocksDbError(e))),
            }
        } else {
            None
        }
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
        assert!(count >= 1);
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
        assert!(count >= 100);
    }
}
