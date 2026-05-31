//! Delta Plane Locator
//!
//! Tracks location of delta segments and provides fast lookup for account updates.
#![allow(unused_imports)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use solana_sdk::pubkey::Pubkey;
use hyperplane_types::AccountLocation;
use hyperplane_types::StorageType;

/// Metadata for a delta segment
#[derive(Debug, Clone)]
pub struct DeltaSegmentMeta {
    pub path: PathBuf,
    pub start_slot: u64,
    pub end_slot: u64,
    pub entry_count: u64,
    pub data_size: u64,
    pub created_at: std::time::SystemTime,
}

/// Delta locator state
#[derive(Debug, Default)]
pub struct DeltaLocatorState {
    /// Map of slot -> segment metadata (sorted by slot)
    segments_by_slot: BTreeMap<u64, DeltaSegmentMeta>,
    /// Map of pubkey -> latest location in delta layer
    latest_locations: BTreeMap<Pubkey, AccountLocation>,
    /// Total number of delta entries
    total_entries: u64,
}

/// Delta Locator for tracking delta segments
#[allow(dead_code)]
pub struct DeltaLocator {
    state: Arc<RwLock<DeltaLocatorState>>,
    delta_path: PathBuf,
}

impl DeltaLocator {
    /// Create a new delta locator
    pub fn new(delta_path: PathBuf) -> Self {
        Self {
            state: Arc::new(RwLock::new(DeltaLocatorState::default())),
            delta_path,
        }
    }
    
    /// Register a new delta segment
    pub fn register_segment(&self, meta: DeltaSegmentMeta) {
        let mut state = self.state.write();
        state.segments_by_slot.insert(meta.start_slot, meta);
    }
    
    /// Register an account update in delta layer
    pub fn register_update(&self, pubkey: Pubkey, location: AccountLocation) {
        let mut state = self.state.write();
        state.latest_locations.insert(pubkey, location);
        state.total_entries += 1;
    }
    
    /// Get latest location for a pubkey from delta layer
    pub fn get_latest(&self, pubkey: &Pubkey) -> Option<AccountLocation> {
        let state = self.state.read();
        state.latest_locations.get(pubkey).copied()
    }
    
    /// Get all segments for a slot range
    pub fn get_segments_for_range(&self, start_slot: u64, end_slot: u64) -> Vec<DeltaSegmentMeta> {
        let state = self.state.read();
        state.segments_by_slot
            .range(start_slot..=end_slot)
            .map(|(_, meta)| meta.clone())
            .collect()
    }
    
    /// Get latest slot in delta layer
    pub fn get_latest_slot(&self) -> Option<u64> {
        let state = self.state.read();
        state.segments_by_slot.last_key_value().map(|(slot, _)| *slot)
    }
    
    /// Get total entry count
    pub fn entry_count(&self) -> u64 {
        let state = self.state.read();
        state.total_entries
    }
    
    /// Get all segments
    pub fn get_all_segments(&self) -> Vec<DeltaSegmentMeta> {
        let state = self.state.read();
        state.segments_by_slot.values().cloned().collect()
    }
    
    /// Remove old segments (compaction cleanup)
    pub fn remove_segments(&self, slots: &[u64]) {
        let mut state = self.state.write();
        for slot in slots {
            state.segments_by_slot.remove(slot);
        }
    }
    
    /// Clear delta state (after compaction to base)
    pub fn clear(&self) {
        let mut state = self.state.write();
        state.segments_by_slot.clear();
        state.latest_locations.clear();
        state.total_entries = 0;
    }
}

/// Delta locator builder
pub struct DeltaLocatorBuilder {
    delta_path: PathBuf,
}

impl DeltaLocatorBuilder {
    pub fn new(delta_path: PathBuf) -> Self {
        Self { delta_path }
    }
    
    pub fn build(self) -> DeltaLocator {
        DeltaLocator::new(self.delta_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_delta_locator_basic() {
        let locator = DeltaLocator::new(PathBuf::from("/tmp/delta"));
        
        // Register segment
        let meta = DeltaSegmentMeta {
            path: PathBuf::from("/tmp/delta/segment_0.bin"),
            start_slot: 100,
            end_slot: 200,
            entry_count: 1000,
            data_size: 1024 * 1024,
            created_at: std::time::SystemTime::now(),
        };
        locator.register_segment(meta);
        
        // Register update
        let pubkey = Pubkey::new_unique();
        let location = AccountLocation {
            file_id: 1,
            offset: 1024,
            stored_size: 100,
            data_offset: 50,
            data_len: 100,
            slot: 150,
            write_version: 1,
            storage_type: StorageType::Delta,
        };
        locator.register_update(pubkey, location);
        
        // Query
        assert_eq!(locator.get_latest(&pubkey), Some(location));
        assert_eq!(locator.entry_count(), 1);
        
        let segments = locator.get_segments_for_range(100, 200);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].start_slot, 100);
    }
}
