//! Tiered Dictionary per Token Mints
//! 
//! Dizionario gerarchico per discriminators e mint addresses
//! con separazione hot/cold e caching multi-livello.

use std::sync::Arc;
use std::path::PathBuf;
use std::collections::{BTreeMap, HashMap};

/// Tiered Dictionary Level
#[derive(Debug, Clone, Copy)]
pub enum DictTier {
    /// L1: Hot dictionary (in-memory, accesso frequente)
    L1Hot,
    /// L2: Warm dictionary (in-memory, accesso moderato)
    L2Warm,
    /// L3: Cold dictionary (disk-backed, accesso raro)
    L3Cold,
}

/// Entry del dictionary
#[derive(Debug, Clone)]
pub struct DictEntry {
    /// Key (discriminator o mint)
    pub key: u64,
    /// Account IDs associati
    pub account_ids: Vec<u32>,
    /// Tier corrente
    pub tier: DictTier,
    /// Conteggio accessi
    pub access_count: u64,
    /// Ultimo accesso (slot)
    pub last_access_slot: u64,
}

impl DictEntry {
    pub fn new(key: u64, account_id: u32, slot: u64) -> Self {
        Self {
            key,
            account_ids: vec![account_id],
            tier: DictTier::L1Hot,
            access_count: 1,
            last_access_slot: slot,
        }
    }
    
    /// Aggiorna accesso
    pub fn access(&mut self, slot: u64) {
        self.access_count += 1;
        self.last_access_slot = slot;
    }
    
    /// Aggiungi account
    pub fn add_account(&mut self, account_id: u32) {
        if !self.account_ids.contains(&account_id) {
            self.account_ids.push(account_id);
        }
    }
}

/// L1 Hot Dictionary (veloce, in-memory)
pub struct L1HotDict {
    /// HashMap per lookup O(1)
    entries: Arc<parking_lot::RwLock<HashMap<u64, DictEntry>>>,
    /// Dimensione massima
    max_size: usize,
}

impl L1HotDict {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: Arc::new(parking_lot::RwLock::new(HashMap::with_capacity(max_size))),
            max_size,
        }
    }
    
    /// Inserisci entry
    pub fn insert(&self, key: u64, account_id: u32, slot: u64) -> Option<DictEntry> {
        let mut entries = self.entries.write();
        
        if let Some(entry) = entries.get_mut(&key) {
            entry.access(slot);
            entry.add_account(account_id);
            Some(entry.clone())
        } else {
            // Check capacity
            if entries.len() >= self.max_size {
                // Eviction policy: LRU semplificato
                entries.retain(|_, v| v.access_count > 0);
            }
            
            let entry = DictEntry::new(key, account_id, slot);
            entries.insert(key, entry.clone());
            Some(entry)
        }
    }
    
    /// Lookup
    pub fn get(&self, key: u64) -> Option<DictEntry> {
        let mut entries = self.entries.write();
        if let Some(entry) = entries.get_mut(&key) {
            entry.access_count += 1;
            Some(entry.clone())
        } else {
            None
        }
    }
    
    /// Rimuovi entry
    pub fn remove(&self, key: u64) -> Option<DictEntry> {
        self.entries.write().remove(&key)
    }
    
    /// Cardinalità
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }
}

/// L2 Warm Dictionary (medio, in-memory)
pub struct L2WarmDict {
    /// B-Tree per lookup ordinato
    entries: Arc<parking_lot::RwLock<BTreeMap<u64, DictEntry>>>,
    /// Dimensione massima
    max_size: usize,
}

impl L2WarmDict {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: Arc::new(parking_lot::RwLock::new(BTreeMap::new())),
            max_size,
        }
    }
    
    /// Inserisci entry
    pub fn insert(&self, key: u64, account_id: u32, slot: u64) {
        let mut entries = self.entries.write();
        
        if let Some(entry) = entries.get_mut(&key) {
            entry.access(slot);
            entry.add_account(account_id);
        } else {
            if entries.len() >= self.max_size {
                // Rimuovi entry meno usata
                entries.pop_first();
            }
            
            let entry = DictEntry::new(key, account_id, slot);
            entries.insert(key, entry);
        }
    }
    
    /// Lookup
    pub fn get(&self, key: u64) -> Option<DictEntry> {
        let mut entries = self.entries.write();
        if let Some(entry) = entries.get_mut(&key) {
            entry.access_count += 1;
            Some(entry.clone())
        } else {
            None
        }
    }
    
    /// Range query
    pub fn range(&self, start: u64, end: u64) -> Vec<DictEntry> {
        let entries = self.entries.read();
        entries
            .range(start..=end)
            .map(|(_, v)| v.clone())
            .collect()
    }
    
    /// Cardinalità
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }
}

/// L3 Cold Dictionary (lento, disk-backed)
pub struct L3ColdDict {
    /// Path del file
    file_path: PathBuf,
    /// Cache in-memory
    cache: Arc<parking_lot::RwLock<HashMap<u64, DictEntry>>>,
}

impl L3ColdDict {
    pub fn new(file_path: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(file_path.parent().unwrap())?;
        
        Ok(Self {
            file_path,
            cache: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        })
    }
    
