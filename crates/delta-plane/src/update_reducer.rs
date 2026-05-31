//! Update Reducer
//!
//! Reduces multiple updates to same account by keeping only the latest version.

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use solana_sdk::pubkey::Pubkey;

use hyperplane_types::AccountLocation;

/// Pending update waiting to be flushed
#[derive(Debug, Clone)]
pub struct PendingUpdate {
    pub slot: u64,
    pub write_version: u64,
    pub location: AccountLocation,
    pub timestamp: std::time::SystemTime,
}

/// Update reducer state
#[derive(Debug, Default)]
pub struct UpdateReducerState {
    /// Map of pubkey -> latest pending update
    pending_updates: HashMap<Pubkey, PendingUpdate>,
    /// Number of updates flushed
    flushed_count: u64,
    /// Number of updates deduplicated
    dedup_count: u64,
}

/// Update Reducer for deduplicating account updates
pub struct UpdateReducer {
    state: Arc<RwLock<UpdateReducerState>>,
    /// Flush threshold (number of pending updates before flush)
    flush_threshold: usize,
}

impl UpdateReducer {
    /// Create a new update reducer with default threshold
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(UpdateReducerState::default())),
            flush_threshold: 10_000,
        }
    }
    
    /// Create with custom flush threshold
    pub fn with_threshold(flush_threshold: usize) -> Self {
        Self {
            state: Arc::new(RwLock::new(UpdateReducerState::default())),
            flush_threshold,
        }
    }
    
    /// Add an update to the reducer
    pub fn add_update(&self, pubkey: Pubkey, slot: u64, write_version: u64, location: AccountLocation) -> bool {
        let mut state = self.state.write();
        
        // Check if we already have a pending update for this pubkey
        let is_dedup = state.pending_updates.contains_key(&pubkey);
        
        if is_dedup {
            state.dedup_count += 1;
        }
        
        // Always keep the latest update (highest write_version)
        let should_update = match state.pending_updates.get(&pubkey) {
            Some(existing) => write_version > existing.write_version,
            None => true,
        };
        
        if should_update {
            state.pending_updates.insert(
                pubkey,
                PendingUpdate {
                    slot,
                    write_version,
                    location,
                    timestamp: std::time::SystemTime::now(),
                },
            );
        }
        
        is_dedup
    }
    
    /// Get pending updates ready for flush
    pub fn get_pending_updates(&self) -> Vec<(Pubkey, PendingUpdate)> {
        let state = self.state.read();
        state.pending_updates.iter().map(|(k, v)| (*k, v.clone())).collect()
    }
    
    /// Check if we should flush (reached threshold)
    pub fn should_flush(&self) -> bool {
        let state = self.state.read();
        state.pending_updates.len() >= self.flush_threshold
    }
    
    /// Flush pending updates (clear from pending)
    pub fn flush(&self) -> Vec<(Pubkey, PendingUpdate)> {
        let mut state = self.state.write();
        let updates: Vec<_> = state.pending_updates.drain().collect();
        state.flushed_count += updates.len() as u64;
        updates
    }
    
    /// Get statistics
    pub fn stats(&self) -> ReducerStats {
        let state = self.state.read();
        ReducerStats {
            pending_count: state.pending_updates.len() as u64,
            flushed_count: state.flushed_count,
            dedup_count: state.dedup_count,
        }
    }
    
    /// Get pending count
    pub fn pending_count(&self) -> usize {
        let state = self.state.read();
        state.pending_updates.len()
    }
}

impl Default for UpdateReducer {
    fn default() -> Self {
        Self::new()
    }
}

/// Reducer statistics
#[derive(Debug, Clone, Default)]
pub struct ReducerStats {
    pub pending_count: u64,
    pub flushed_count: u64,
    pub dedup_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_update_reducer_dedup() {
        let reducer = UpdateReducer::new();
        
        let pubkey = Pubkey::new_unique();
        let location1 = AccountLocation {
            file_id: 1,
            offset: 1024,
            stored_size: 100,
            data_offset: 50,
            data_len: 100,
            slot: 100,
            write_version: 1,
            storage_type: hyperplane_types::StorageType::Delta,
        };
        
        // First update
        assert!(!reducer.add_update(pubkey, 100, 1, location1));
        assert_eq!(reducer.pending_count(), 1);
        
        // Second update (same pubkey, higher write_version) - should dedup
        let location2 = AccountLocation {
            file_id: 1,
            offset: 2048,
            stored_size: 100,
            data_offset: 50,
            data_len: 100,
            slot: 101,
            write_version: 2,
            storage_type: hyperplane_types::StorageType::Delta,
        };
        assert!(reducer.add_update(pubkey, 101, 2, location2));
        assert_eq!(reducer.pending_count(), 1); // Still 1, deduplicated
        
        // Get pending - should have latest version
        let pending = reducer.get_pending_updates();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1.write_version, 2);
        
        // Stats
        let stats = reducer.stats();
        assert_eq!(stats.dedup_count, 1);
    }
    
    #[test]
    fn test_update_reducer_flush() {
        let reducer = UpdateReducer::with_threshold(3);
        
        // Add 3 updates
        for i in 0..3 {
            let pubkey = Pubkey::new_unique();
            let location = AccountLocation {
                file_id: 1,
                offset: 1024 * i,
                stored_size: 100,
                data_offset: 50,
                data_len: 100,
                slot: 100 + i,
                write_version: 1,
                storage_type: hyperplane_types::StorageType::Delta,
            };
            reducer.add_update(pubkey, 100 + i, 1, location);
        }
        
        // Should flush
        assert!(reducer.should_flush());
        
        // Flush
        let updates = reducer.flush();
        assert_eq!(updates.len(), 3);
        assert_eq!(reducer.pending_count(), 0);
        
        let stats = reducer.stats();
        assert_eq!(stats.flushed_count, 3);
    }
}
