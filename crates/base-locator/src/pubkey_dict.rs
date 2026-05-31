//! Pubkey Dictionary - Persistent storage for pubkey <-> pubkey_id mapping
//!
//! Extends the in-memory PubkeyDictionary with RocksDB persistence.

use hyperplane_types::PubkeyDictionary;
use parking_lot::RwLock;
use rocksdb::{Options, WriteBatch, WriteOptions, DB};
use solana_sdk::pubkey::Pubkey;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tracing::info;

/// Dictionary errors
#[derive(Debug, Error)]
pub enum DictionaryError {
    #[error("RocksDB error: {0}")]
    RocksDbError(#[from] rocksdb::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Pubkey ID {0} not found")]
    NotFound(u64),
}

/// Result type
pub type Result<T> = std::result::Result<T, DictionaryError>;

/// Persistent pubkey dictionary
pub struct PersistentPubkeyDictionary {
    db: Arc<DB>,
    memory_cache: RwLock<PubkeyDictionary>,
    write_opts: WriteOptions,
    next_id: std::sync::atomic::AtomicU64,
}

impl PersistentPubkeyDictionary {
    /// Open or create dictionary database
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        info!("Opening pubkey dictionary at {:?}", path);
        
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        
        let db = DB::open(&opts, path)?;
        let db = Arc::new(db);
        
        // Load existing entries into memory cache
        let mut cache = PubkeyDictionary::new();
        let mut max_id = 0u64;
        
        let mut iter = db.iterator(rocksdb::IteratorMode::Start);
        while let Some(result) = iter.next() {
            let (key, value) = result?;
            
            // Key format: "pk:{pubkey}" -> pubkey_id
            // Key format: "id:{pubkey_id}" -> pubkey
            if let Some(key_str) = std::str::from_utf8(&key).ok() {
                if key_str.starts_with("pk:") {
                    if let Ok(pubkey) = Pubkey::try_from(&value[..]) {
                        if let Ok(id) = key_str[3..].parse::<u64>() {
                            cache.get_or_create_id(pubkey);
                            if id > max_id {
                                max_id = id;
                            }
                        }
                    }
                }
            }
        }
        
        let mut write_opts = WriteOptions::default();
        write_opts.set_sync(true);
        
        info!(
            "Pubkey dictionary loaded with {} entries (max_id={})",
            cache.len(),
            max_id
        );
        
        Ok(Self {
            db,
            memory_cache: RwLock::new(cache),
            write_opts,
            next_id: std::sync::atomic::AtomicU64::new(max_id + 1),
        })
    }

    /// Get or create pubkey_id
    pub fn get_or_create_id(&self, pubkey: Pubkey) -> u64 {
        // Check memory cache first
        {
            let cache = self.memory_cache.read();
            if let Some(id) = cache.get_id(pubkey) {
                return id;
            }
        }
        
        // Check DB
        let key = format!("pk:{}", pubkey);
        if let Ok(Some(value)) = self.db.get(key.as_bytes()) {
            if let Ok(id) = std::str::from_utf8(&value).unwrap_or("").parse::<u64>() {
                // Add to cache
                {
                    let mut cache = self.memory_cache.write();
                    cache.get_or_create_id(pubkey);
                }
                return id;
            }
        }
        
        // Create new ID
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        // Insert into DB
        let mut batch = WriteBatch::default();
        batch.put(format!("pk:{}", pubkey).as_bytes(), id.to_string().as_bytes());
        batch.put(format!("id:{}", id).as_bytes(), &pubkey.to_bytes());
        
        if let Err(e) = self.db.write_opt(batch, &self.write_opts) {
            tracing::error!("Failed to write pubkey dictionary: {}", e);
        }
        
        // Add to cache
        {
            let mut cache = self.memory_cache.write();
            cache.get_or_create_id(pubkey);
        }
        
        id
    }

