//! Memcmp Acceleration Layer
//!
//! Pre-indicizza offset comuni per trasformare memcmp da O(n) a O(1):
//! - Offset 0: Discriminator (8 bytes)
//! - Offset 32: Owner (32 bytes) per Token Program
//! - Offset 64: Mint (32 bytes) per Token Accounts
//! - Offset 1: Collection key (32 bytes) per Metaplex NFT
//!
//! Invece di scansionare tutti gli account, usa bitmap lookup diretto.

use std::collections::BTreeMap;
use std::path::PathBuf;
use solana_sdk::pubkey::Pubkey;

use hyperplane_types::PubkeyBitmap;

/// Offset pre-indicizzati comuni
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommonOffset {
    /// Discriminator: offset 0, 8 bytes
    Discriminator,
    /// Owner: offset 32, 32 bytes (Token Program, Metaplex, etc.)
    Owner,
    /// Mint: offset 64, 32 bytes (Token Accounts)
    Mint,
    /// Collection: offset 1, 32 bytes (Metaplex NFT)
    Collection,
    /// Custom offset
    Custom(usize),
}

impl CommonOffset {
    /// Ritorna l'offset numerico
    pub fn offset(&self) -> usize {
        match self {
            CommonOffset::Discriminator => 0,
            CommonOffset::Owner => 32,
            CommonOffset::Mint => 64,
            CommonOffset::Collection => 1,
            CommonOffset::Custom(offset) => *offset,
        }
    }
    
    /// Ritorna la dimensione attesa in bytes
    pub fn expected_size(&self) -> usize {
        match self {
            CommonOffset::Discriminator => 8,
            CommonOffset::Owner => 32,
            CommonOffset::Mint => 32,
            CommonOffset::Collection => 32,
            CommonOffset::Custom(_) => 0, // Variabile
        }
    }
}

/// Index per un offset specifico
/// Mappa: bytes → bitmap di account_id
#[derive(Debug, Clone, Default)]
pub struct OffsetIndex {
    /// (bytes) → bitmap di account_id
    index: BTreeMap<Vec<u8>, PubkeyBitmap>,
    
    /// Numero totale di entry
    entry_count: u64,
}

impl OffsetIndex {
    pub fn new() -> Self {
        Self {
            index: BTreeMap::new(),
            entry_count: 0,
        }
    }
    
    /// Inserisce un account nell'index
    pub fn insert(&mut self, bytes: &[u8], account_id: u64) {
        let bitmap = self.index.entry(bytes.to_vec()).or_insert_with(PubkeyBitmap::new);
        bitmap.insert(account_id);
        self.entry_count += 1;
    }
    
    /// Rimuove un account dall'index
    pub fn remove(&mut self, bytes: &[u8], account_id: u64) {
        if let Some(bitmap) = self.index.get_mut(bytes) {
            bitmap.remove(account_id);
            self.entry_count = self.entry_count.saturating_sub(1);
        }
    }
    
    /// Cerca account per bytes esatti
    pub fn get(&self, bytes: &[u8]) -> Option<PubkeyBitmap> {
        self.index.get(bytes).cloned()
    }
    
    /// Ritorna tutte le chiavi (bytes) presenti
    pub fn keys(&self) -> Vec<Vec<u8>> {
        self.index.keys().cloned().collect()
    }
    
    /// Numero di entry uniche
    pub fn len(&self) -> usize {
        self.index.len()
    }
    
    /// Statistics
    pub fn stats(&self) -> OffsetIndexStats {
        OffsetIndexStats {
            unique_keys: self.index.len() as u64,
            total_entries: self.entry_count,
            avg_entries_per_key: if self.index.is_empty() {
                0.0
            } else {
                self.entry_count as f64 / self.index.len() as f64
            },
        }
    }
}

/// Statistics per OffsetIndex
#[derive(Debug, Clone)]
pub struct OffsetIndexStats {
    pub unique_keys: u64,
    pub total_entries: u64,
    pub avg_entries_per_key: f64,
}

/// Memcmp Accelerator per un programma specifico
#[derive(Debug)]
pub struct ProgramMemcmpAccelerator {
    /// Programma ID
    program_id: Pubkey,
    
