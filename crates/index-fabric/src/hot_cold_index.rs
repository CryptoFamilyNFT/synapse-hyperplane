//! Hot/Cold Index Separation
//!
//! Separa gli indici in base alla frequenza di accesso:
//! - HOT: Ultimi 2-5M account modificati (L3 cache friendly)
//! - WARM: Account attivi negli ultimi N epoch
//! - COLD: Tutto il resto (RAM normale)
//!
//! Ottimizzazione per mainnet: 90% query su 10% account

use std::collections::BTreeMap;
use std::sync::Arc;
use parking_lot::RwLock;
use solana_sdk::pubkey::Pubkey;

use crate::lsm_bitmap::LsmBitmap;

/// Soglia per considerare un account "hot" (accessi negli ultimi N slot)
const HOT_THRESHOLD_SLOTS: u64 = 100;

/// Soglia per considerare un account "warm" (accessi negli ultimi N slot)
const WARM_THRESHOLD_SLOTS: u64 = 10_000;

/// Tier di storage per un account
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageTier {
    /// Hot: accesso frequente (< 100 slot fa)
    Hot,
    /// Warm: accesso moderato (< 10k slot fa)
    Warm,
    /// Cold: accesso raro
    Cold,
}

/// Metadati per un account
#[derive(Debug, Clone)]
pub struct AccountMetadata {
    /// Ultima slot di accesso
    pub last_access_slot: u64,
    /// Numero di accessi totali
    pub access_count: u64,
    /// Tier corrente
    pub tier: StorageTier,
}

impl AccountMetadata {
    pub fn new(current_slot: u64) -> Self {
        Self {
            last_access_slot: current_slot,
            access_count: 1,
            tier: StorageTier::Hot,
        }
    }
    
    /// Aggiorna metadati e ritorna nuovo tier
    pub fn access(&mut self, current_slot: u64) -> StorageTier {
        self.last_access_slot = current_slot;
        self.access_count += 1;
        
        // Determina nuovo tier
        self.tier = self.calculate_tier(current_slot);
        self.tier
    }
    
    /// Calcola tier basato su ultima accesso
    fn calculate_tier(&self, current_slot: u64) -> StorageTier {
        let slots_since_access = current_slot.saturating_sub(self.last_access_slot);
        
        if slots_since_access < HOT_THRESHOLD_SLOTS {
            StorageTier::Hot
        } else if slots_since_access < WARM_THRESHOLD_SLOTS {
            StorageTier::Warm
        } else {
            StorageTier::Cold
        }
    }
}

/// Hot/Cold Index per un programma
pub struct TieredIndex {
    /// Hot index: account frequentemente accessiti
    hot_index: LsmBitmap,
    
    /// Warm index: account moderatamente accessiti
    warm_index: LsmBitmap,
    
    /// Cold index: account raramente accessiti
    cold_index: LsmBitmap,
    
    /// Metadati per account (pubkey → metadata)
    metadata: Arc<RwLock<BTreeMap<Pubkey, AccountMetadata>>>,
    
    /// Slot corrente
    current_slot: Arc<RwLock<u64>>,
}

impl TieredIndex {
    /// Crea un nuovo TieredIndex
    pub fn new() -> Self {
        Self {
            hot_index: LsmBitmap::new(),
            warm_index: LsmBitmap::new(),
            cold_index: LsmBitmap::new(),
            metadata: Arc::new(RwLock::new(BTreeMap::new())),
            current_slot: Arc::new(RwLock::new(0)),
        }
    }
    
    /// Imposta slot corrente
    pub fn set_current_slot(&self, slot: u64) {
        *self.current_slot.write() = slot;
    }
    
