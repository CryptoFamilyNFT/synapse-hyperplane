//! Adaptive Secondary Indexes
//! 
//! Indici secondari che si adattano automaticamente al pattern di accesso
//! e al tipo di dati.

use std::sync::Arc;
use std::path::PathBuf;
use std::collections::{BTreeMap, HashMap};

/// Adaptive Index Type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptiveIndexType {
    /// Bitmap per bassa cardinalità
    Bitmap,
    /// B-Tree per alta cardinalità
    BTree,
    /// Hash per lookup esatte
    Hash,
    /// Range index per query range
    Range,
}

/// Statistics per adattamento
#[derive(Debug, Clone)]
pub struct IndexStats {
    /// Numero di entries
    pub entry_count: u64,
    /// Numero di query
    pub query_count: u64,
    /// Tempo medio query (microseconds)
    pub avg_query_time_us: u64,
    /// Memoria usata (bytes)
    pub memory_bytes: u64,
    /// Tasso di hit
    pub hit_rate: f64,
}

/// Adaptive Secondary Index
pub struct AdaptiveIndex {
    /// Tipo corrente
    index_type: Arc<parking_lot::RwLock<AdaptiveIndexType>>,
    /// Bitmap index
    bitmap_index: Arc<parking_lot::RwLock<BTreeMap<u32, roaring::RoaringBitmap>>>,
    /// B-Tree index
    btree_index: Arc<parking_lot::RwLock<BTreeMap<u64, Vec<u32>>>>,
    /// Hash index
    hash_index: Arc<parking_lot::RwLock<HashMap<u64, Vec<u32>>>>,
    /// Statistics
    stats: Arc<parking_lot::RwLock<IndexStats>>,
    /// Path per persistenza
    index_path: PathBuf,
    /// Lock per migrazione
    migration_lock: parking_lot::Mutex<()>,
}

impl AdaptiveIndex {
    /// Crea un nuovo adaptive index
    pub fn new(index_path: PathBuf) -> Self {
        Self {
            index_type: Arc::new(parking_lot::RwLock::new(AdaptiveIndexType::Bitmap)), // Default
            bitmap_index: Arc::new(parking_lot::RwLock::new(BTreeMap::new())),
            btree_index: Arc::new(parking_lot::RwLock::new(BTreeMap::new())),
            hash_index: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            stats: Arc::new(parking_lot::RwLock::new(IndexStats {
                entry_count: 0,
                query_count: 0,
                avg_query_time_us: 0,
                memory_bytes: 0,
                hit_rate: 0.0,
            })),
            index_path,
            migration_lock: parking_lot::Mutex::new(()),
        }
    }
    
    /// Inserisce una entry
    pub fn insert(&self, key: u64, account_id: u32) {
        let index_type = *self.index_type.read();
        
        match index_type {
            AdaptiveIndexType::Bitmap => {
                let mut bitmap = self.bitmap_index.write();
                bitmap
                    .entry(key as u32)
                    .or_insert_with(roaring::RoaringBitmap::new)
                    .insert(account_id);
            }
            AdaptiveIndexType::BTree => {
                let mut btree = self.btree_index.write();
                btree.entry(key).or_insert_with(Vec::new).push(account_id);
            }
            AdaptiveIndexType::Hash => {
                let mut hash = self.hash_index.write();
                hash.entry(key).or_insert_with(Vec::new).push(account_id);
            }
            AdaptiveIndexType::Range => {
                // Range index usa B-Tree internamente
                let mut btree = self.btree_index.write();
                btree.entry(key).or_insert_with(Vec::new).push(account_id);
            }
        }
        
        // Aggiorna stats
        let mut stats = self.stats.write();
        stats.entry_count += 1;
    }
    
    /// Query per key
    pub fn query(&self, key: u64) -> Vec<u32> {
        let start = std::time::Instant::now();
        
        let index_type = *self.index_type.read();
        let result = match index_type {
            AdaptiveIndexType::Bitmap => {
                let bitmap = self.bitmap_index.read();
                bitmap
                    .get(&(key as u32))
                    .map(|b| b.iter().collect::<Vec<_>>())
                    .unwrap_or_default()
            }
            AdaptiveIndexType::BTree => {
                let btree = self.btree_index.read();
                btree.get(&key).cloned().unwrap_or_default()
            }
            AdaptiveIndexType::Hash => {
                let hash = self.hash_index.read();
                hash.get(&key).cloned().unwrap_or_default()
            }
            AdaptiveIndexType::Range => {
                let btree = self.btree_index.read();
                btree.get(&key).cloned().unwrap_or_default()
            }
        };
        
        // Aggiorna stats
        let elapsed = start.elapsed().as_micros() as u64;
        let mut stats = self.stats.write();
        stats.query_count += 1;
        stats.avg_query_time_us = (stats.avg_query_time_us + elapsed) / 2;
        
        result
    }
    
    /// Query range (solo per B-Tree e Range)
    pub fn query_range(&self, start_key: u64, end_key: u64) -> Vec<u32> {
        let index_type = *self.index_type.read();
        if index_type != AdaptiveIndexType::BTree
            && index_type != AdaptiveIndexType::Range
        {
            return Vec::new();
        }
        
        let btree = self.btree_index.read();
        btree
            .range(start_key..=end_key)
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect()
    }
    