    /// Get pubkey_id (no creation)
    pub fn get_id(&self, pubkey: Pubkey) -> Option<u64> {
        // Check memory cache first
        {
            let cache = self.memory_cache.read();
            if let Some(id) = cache.get_id(pubkey) {
                return Some(id);
            }
        }
        
        // Check DB
        let key = format!("pk:{}", pubkey);
        if let Ok(Some(value)) = self.db.get(key.as_bytes()) {
            if let Ok(id) = std::str::from_utf8(&value).unwrap_or("").parse::<u64>() {
                return Some(id);
            }
        }
        
        None
    }

    /// Get pubkey from ID
    pub fn get_pubkey(&self, pubkey_id: u64) -> Option<Pubkey> {
        // Check memory cache first
        {
            let cache = self.memory_cache.read();
            if let Some(pubkey) = cache.get_pubkey(pubkey_id) {
                return Some(pubkey);
            }
        }
        
        // Check DB
        let key = format!("id:{}", pubkey_id);
        if let Ok(Some(value)) = self.db.get(key.as_bytes()) {
            if let Ok(pubkey) = Pubkey::try_from(&value[..]) {
                return Some(pubkey);
            }
        }
        
        None
    }

    /// Batch insert pubkeys
    pub fn insert_batch(&self, pubkeys: &[Pubkey]) -> Vec<u64> {
        let mut ids = Vec::with_capacity(pubkeys.len());
        let mut batch = WriteBatch::default();
        
        for pubkey in pubkeys {
            // Check if exists
            if let Some(id) = self.get_id(*pubkey) {
                ids.push(id);
            } else {
                let id = self
                    .next_id
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                ids.push(id);
                
                batch.put(format!("pk:{}", pubkey).as_bytes(), id.to_string().as_bytes());
                batch.put(format!("id:{}", id).as_bytes(), &pubkey.to_bytes());
                
                // Add to cache
                {
                    let mut cache = self.memory_cache.write();
                    cache.get_or_create_id(*pubkey);
                }
            }
        }
        
        if let Err(e) = self.db.write_opt(batch, &self.write_opts) {
            tracing::error!("Failed to batch write pubkey dictionary: {}", e);
        }
        
        ids
    }

    /// Get count
    pub fn len(&self) -> usize {
        self.memory_cache.read().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.memory_cache.read().is_empty()
    }

    /// Get memory cache (for serialization/export)
    pub fn get_cache(&self) -> PubkeyDictionary {
        self.memory_cache.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_dictionary_persistence() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create and insert
        {
            let dict = PersistentPubkeyDictionary::open(temp_dir.path()).unwrap();
            let pubkey = Pubkey::new_unique();
            let id = dict.get_or_create_id(pubkey);
            
            assert_eq!(dict.get_id(pubkey), Some(id));
            assert_eq!(dict.get_pubkey(id), Some(pubkey));
        }
        
        // Reopen and verify
        {
            let dict = PersistentPubkeyDictionary::open(temp_dir.path()).unwrap();
            let pubkey = Pubkey::new_unique();
            let id = dict.get_or_create_id(pubkey);
            
            // Should have same ID after reopen
            assert_eq!(dict.get_id(pubkey), Some(id));
            assert_eq!(dict.get_pubkey(id), Some(pubkey));
        }
    }

    #[test]
    fn test_dictionary_batch_insert() {
        let temp_dir = TempDir::new().unwrap();
        let dict = PersistentPubkeyDictionary::open(temp_dir.path()).unwrap();
        
        let pubkeys: Vec<Pubkey> = (0..100).map(|_| Pubkey::new_unique()).collect();
        let ids = dict.insert_batch(&pubkeys);
        
        assert_eq!(ids.len(), 100);
        
        // Verify all inserted
        for (pubkey, id) in pubkeys.iter().zip(ids.iter()) {
            assert_eq!(dict.get_id(*pubkey), Some(*id));
            assert_eq!(dict.get_pubkey(*id), Some(*pubkey));
        }
    }
}
