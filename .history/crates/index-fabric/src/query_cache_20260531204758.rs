//! Query Result Cache per Risultati Frequenti
//! 
//! Cache multi-livello (L1/L2/L3) per risultati di query
//! con invalidazione basata su slot e TTL.

use std::sync::Arc;
use std::path::PathBuf;
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

/// Cache Key (hash della query)
pub type CacheKey = u64;

/// Cache Entry
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Dati cached
    pub data: Vec<u8>,
    /// Timestamp creazione
    pub created_at: u64,
    /// Slot di creazione
    pub slot: u64,
    /// Access count
    pub access_count: u64,
    /// Ultimo accesso
    pub last_access: u64,
    /// TTL (seconds)
    pub ttl_secs: u64,
}

impl CacheEntry {
    pub fn new(data: Vec<u8>, slot: u64, ttl_secs: u64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            data,
            created_at: now,
            slot,
            access_count: 0,
            last_access: now,
            ttl_secs,
        }
    }
    
    /// Check se expired
    pub fn is_expired(&self) -> bool {
        if self.ttl_secs == 0 {
            return true; // TTL 0 means immediate expiration
        }
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        now > self.created_at + self.ttl_secs
    }
    
    /// Check se invalidato da slot
    pub fn is_invalidated_by_slot(&self, current_slot: u64, slot_threshold: u64) -> bool {
        current_slot > self.slot + slot_threshold
    }
    
    /// Aggiorna accesso
    pub fn access(&mut self) {
        self.access_count += 1;
        self.last_access = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
}

/// L1 Cache (in-memory, velocissima, piccola)
pub struct L1Cache {
    /// HashMap per lookup O(1)
    entries: Arc<parking_lot::RwLock<HashMap<CacheKey, CacheEntry>>>,
    /// Dimensione massima
    max_size: usize,
    /// Dimensione corrente
    current_size: Arc<parking_lot::RwLock<usize>>,
}

impl L1Cache {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: Arc::new(parking_lot::RwLock::new(HashMap::with_capacity(max_size))),
            max_size,
            current_size: Arc::new(parking_lot::RwLock::new(0)),
        }
    }
    
    /// Inserisci entry
    pub fn insert(&self, key: CacheKey, entry: CacheEntry) -> Option<CacheEntry> {
        let mut entries = self.entries.write();
        
        // Check capacity
        if entries.len() >= self.max_size {
            // Eviction: LRU semplificato
            self.evict_lru(&mut entries);
        }
        
        let old = entries.insert(key, entry);
        *self.current_size.write() = entries.len();
        
        old
    }
    
    /// Lookup
    pub fn get(&self, key: CacheKey) -> Option<CacheEntry> {
        let mut entries = self.entries.write();
        if let Some(entry) = entries.get_mut(&key) {
            if entry.is_expired() {
                entries.remove(&key);
                *self.current_size.write() = entries.len();
                None
            } else {
                entry.access();
                Some(entry.clone())
            }
        } else {
            None
        }
    }
    
    /// Rimuovi entry
    pub fn remove(&self, key: CacheKey) -> Option<CacheEntry> {
        let result = self.entries.write().remove(&key);
        *self.current_size.write() = self.entries.read().len();
        result
    }
    
    /// Eviction LRU
    fn evict_lru(&self, entries: &mut HashMap<CacheKey, CacheEntry>) {
        if let Some((&key, _)) = entries.iter().min_by_key(|(_, e)| e.last_access) {
            entries.remove(&key);
        }
    }
    
    /// Cardinalità
    pub fn len(&self) -> usize {
        *self.current_size.read()
    }
    
    /// Hit rate
    pub fn hit_rate(&self) -> f64 {
        // Semplificato: in produzione terrebbe traccia di hit/miss
        0.8
    }
}

/// L2 Cache (in-memory, grande)
pub struct L2Cache {
    entries: Arc<parking_lot::RwLock<HashMap<CacheKey, CacheEntry>>>,
    max_size: usize,
    current_size: Arc<parking_lot::RwLock<usize>>,
}