    /// Inserisci entry (scrive su disk)
    pub fn insert(&self, key: u64, account_id: u32, slot: u64) -> std::io::Result<()> {
        let mut cache = self.cache.write();
        
        if let Some(entry) = cache.get_mut(&key) {
            entry.access(slot);
            entry.add_account(account_id);
        } else {
            let entry = DictEntry::new(key, account_id, slot);
            cache.insert(key, entry);
        }
        
        // Persisti su disk (semplificato)
        self.persist()?;
        
        Ok(())
    }
    
    /// Lookup (legge da disk se non in cache)
    pub fn get(&self, key: u64) -> Option<DictEntry> {
        let mut cache = self.cache.write();
        if let Some(entry) = cache.get_mut(&key) {
            entry.access_count += 1;
            Some(entry.clone())
        } else {
            // Leggi da disk (semplificato)
            None
        }
    }
    
    /// Persisti su disk
    fn persist(&self) -> std::io::Result<()> {
        // Implementazione semplificata
        Ok(())
    }
    
    /// Cardinalità
    pub fn len(&self) -> usize {
        self.cache.read().len()
    }
}

/// Tiered Dictionary Manager
pub struct TieredDictionary {
    /// L1 Hot
    l1: Arc<L1HotDict>,
    /// L2 Warm
    l2: Arc<L2WarmDict>,
    /// L3 Cold
    l3: Arc<L3ColdDict>,
    /// Slot corrente
    current_slot: Arc<parking_lot::RwLock<u64>>,
}

impl TieredDictionary {
    pub fn new(base_dir: PathBuf, l1_size: usize, l2_size: usize) -> std::io::Result<Self> {
        Ok(Self {
            l1: Arc::new(L1HotDict::new(l1_size)),
            l2: Arc::new(L2WarmDict::new(l2_size)),
            l3: Arc::new(L3ColdDict::new(base_dir.join("cold.dict"))?),
            current_slot: Arc::new(parking_lot::RwLock::new(0)),
        })
    }
    
    /// Inserisci entry (inizia da L1)
    pub fn insert(&self, key: u64, account_id: u32) {
        let slot = *self.current_slot.read();
        
        // Inserisci in L1
        self.l1.insert(key, account_id, slot);
    }
    
    /// Lookup (cerca in L1 → L2 → L3)
    pub fn get(&self, key: u64) -> Option<DictEntry> {
        // Cerca in L1
        if let Some(entry) = self.l1.get(key) {
            return Some(entry);
        }
        
        // Cerca in L2
        if let Some(entry) = self.l2.get(key) {
            // Promuovi a L1
            self.l1.insert(key, entry.account_ids[0], entry.last_access_slot);
            return Some(entry);
        }
        
        // Cerca in L3
        self.l3.get(key)
    }
    
    /// Demote da L1 a L2
    pub fn demote_l1_to_l2(&self, key: u64) {
        if let Some(entry) = self.l1.remove(key) {
            self.l2.insert(key, entry.account_ids[0], entry.last_access_slot);
        }
    }
    
    /// Demote da L2 a L3
    pub fn demote_l2_to_l3(&self, key: u64) -> std::io::Result<()> {
        if let Some(entry) = self.l2.get(key) {
            self.l3.insert(key, entry.account_ids[0], entry.last_access_slot)?;
            let _ = self.l3.insert(key, entry.account_ids[0], entry.last_access_slot);
        }
        Ok(())
    }
    
    /// Aggiorna slot corrente
    pub fn update_slot(&self, slot: u64) {
        *self.current_slot.write() = slot;
    }
    
    /// Statistics
    pub fn stats(&self) -> TieredDictStats {
        TieredDictStats {
            l1_count: self.l1.len(),
            l2_count: self.l2.len(),
            l3_count: self.l3.len(),
            total_count: self.l1.len() + self.l2.len() + self.l3.len(),
        }
    }
}

/// Statistics per Tiered Dictionary
#[derive(Debug, Clone)]
pub struct TieredDictStats {
    pub l1_count: usize,
    pub l2_count: usize,
    pub l3_count: usize,
    pub total_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_l1_hot_dict() {
        let l1 = L1HotDict::new(100);
        
        l1.insert(1000, 1, 1);
        l1.insert(1000, 2, 2);
        
        let entry = l1.get(1000).unwrap();
        assert_eq!(entry.account_ids.len(), 2);
        assert!(entry.account_ids.contains(&1));
        assert!(entry.account_ids.contains(&2));
    }
    
    #[test]
    fn test_tiered_dictionary() {
        let temp_dir = std::env::temp_dir().join("tiered_dict");
        let dict = TieredDictionary::new(temp_dir, 100, 1000).unwrap();
        
        // Inserisci
        dict.insert(1000, 1);
        dict.insert(2000, 2);
        
        // Lookup
        let entry = dict.get(1000).unwrap();
        assert_eq!(entry.key, 1000);
        assert_eq!(entry.account_ids, vec![1]);
        
        // Stats
        let stats = dict.stats();
        assert_eq!(stats.l1_count, 2);
    }
}