    /// Offset indexes: offset → OffsetIndex
    offset_indexes: BTreeMap<usize, OffsetIndex>,
    
    /// Offset pre-configurati da indicizzare
    tracked_offsets: Vec<CommonOffset>,
}

impl ProgramMemcmpAccelerator {
    pub fn new(program_id: Pubkey, tracked_offsets: Vec<CommonOffset>) -> Self {
        Self {
            program_id,
            offset_indexes: BTreeMap::new(),
            tracked_offsets,
        }
    }
    
    /// Inserisce un account negli index appropriati
    pub fn insert_account(&mut self, account_id: u64, data: &[u8]) {
        for offset_def in &self.tracked_offsets {
            let offset = offset_def.offset();
            let size = offset_def.expected_size();
            
            // Skip se data troppo piccolo
            if data.len() < offset + size {
                continue;
            }
            
            // Estrai bytes
            let bytes = if size > 0 {
                &data[offset..offset + size]
            } else {
                // Custom offset: usa size variabile
                continue; // Per ora skip custom size
            };
            
            // Inserisci nell'index per questo offset
            let offset_index = self.offset_indexes.entry(offset).or_insert_with(OffsetIndex::new);
            offset_index.insert(bytes, account_id);
        }
    }
    
    /// Rimuove un account dagli index
    pub fn remove_account(&mut self, account_id: u64, data: &[u8]) {
        for offset_def in &self.tracked_offsets {
            let offset = offset_def.offset();
            let size = offset_def.expected_size();
            
            if data.len() < offset + size {
                continue;
            }
            
            let bytes = if size > 0 {
                &data[offset..offset + size]
            } else {
                continue;
            };
            
            if let Some(offset_index) = self.offset_indexes.get_mut(&offset) {
                offset_index.remove(bytes, account_id);
            }
        }
    }
    
    /// Cerca account per offset e bytes
    pub fn query(&self, offset: usize, bytes: &[u8]) -> Option<PubkeyBitmap> {
        self.offset_indexes.get(&offset)?.get(bytes)
    }
    
    /// Statistics per tutti gli offset
    pub fn stats(&self) -> BTreeMap<usize, OffsetIndexStats> {
        self.offset_indexes
            .iter()
            .map(|(offset, index)| (*offset, index.stats()))
            .collect()
    }
    
    /// Numero di offset indicizzati
    pub fn indexed_offset_count(&self) -> usize {
        self.offset_indexes.len()
    }
}

/// Memcmp Accelerator globale (tutti i programmi)
pub struct MemcmpAccelerator {
    /// Program accelerators
    program_accelerators: BTreeMap<Pubkey, ProgramMemcmpAccelerator>,
    
    /// Offset pre-configurati per programma
    program_offset_configs: BTreeMap<Pubkey, Vec<CommonOffset>>,
    
    /// Path per persistenza (opzionale)
    _accelerator_path: PathBuf,
}

impl MemcmpAccelerator {
    pub fn new(accelerator_path: PathBuf) -> Self {
        Self {
            program_accelerators: BTreeMap::new(),
            program_offset_configs: BTreeMap::new(),
            _accelerator_path: accelerator_path,
        }
    }
    
    /// Configura offset da indicizzare per un programma
    pub fn configure_program(&mut self, program_id: Pubkey, offsets: Vec<CommonOffset>) {
        self.program_offset_configs.insert(program_id, offsets.clone());
        
        // Crea accelerator se non esiste
        self.program_accelerators
            .entry(program_id)
            .or_insert_with(|| ProgramMemcmpAccelerator::new(program_id, offsets));
    }
    
    /// Inserisce un account
    pub fn insert_account(&mut self, program_id: Pubkey, account_id: u64, data: &[u8]) {
        // Assicurati che l'accelerator esista
        if !self.program_accelerators.contains_key(&program_id) {
            let offsets = self.program_offset_configs
                .get(&program_id)
                .cloned()
                .unwrap_or_else(|| vec![
                    CommonOffset::Discriminator,
                    CommonOffset::Owner,
                ]);
            
            self.program_accelerators.insert(
                program_id,
                ProgramMemcmpAccelerator::new(program_id, offsets),
            );
        }
        
        // Inserisci nell'accelerator
        if let Some(accelerator) = self.program_accelerators.get_mut(&program_id) {
            accelerator.insert_account(account_id, data);
        }
    }
    
