//! Compactor
//!
//! Compacts delta segments into base layer by merging updates.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use parking_lot::RwLock;
use solana_sdk::pubkey::Pubkey;

use hyperplane_types::{AccountLocation, StorageType};
use base_locator::{PersistentPubkeyDictionary, RocksLocator};
use crate::delta_locator::DeltaLocator;

/// Compaction result
#[derive(Debug, Clone)]
pub struct CompactionResult {
    pub segments_compacted: u64,
    pub accounts_merged: u64,
    pub new_base_locations: u64,
    pub bytes_freed: u64,
}

/// Compactor state
#[derive(Debug, Default)]
pub struct CompactorState {
    /// Number of compactions performed
    compaction_count: u64,
    /// Total accounts merged
    total_merged: u64,
    /// Last compaction timestamp
    last_compaction: Option<std::time::SystemTime>,
}

/// Compactor for merging delta segments into base layer
pub struct Compactor {
    state: Arc<RwLock<CompactorState>>,
    /// Minimum number of delta segments before compaction
    min_segments: usize,
    /// Minimum number of delta entries before compaction
    min_entries: u64,
}

impl Compactor {
    /// Create a new compactor with default thresholds
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(CompactorState::default())),
            min_segments: 10,
            min_entries: 100_000,
        }
    }
    
    /// Create with custom thresholds
    pub fn with_thresholds(min_segments: usize, min_entries: u64) -> Self {
        Self {
            state: Arc::new(RwLock::new(CompactorState::default())),
            min_segments,
            min_entries,
        }
    }
    
    /// Check if compaction should run
    pub fn should_compact(&self, delta_locator: &DeltaLocator) -> bool {
        let segments = delta_locator.get_all_segments();
        let total_entries: u64 = segments.iter().map(|s| s.entry_count).sum();
        
        segments.len() >= self.min_segments || total_entries >= self.min_entries
    }
    
    /// Run compaction
    pub fn compact(
        &self,
        delta_locator: &DeltaLocator,
        _base_locator: &RocksLocator,
        dictionary: &PersistentPubkeyDictionary,
        _output_path: &Path,
    ) -> Result<CompactionResult, CompactionError> {
        let mut state = self.state.write();
        
        // Get all delta segments
        let segments = delta_locator.get_all_segments();
        if segments.is_empty() {
            return Ok(CompactionResult {
                segments_compacted: 0,
                accounts_merged: 0,
                new_base_locations: 0,
                bytes_freed: 0,
            });
        }
        
        // Collect all delta locations by pubkey (keep latest)
        let _segments = delta_locator.get_all_segments();
        let latest_deltas: HashMap<Pubkey, AccountLocation> = HashMap::new();
        
        // Merge with base layer
        #[allow(unused_mut)]
        let mut new_locations = Vec::new();
        for (pubkey, delta_location) in &latest_deltas {
            // Get dictionary ID for pubkey
            let dict_id = dictionary.get_id(pubkey)?;
            
            // Get base location (if exists)
            if let Some(base_location) = base_locator.get(*pubkey)? {
                new_locations.push((dict_id, base_location));
            }
        }
        
        // Update state
        state.segments_compacted += segments.len() as u64;
        state.total_merged += new_locations.len() as u64;
        for (pubkey, delta_location) in &latest_deltas {
            // Get dictionary ID for pubkey
            let dict_id = dictionary.get_id(pubkey)?
                .ok_or(CompactionError::PubkeyNotFound(*pubkey))?;
            
            // Create new base location
            let base_location = AccountLocation {
                file_id: 0, // Will be assigned by segment writer
                offset: 0,  // Will be assigned
                stored_size: delta_location.stored_size,
                data_offset: delta_location.data_offset,
                data_len: delta_location.data_len,
                slot: delta_location.slot,
                write_version: delta_location.write_version,
                storage_type: StorageType::Compacted,
            };
            
            new_locations.push((dict_id, base_location));
        }
        
        // Write compacted segment
        // TODO: Implement segment writer for compacted data
        
        // Update base locator with new locations
        // TODO: Batch insert into base_locator
        
        // Clear delta segments
        let old_slots: Vec<u64> = segments.iter().map(|s| s.start_slot).collect();
        delta_locator.remove_segments(&old_slots);
        
        // Update stats
        state.compaction_count += 1;
        state.total_merged += new_locations.len() as u64;
        state.last_compaction = Some(std::time::SystemTime::now());
        
        Ok(CompactionResult {
            segments_compacted: segments.len() as u64,
            accounts_merged: new_locations.len() as u64,
            new_base_locations: new_locations.len() as u64,
            bytes_freed: 0, // TODO: Calculate from segment sizes
        })
    }
    
    /// Get compaction statistics
    pub fn stats(&self) -> CompactorStats {
        let state = self.state.read();
        CompactorStats {
            compaction_count: state.compaction_count,
            total_merged: state.total_merged,
            last_compaction: state.last_compaction,
        }
    }
}

impl Default for Compactor {
    fn default() -> Self {
        Self::new()
    }
}

/// Compactor statistics
#[derive(Debug, Clone)]
pub struct CompactorStats {
    pub compaction_count: u64,
    pub total_merged: u64,
    pub last_compaction: Option<std::time::SystemTime>,
}

/// Compaction errors
#[derive(Debug, thiserror::Error)]
pub enum CompactionError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Pubkey not found in dictionary: {0}")]
    PubkeyNotFound(Pubkey),
    
    #[error("Base locator error: {0}")]
    BaseLocator(#[from] base_locator::LocatorError),
    
    #[error("Dictionary error: {0}")]
    Dictionary(#[from] base_locator::DictionaryError),
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compactor_should_compact() {
        let compactor = Compactor::with_thresholds(5, 1000);
        let delta_locator = DeltaLocator::new(std::path::PathBuf::from("/tmp/delta"));
        
        // Not enough segments
        assert!(!compactor.should_compact(&delta_locator));
        
        // TODO: Add segments and test threshold
    }
}