    /// Inserisce un account (determina automaticamente il tier)
    pub fn insert_account(&self, account_id: u32, pubkey: Pubkey) {
        let current_slot = *self.current_slot.read();
        
        // Crea metadati
        let mut metadata = AccountMetadata::new(current_slot);
        metadata.tier = StorageTier::Hot; // Nuovo account è sempre hot
        
        // Inserisci nel tier appropriato
        match metadata.tier {
            StorageTier::Hot => self.hot_index.insert(account_id),
            StorageTier::Warm => self.warm_index.insert(account_id),
            StorageTier::Cold => self.cold_index.insert(account_id),
        }
        
        // Salva metadati
        let mut metadata_map = self.metadata.write();
        metadata_map.insert(pubkey, metadata);
    }
    
    /// Accessa un account (potenzialmente migra di tier)
    pub fn access_account(&self, _account_id: u32, pubkey: Pubkey) -> StorageTier {
        let current_slot = *self.current_slot.read();
        
        // Aggiorna metadati e ottieni nuovo tier
        let new_tier = {
            let mut metadata_map = self.metadata.write();
            if let Some(metadata) = metadata_map.get_mut(&pubkey) {
                metadata.access(current_slot)
            } else {
                // Account non trovato, crea nuovo
                let metadata = AccountMetadata::new(current_slot);
                let tier = metadata.tier;
                metadata_map.insert(pubkey, metadata);
                tier
            }
        };
        
        // Nota: migrazione fisica tra tier potrebbe essere necessaria
        // Per ora: solo tracking, migrazione lazy durante query
        
        new_tier
    }
    
    /// Query su hot index (più veloce, L3 cache friendly)
    pub fn query_hot(&self) -> roaring::RoaringBitmap {
        self.hot_index.query()
    }
    
    /// Query su warm index
    pub fn query_warm(&self) -> roaring::RoaringBitmap {
        self.warm_index.query()
    }
    
    /// Query su cold index
    pub fn query_cold(&self) -> roaring::RoaringBitmap {
        self.cold_index.query()
    }
    
    /// Query su tutti i tier (unione)
    pub fn query_all(&self) -> roaring::RoaringBitmap {
        let mut result = self.hot_index.query();
        result |= &self.warm_index.query();
        result |= &self.cold_index.query();
        result
    }
    
    /// Statistics
    pub fn stats(&self) -> TieredIndexStats {
        let hot_stats = self.hot_index.stats();
        let warm_stats = self.warm_index.stats();
        let cold_stats = self.cold_index.stats();
        let metadata = self.metadata.read();
        
        TieredIndexStats {
            hot_cardinality: hot_stats.estimated_total,
            warm_cardinality: warm_stats.estimated_total,
            cold_cardinality: cold_stats.estimated_total,
            total_accounts: metadata.len() as u64,
            hot_percentage: if metadata.is_empty() {
                0.0
            } else {
                hot_stats.estimated_total as f64 / metadata.len() as f64 * 100.0
            },
        }
    }
    
    /// Promuovi account a tier superiore (opzionale, per ottimizzazione)
    pub fn promote_to_hot(&self, account_id: u32) {
        // Rimuovi da warm/cold e inserisci in hot
        // Nota: richiede tracking più sofisticato
        self.hot_index.insert(account_id);
    }
}

impl Default for TieredIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics per TieredIndex
#[derive(Debug, Clone)]
pub struct TieredIndexStats {
    pub hot_cardinality: u64,
    pub warm_cardinality: u64,
    pub cold_cardinality: u64,
    pub total_accounts: u64,
    pub hot_percentage: f64,
}

/// Hot/Cold Index Manager (globale, tutti i programmi)
pub struct HotColdIndexManager {
    /// Tiered index per programma
    program_indexes: Arc<RwLock<BTreeMap<Pubkey, Arc<TieredIndex>>>>,
    
    /// Slot corrente
    current_slot: Arc<RwLock<u64>>,
}

impl HotColdIndexManager {
    pub fn new() -> Self {
        Self {
            program_indexes: Arc::new(RwLock::new(BTreeMap::new())),
            current_slot: Arc::new(RwLock::new(0)),
        }
    }
    