    /// Rimuove un account
    pub fn remove_account(&mut self, program_id: Pubkey, account_id: u64, data: &[u8]) {
        if let Some(accelerator) = self.program_accelerators.get_mut(&program_id) {
            accelerator.remove_account(account_id, data);
        }
    }
    
    /// Query accelerata: offset + bytes → bitmap
    pub fn query(&self, program_id: Pubkey, offset: usize, bytes: &[u8]) -> Option<PubkeyBitmap> {
        self.program_accelerators.get(&program_id)?.query(offset, bytes)
    }
    
    /// Statistics per programma
    pub fn get_program_stats(&self, program_id: Pubkey) -> Option<BTreeMap<usize, OffsetIndexStats>> {
        self.program_accelerators.get(&program_id).map(|acc| acc.stats())
    }
    
    /// Statistics globali
    pub fn global_stats(&self) -> MemcmpAcceleratorStats {
        let mut total_programs = 0;
        let mut total_offsets = 0u64;  // Fix: usa u64 esplicitamente
        let mut total_entries = 0;
        
        for accelerator in self.program_accelerators.values() {
            total_programs += 1;
            total_offsets += accelerator.indexed_offset_count() as u64;
            
            for stats in accelerator.stats().values() {
                total_entries += stats.total_entries;
            }
        }
        
        MemcmpAcceleratorStats {
            total_programs,
            total_offsets,
            total_entries,
        }
    }
}

/// Statistics globali per MemcmpAccelerator
#[derive(Debug, Clone)]
pub struct MemcmpAcceleratorStats {
    pub total_programs: u64,
    pub total_offsets: u64,
    pub total_entries: u64,
}

/// Configurazione predefinita per programmi noti
pub mod predefined_configs {
    use super::*;
    
    /// Token Program offsets
    pub fn token_program() -> Vec<CommonOffset> {
        vec![
            CommonOffset::Discriminator,  // 0: discriminator (8 bytes)
            CommonOffset::Owner,          // 32: owner (32 bytes)
            CommonOffset::Mint,           // 64: mint (32 bytes)
        ]
    }
    
    /// Metaplex NFT offsets
    pub fn metaplex_nft() -> Vec<CommonOffset> {
        vec![
            CommonOffset::Discriminator,  // 0: discriminator (8 bytes)
            CommonOffset::Collection,     // 1: collection key (32 bytes)
        ]
    }
    
    /// Default configuration (solo discriminator)
    pub fn default_config() -> Vec<CommonOffset> {
        vec![CommonOffset::Discriminator]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memcmp_accelerator_basic() {
        let mut accelerator = MemcmpAccelerator::new(PathBuf::from("/tmp/memcmp"));
        
        // Configura Token Program
        let token_program = Pubkey::new_unique();
        accelerator.configure_program(token_program, predefined_configs::token_program());
        
        // Inserisci account simulati
        let mut data = vec![0u8; 100];
        data[0..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]); // Discriminator
        data[32..64].copy_from_slice(&[9; 32]); // Owner
        data[64..96].copy_from_slice(&[10; 32]); // Mint
        
        accelerator.insert_account(token_program, 1, &data);
        
        // Query per discriminator
        let result = accelerator.query(token_program, 0, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(result.is_some());
        assert!(result.unwrap().contains(1));
        
        // Query per owner
        let result = accelerator.query(token_program, 32, &[9; 32]);
        assert!(result.is_some());
        
        // Query per mint
        let result = accelerator.query(token_program, 64, &[10; 32]);
        assert!(result.is_some());
        
        // Query non esistente
        let result = accelerator.query(token_program, 0, &[0; 8]);
        assert!(result.is_none());
    }
    
    #[test]
    fn test_offset_index_stats() {
        let mut index = OffsetIndex::new();
        
        // Inserisci 100 account con stesso bytes
        for i in 0..100 {
            index.insert(&[1, 2, 3], i);
        }
        
        let stats = index.stats();
        assert_eq!(stats.unique_keys, 1);
        assert_eq!(stats.total_entries, 100);
        assert_eq!(stats.avg_entries_per_key, 100.0);
    }
}
