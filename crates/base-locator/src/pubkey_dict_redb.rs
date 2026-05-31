//! Pubkey Dictionary - Persistent storage (redb backend)

use hyperplane_types::PubkeyDictionary;
use parking_lot::RwLock;
use redb::{Database, TableDefinition};
use solana_sdk::pubkey::Pubkey;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;
use tracing::info;

const DICT_TABLE: TableDefinition<[u8; 32], u64> = TableDefinition::new("pubkey_dict");

#[derive(Debug, Error)]
pub enum DictionaryError {
    #[error("Redb error: {0}")]
    RedbError(#[from] redb::Error),
    
    #[error("Database error: {0}")]
    DatabaseError(#[from] redb::DatabaseError),
    
    #[error("Table error: {0}")]
    TableError(#[from] redb::TableError),
    
    #[error("Storage error: {0}")]
    StorageError(#[from] redb::StorageError),
    
    #[error("Transaction error: {0}")]
    TransactionError(#[from] redb::TransactionError),
    
    #[error("Commit error: {0}")]
    CommitError(#[from] redb::CommitError),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DictionaryError>;

pub struct PersistentPubkeyDictionary {
    db: Arc<Database>,
    memory_cache: RwLock<PubkeyDictionary>,
    next_id: AtomicU64,
}

impl PersistentPubkeyDictionary {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        info!("Opening pubkey dictionary (redb) at {:?}", path);
        
        std::fs::create_dir_all(path)?;
        let db_path = path.join("pubkey_dict.redb");
        let db = Database::create(db_path)?;
        
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(DICT_TABLE)?;
        }
        write_txn.commit()?;
        
        Ok(Self {
            db: Arc::new(db),
            memory_cache: RwLock::new(PubkeyDictionary::new()),
            next_id: AtomicU64::new(0),
        })
    }

    pub fn get_or_create_id(&self, pubkey: &Pubkey) -> Result<u64> {
        {
            let cache = self.memory_cache.read();
            if let Some(id) = cache.get_id(pubkey) {
                return Ok(id);
            }
        }
        
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(DICT_TABLE)?;
        
        if let Some(id) = table.get(&pubkey.to_bytes())? {
            return Ok(id.value());
        }
        
        let new_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(DICT_TABLE)?;
            let _ = table.insert(&pubkey.to_bytes(), &new_id);
        }
        write_txn.commit()?;
        
        let mut cache = self.memory_cache.write();
        cache.insert(*pubkey);
        
        Ok(new_id)
    }

    pub fn get_id(&self, pubkey: &Pubkey) -> Result<Option<u64>> {
        {
            let cache = self.memory_cache.read();
            if let Some(id) = cache.get_id(pubkey) {
                return Ok(Some(id));
            }
        }
        
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(DICT_TABLE)?;
        
        match table.get(&pubkey.to_bytes()) {
            Ok(Some(v)) => Ok(Some(v.value())),
            Ok(None) | Err(_) => Ok(None),
        }
    }

    pub fn get_pubkey(&self, id: u64) -> Option<Pubkey> {
        let cache = self.memory_cache.read();
        cache.get_pubkey(id)
    }
    
    pub fn insert_batch(&self, pubkeys: &[Pubkey]) -> Result<Vec<u64>> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(DICT_TABLE)?;
            let mut cache = self.memory_cache.write();
            
            for pk in pubkeys {
                if cache.get_id(pk).is_none() {
                    let new_id = self.next_id.fetch_add(1, Ordering::Relaxed);
                    let _ = table.insert(&pk.to_bytes(), &new_id);
                    cache.insert(*pk);
                }
            }
        }
        write_txn.commit()?;
        
        Ok(pubkeys.iter().filter_map(|pk| self.memory_cache.read().get_id(pk)).collect())
    }
    
    pub fn len(&self) -> usize {
        self.memory_cache.read().len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.memory_cache.read().is_empty()
    }
}