    /// Aggiorna slot corrente
    pub fn update_slot(&self, slot: u64) {
        *self.current_slot.write() = slot;
        
        // Aggiorna tutti gli index
        let indexes = self.program_indexes.read();
        for index in indexes.values() {
            index.set_current_slot(slot);
        }
    }
    
    /// Ottieni o crea index per programma
    pub fn get_or_create_index(&self, program_id: Pubkey) -> Arc<TieredIndex> {
        {
            let indexes = self.program_indexes.read();
            if let Some(index) = indexes.get(&program_id) {
                return Arc::clone(index);
            }
        }
        
        // Crea nuovo index
        let new_index = TieredIndex::new();
        new_index.set_current_slot(*self.current_slot.read());
        
        let index = Arc::new(new_index);
        
        let mut indexes = self.program_indexes.write();
        indexes.insert(program_id, Arc::clone(&index));
        
        index
    }
    
    /// Statistics globali
    pub fn global_stats(&self) -> HotColdGlobalStats {
        let indexes = self.program_indexes.read();
        
        let mut total_hot = 0;
        let mut total_warm = 0;
        let mut total_cold = 0;
        let mut total_accounts = 0;
        
        for index in indexes.values() {
            let stats = index.stats();
            total_hot += stats.hot_cardinality;
            total_warm += stats.warm_cardinality;
            total_cold += stats.cold_cardinality;
            total_accounts += stats.total_accounts;
        }
        
        HotColdGlobalStats {
            total_programs: indexes.len() as u64,
            total_hot,
            total_warm,
            total_cold,
            total_accounts,
            hot_percentage: if total_accounts == 0 {
                0.0
            } else {
                total_hot as f64 / total_accounts as f64 * 100.0
            },
        }
    }
}

impl Default for HotColdIndexManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics globali per HotColdIndexManager
#[derive(Debug, Clone)]
pub struct HotColdGlobalStats {
    pub total_programs: u64,
    pub total_hot: u64,
    pub total_warm: u64,
    pub total_cold: u64,
    pub total_accounts: u64,
    pub hot_percentage: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tiered_index_basic() {
        let index = TieredIndex::new();
        index.set_current_slot(1000);
        
        // Inserisci account
        let pubkey = Pubkey::new_unique();
        index.insert_account(1, pubkey);
        
        // Query hot
        let hot = index.query_hot();
        assert_eq!(hot.len(), 1);
        
        // Stats
        let stats = index.stats();
        assert_eq!(stats.hot_cardinality, 1);
    }
    
    #[test]
    fn test_account_metadata_tier_calculation() {
        let mut metadata = AccountMetadata::new(1000);
        
        // Slot 1000: Hot (appena creato)
        assert_eq!(metadata.tier, StorageTier::Hot);
        
        // Slot 1050: Ancora Hot (< 100 slot di distanza)
        metadata.last_access_slot = 1000;
        assert_eq!(metadata.calculate_tier(1050), StorageTier::Hot);
        
        // Slot 2000: Warm (> 100 slot, < 10k slot di distanza)
        metadata.last_access_slot = 1000;
        assert_eq!(metadata.calculate_tier(2000), StorageTier::Warm);
        
        // Slot 15000: Cold (> 10k slot di distanza)
        metadata.last_access_slot = 1000;
        assert_eq!(metadata.calculate_tier(15000), StorageTier::Cold);
    }
    
    #[test]
    fn test_hot_cold_manager() {
        let manager = HotColdIndexManager::new();
        manager.update_slot(1000);
        
        // Ottieni index per programma
        let program_id = Pubkey::new_unique();
        let index = manager.get_or_create_index(program_id);
        
        // Inserisci account
        let account_pubkey = Pubkey::new_unique();
        index.insert_account(1, account_pubkey);
        
        // Stats globali
        let stats = manager.global_stats();
        assert_eq!(stats.total_programs, 1);
        assert_eq!(stats.total_hot, 1);
    }
}