    /// Valuta se migrare a un altro tipo di index
    pub fn evaluate_migration(&self) -> Option<AdaptiveIndexType> {
        let stats = self.stats.read();
        
        // Decisioni basate su statistics
        if stats.entry_count < 100 {
            return None; // Troppo poche entries per migrare
        }
        
        // Alta cardinalità → B-Tree
        let unique_keys = self.count_unique_keys();
        if (unique_keys as f64) > (stats.entry_count as f64) * 0.8 {
            let current_type = *self.index_type.read();
            if current_type != AdaptiveIndexType::BTree {
                return Some(AdaptiveIndexType::BTree);
            }
        }
        
        // Bassa cardinalità + molti account per key → Bitmap
        if unique_keys < 1000 && stats.entry_count > 10000 {
            let current_type = *self.index_type.read();
            if current_type != AdaptiveIndexType::Bitmap {
                return Some(AdaptiveIndexType::Bitmap);
            }
        }
        
        // Lookup esatti frequenti → Hash
        if stats.avg_query_time_us > 100 && unique_keys > 10000 {
            let current_type = *self.index_type.read();
            if current_type != AdaptiveIndexType::Hash {
                return Some(AdaptiveIndexType::Hash);
            }
        }
        
        None
    }
    
    /// Conta unique keys
    fn count_unique_keys(&self) -> u64 {
        let index_type = *self.index_type.read();
        match index_type {
            AdaptiveIndexType::Bitmap => {
                self.bitmap_index.read().len() as u64
            }
            AdaptiveIndexType::BTree | AdaptiveIndexType::Range => {
                self.btree_index.read().len() as u64
            }
            AdaptiveIndexType::Hash => {
                self.hash_index.read().len() as u64
            }
        }
    }
    
    /// Migra a un nuovo tipo di index (background)
    pub fn migrate_to(&self, new_type: AdaptiveIndexType) -> std::io::Result<()> {
        let _guard = self.migration_lock.lock();
        
        let current_type = *self.index_type.read();
        if new_type == current_type {
            return Ok(()); // Nessuna migrazione necessaria
        }
        
        // Migrazione semplificata (in produzione sarebbe più complessa)
        match new_type {
            AdaptiveIndexType::Bitmap => {
                self.bitmap_index.write().clear();
            }
            AdaptiveIndexType::BTree | AdaptiveIndexType::Range => {
                self.btree_index.write().clear();
            }
            AdaptiveIndexType::Hash => {
                self.hash_index.write().clear();
            }
        }
        
        *self.index_type.write() = new_type;
        
        Ok(())
    }
    
    /// Ottieni statistics
    pub fn stats(&self) -> IndexStats {
        self.stats.read().clone()
    }
    
    /// Ottieni tipo corrente
    pub fn index_type(&self) -> AdaptiveIndexType {
        *self.index_type.read()
    }
}

/// Adaptive Index Manager (gestisce tutti gli indici secondari)
pub struct AdaptiveIndexManager {
    /// Indici per programma
    indexes: Arc<parking_lot::RwLock<BTreeMap<String, AdaptiveIndex>>>,
    /// Directory base
    base_dir: PathBuf,
}

impl AdaptiveIndexManager {
    pub fn new(base_dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&base_dir)?;
        
        Ok(Self {
            indexes: Arc::new(parking_lot::RwLock::new(BTreeMap::new())),
            base_dir,
        })
    }
    
    /// Ottieni o crea un adaptive index
    pub fn get_or_create_index(&self, name: &str) -> Arc<AdaptiveIndex> {
        {
            let indexes = self.indexes.read();
            if let Some(index) = indexes.get(name) {
                return Arc::new(AdaptiveIndex::new(index.index_path.clone()));
            }
        }
        
        let index_path = self.base_dir.join(format!("{}.idx", name));
        let index = AdaptiveIndex::new(index_path.clone());
        
        let mut indexes = self.indexes.write();
        indexes.insert(name.to_string(), index);
        
        Arc::new(AdaptiveIndex::new(index_path))
    }
    
    /// Background task per adattamento
    pub fn run_adaptation_loop(&self) {
        std::thread::spawn({
            let indexes = self.indexes.clone();
            move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    
                    let indexes = indexes.read();
                    for (_, index) in indexes.iter() {
                        if let Some(new_type) = index.evaluate_migration() {
                            let _ = index.migrate_to(new_type);
                        }
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_adaptive_index_bitmap() {
        let temp_dir = std::env::temp_dir().join("adaptive_test");
        let index = AdaptiveIndex::new(temp_dir);
        
        // Inserisci dati
        index.insert(1, 100);
        index.insert(1, 101);
        index.insert(2, 102);
        
        // Query
        let result = index.query(1);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&100));
        assert!(result.contains(&101));
    }
    
    #[test]
    fn test_adaptive_index_btree() {
        let temp_dir = std::env::temp_dir().join("adaptive_btree");
        let mut index = AdaptiveIndex::new(temp_dir);
        let _ = index.migrate_to(AdaptiveIndexType::BTree);
        
        // Inserisci dati
        index.insert(1000, 1);
        index.insert(2000, 2);
        index.insert(3000, 3);
        
        // Query range
        let result = index.query_range(1500, 2500);
        assert_eq!(result, vec![2]);
        assert_eq!(result, vec![2]);
    }
    
    #[test]
    fn test_adaptive_index_manager() {
        let temp_dir = std::env::temp_dir().join("adaptive_mgr");
        let manager = AdaptiveIndexManager::new(temp_dir).unwrap();
        
        let index = manager.get_or_create_index("test_idx");
        index.insert(1, 100);
        
        let result = index.query(1);
        assert_eq!(result.len(), 1);
    }
}