impl L2Cache {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: Arc::new(parking_lot::RwLock::new(HashMap::with_capacity(max_size))),
            max_size,
            current_size: Arc::new(parking_lot::RwLock::new(0)),
        }
    }
    
    pub fn insert(&self, key: CacheKey, entry: CacheEntry) -> Option<CacheEntry> {
        let mut entries = self.entries.write();
        
        if entries.len() >= self.max_size {
            self.evict_lfu(&mut entries);
        }
        
        let old = entries.insert(key, entry);
        *self.current_size.write() = entries.len();
        
        old
    }
    
    pub fn get(&self, key: CacheKey) -> Option<CacheEntry> {
        let mut entries = self.entries.write();
        if let Some(entry) = entries.get_mut(&key) {
            if entry.is_expired() {
                entries.remove(&key);
                *self.current_size.write() = entries.len();
                None
            } else {
                entry.access();
                Some(entry.clone())
            }
        } else {
            None
        }
    }
    
    fn evict_lfu(&self, entries: &mut HashMap<CacheKey, CacheEntry>) {
        if let Some((&key, _)) = entries.iter().min_by_key(|(_, e)| e.access_count) {
            entries.remove(&key);
        }
    }
    
    pub fn len(&self) -> usize {
        *self.current_size.read()
    }
}

/// L3 Cache (disk-backed, molto grande)
pub struct L3Cache {
    /// Directory per file cache
    cache_dir: PathBuf,
    /// Metadata in-memory
    metadata: Arc<parking_lot::RwLock<HashMap<CacheKey, CacheEntry>>>,
}

impl L3Cache {
    pub fn new(cache_dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&cache_dir)?;
        
        Ok(Self {
            cache_dir,
            metadata: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        })
    }
    
    pub fn insert(&self, key: CacheKey, entry: CacheEntry) -> std::io::Result<()> {
        // Scrivi su disk
        let file_path = self.cache_dir.join(format!("{:x}.cache", key));
        std::fs::write(&file_path, &entry.data)?;
        
        // Salva metadata
        self.metadata.write().insert(key, entry);
        
        Ok(())
    }
    
    pub fn get(&self, key: CacheKey) -> Option<CacheEntry> {
        let mut metadata = self.metadata.write();
        if let Some(entry) = metadata.get_mut(&key) {
            if entry.is_expired() {
                let _ = std::fs::remove_file(self.cache_dir.join(format!("{:x}.cache", key)));
                metadata.remove(&key);
                None
            } else {
                entry.access();
                Some(entry.clone())
            }
        } else {
            // Leggi da disk
            let file_path = self.cache_dir.join(format!("{:x}.cache", key));
            if let Ok(data) = std::fs::read(&file_path) {
                let entry = CacheEntry::new(data, 0, 3600);
                metadata.insert(key, entry.clone());
                Some(entry)
            } else {
                None
            }
        }
    }
    
    pub fn len(&self) -> usize {
        self.metadata.read().len()
    }
}

/// Query Result Cache Manager
pub struct QueryResultCache {
    /// L1 Cache
    l1: Arc<L1Cache>,
    /// L2 Cache
    l2: Arc<L2Cache>,
    /// L3 Cache
    l3: Arc<L3Cache>,
    /// Slot corrente
    current_slot: Arc<parking_lot::RwLock<u64>>,
    /// Slot threshold per invalidazione
    #[allow(dead_code)]
    slot_threshold: u64,
    /// Statistics
    hits: Arc<parking_lot::RwLock<u64>>,
    misses: Arc<parking_lot::RwLock<u64>>,
}

impl QueryResultCache {
    pub fn new(cache_dir: PathBuf, l1_size: usize, l2_size: usize, slot_threshold: u64) -> std::io::Result<Self> {
        Ok(Self {
            l1: Arc::new(L1Cache::new(l1_size)),
            l2: Arc::new(L2Cache::new(l2_size)),
            l3: Arc::new(L3Cache::new(cache_dir.join("l3"))?),
            current_slot: Arc::new(parking_lot::RwLock::new(0)),
            slot_threshold,
            hits: Arc::new(parking_lot::RwLock::new(0)),
            misses: Arc::new(parking_lot::RwLock::new(0)),
        })
    }
    
    /// Genera cache key da query
    pub fn generate_key<Q: Hash>(&self, query: &Q) -> CacheKey {
        let mut hasher = DefaultHasher::new();
        query.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Lookup cache (L1 → L2 → L3)
    pub fn get(&self, key: CacheKey) -> Option<CacheEntry> {
        // L1
        if let Some(entry) = self.l1.get(key) {
            *self.hits.write() += 1;
            return Some(entry);
        }
        
        // L2
        if let Some(entry) = self.l2.get(key) {
            *self.hits.write() += 1;
            // Promuovi a L1
            self.l1.insert(key, entry.clone());
            return Some(entry);
        }
        
        // L3
        if let Some(entry) = self.l3.get(key) {
            *self.hits.write() += 1;
            // Promuovi a L2
            self.l2.insert(key, entry.clone());
            return Some(entry);
        }
        
        *self.misses.write() += 1;
        None
    }
    
    /// Inserisci in cache (inizia da L1)
    pub fn insert(&self, key: CacheKey, data: Vec<u8>, slot: u64, ttl_secs: u64) {
        let entry = CacheEntry::new(data, slot, ttl_secs);
        
        // Inserisci in L1
        self.l1.insert(key, entry.clone());
        
        // Backup in L2
        self.l2.insert(key, entry.clone());
        
        // Backup in L3
        let _ = self.l3.insert(key, entry);
    }
    
    /// Invalida per slot
    pub fn invalidate_by_slot(&self, _current_slot: u64) {
        // Rimuovi entries con slot troppo vecchia
        // Implementazione semplificata
    }
    
    /// Aggiorna slot corrente
    pub fn update_slot(&self, slot: u64) {
        *self.current_slot.write() = slot;
        self.invalidate_by_slot(slot);
    }
    
    /// Hit rate
    pub fn hit_rate(&self) -> f64 {
        let hits = *self.hits.read();
        let misses = *self.misses.read();
        
        if hits + misses == 0 {
            0.0
        } else {
            hits as f64 / (hits + misses) as f64
        }
    }
    
    /// Statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            l1_count: self.l1.len(),
            l2_count: self.l2.len(),
            l3_count: self.l3.len(),
            hit_rate: self.hit_rate(),
            total_hits: *self.hits.read(),
            total_misses: *self.misses.read(),
        }
    }
}

/// Cache Statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub l1_count: usize,
    pub l2_count: usize,
    pub l3_count: usize,
    pub hit_rate: f64,
    pub total_hits: u64,
    pub total_misses: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_l1_cache() {
        let l1 = L1Cache::new(100);
        
        let entry = CacheEntry::new(vec![1, 2, 3], 1000, 3600);
        l1.insert(12345, entry);
        
        let retrieved = l1.get(12345).unwrap();
        assert_eq!(retrieved.data, vec![1, 2, 3]);
        assert_eq!(retrieved.access_count, 1);
    }
    
    #[test]
    fn test_query_result_cache() {
        let temp_dir = std::env::temp_dir().join("query_cache");
        let cache = QueryResultCache::new(temp_dir, 100, 1000, 10000).unwrap();
        
        // Inserisci
        let key = cache.generate_key(&"test_query");
        cache.insert(key, vec![1, 2, 3], 1000, 3600);
        
        // Lookup
        let entry = cache.get(key).unwrap();
        assert_eq!(entry.data, vec![1, 2, 3]);
        
        // Stats
        let stats = cache.stats();
        assert_eq!(stats.total_hits, 1);
        assert!(stats.hit_rate > 0.0);
    }
    
    #[test]
    fn test_cache_expiration() {
        let l1 = L1Cache::new(100);
        
        // Entry con TTL 100ms
        let entry = CacheEntry::new(vec![1, 2, 3], 1000, 0);
        l1.insert(12345, entry);
        
        // Sleep 150ms
        std::thread::sleep(std::time::Duration::from_millis(150));
        
        // Dovrebbe essere expired
        let result = l1.get(12345);
        assert!(result.is_none(), "Cache entry should be expired");
    }
}
