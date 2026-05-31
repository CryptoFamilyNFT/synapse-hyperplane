//! Cache Invalidation System
//!
//! Tracks which cached entries need to be invalidated when accounts change.
//! Uses bitmap-based tracking for efficient batch invalidations.

use hyperplane_types::PubkeyBitmap;
use parking_lot::RwLock;
use solana_sdk::pubkey::Pubkey;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Invalidation tracker
/// 
/// Tracks materialized queries and which pubkeys affect them
pub struct InvalidationTracker {
    /// query_hash -> set of affected pubkeys
    query_pubkeys: RwLock<HashMap<[u8; 32], HashSet<Pubkey>>>,
    
    /// pubkey -> set of affected query hashes
    pubkey_queries: RwLock<HashMap<Pubkey, HashSet<[u8; 32]>>>,
    
    /// Materialized query cache (query_hash -> cached_result)
    materialized_cache: RwLock<HashMap<[u8; 32], MaterializedQueryResult>>,
}

impl InvalidationTracker {
    pub fn new() -> Self {
        Self {
            query_pubkeys: RwLock::new(HashMap::new()),
            pubkey_queries: RwLock::new(HashMap::new()),
            materialized_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Register a materialized query
    pub fn register_query(
        &self,
        query_hash: [u8; 32],
        affected_pubkeys: &[Pubkey],
    ) {
        // Register query -> pubkeys mapping
        {
            let mut map = self.query_pubkeys.write();
            map.insert(query_hash, affected_pubkeys.iter().cloned().collect());
        }
        
        // Register pubkey -> queries mapping
        {
            let mut map = self.pubkey_queries.write();
            for pubkey in affected_pubkeys {
                map.entry(*pubkey)
                    .or_insert_with(HashSet::new)
                    .insert(query_hash);
            }
        }
    }

    /// Get queries affected by pubkey changes
    pub fn get_affected_queries(&self, pubkey: Pubkey) -> Vec<[u8; 32]> {
        let map = self.pubkey_queries.read();
        map.get(&pubkey)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get queries affected by multiple pubkey changes
    pub fn get_affected_queries_batch(&self, pubkeys: &[Pubkey]) -> HashSet<[u8; 32]> {
        let map = self.pubkey_queries.read();
        let mut affected = HashSet::new();
        
        for pubkey in pubkeys {
            if let Some(queries) = map.get(pubkey) {
                affected.extend(queries);
            }
        }
        
        affected
    }

    /// Invalidate queries affected by pubkey changes
    pub fn invalidate_for_pubkeys(&self, pubkeys: &[Pubkey]) -> usize {
        let affected_queries = self.get_affected_queries_batch(pubkeys);
        
        {
            let mut cache = self.materialized_cache.write();
            let mut query_map = self.query_pubkeys.write();
            let mut pubkey_map = self.pubkey_queries.write();
            
            for query_hash in &affected_queries {
                // Remove from materialized cache
                cache.remove(query_hash);
                
                // Remove from query_pubkeys
                if let Some(pubkeys) = query_map.remove(query_hash) {
                    // Clean up pubkey -> query mappings
                    for pubkey in &pubkeys {
                        if let Some(queries) = pubkey_map.get_mut(pubkey) {
                            queries.remove(query_hash);
                            if queries.is_empty() {
                                pubkey_map.remove(pubkey);
                            }
                        }
                    }
                }
            }
        }
        
        affected_queries.len()
    }

    /// Cache materialized query result
    pub fn cache_materialized_result(
        &self,
        query_hash: [u8; 32],
        result: MaterializedQueryResult,
    ) {
        self.materialized_cache.write().insert(query_hash, result);
    }

    /// Get materialized query result
    pub fn get_materialized_result(
        &self,
        query_hash: [u8; 32],
    ) -> Option<MaterializedQueryResult> {
        self.materialized_cache.read().get(&query_hash).cloned()
    }

    /// Get number of tracked queries
    pub fn query_count(&self) -> usize {
        self.query_pubkeys.read().len()
    }

    /// Get number of materialized results
    pub fn materialized_count(&self) -> usize {
        self.materialized_cache.read().len()
    }

    /// Clear all tracking data
    pub fn clear(&self) {
        self.query_pubkeys.write().clear();
        self.pubkey_queries.write().clear();
        self.materialized_cache.write().clear();
    }
}

impl Default for InvalidationTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Materialized query result
#[derive(Debug, Clone)]
pub struct MaterializedQueryResult {
    /// Encoded response
    pub response: Arc<Vec<u8>>,
    /// Slot when materialized
    pub materialized_slot: u64,
    /// Affected pubkey bitmap (for fast invalidation checks)
    pub affected_bitmap: PubkeyBitmap,
    /// Materialization time
    pub materialized_at: std::time::Instant,
}

impl MaterializedQueryResult {
    pub fn new(
        response: Vec<u8>,
        slot: u64,
        affected_bitmap: PubkeyBitmap,
    ) -> Self {
        Self {
            response: Arc::new(response),
            materialized_slot: slot,
            affected_bitmap,
            materialized_at: std::time::Instant::now(),
        }
    }

    /// Check if result is stale
    pub fn is_stale(&self, current_slot: u64) -> bool {
        // Consider stale if more than 100 slots have passed
        current_slot > self.materialized_slot + 100
    }

    /// Get age in slots
    pub fn age_slots(&self, current_slot: u64) -> u64 {
        current_slot - self.materialized_slot
    }
}

/// Invalidation strategy
#[derive(Debug, Clone, Copy)]
pub enum InvalidationStrategy {
    /// Immediate invalidation on every update
    Immediate,
    /// Batch invalidations (accumulate then invalidate)
    Batch { max_batch_size: usize, max_delay_ms: u64 },
    /// Lazy invalidation (invalidate on next read)
    Lazy,
}

impl Default for InvalidationStrategy {
    fn default() -> Self {
        Self::Immediate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalidation_tracking() {
        let tracker = InvalidationTracker::new();
        
        let pubkey1 = Pubkey::new_unique();
        let pubkey2 = Pubkey::new_unique();
        let query_hash = [1u8; 32];
        
        // Register query
        tracker.register_query(query_hash, &[pubkey1, pubkey2]);
        
        // Check affected queries
        let affected = tracker.get_affected_queries(pubkey1);
        assert_eq!(affected.len(), 1);
        assert_eq!(affected[0], query_hash);
        
        // Invalidate
        let count = tracker.invalidate_for_pubkeys(&[pubkey1]);
        assert_eq!(count, 1);
        
        // Should be invalidated now
        let affected = tracker.get_affected_queries(pubkey1);
        assert_eq!(affected.len(), 0);
    }

    #[test]
    fn test_batch_invalidation() {
        let tracker = InvalidationTracker::new();
        
        let pubkeys: Vec<Pubkey> = (0..10).map(|_| Pubkey::new_unique()).collect();
        let query_hash1 = [1u8; 32];
        let query_hash2 = [2u8; 32];
        
        // Register two queries with overlapping pubkeys
        tracker.register_query(query_hash1, &pubkeys[0..5]);
        tracker.register_query(query_hash2, &pubkeys[3..8]);
        
        // Invalidate pubkeys that affect both queries
        let affected = tracker.get_affected_queries_batch(&pubkeys[3..5]);
        assert!(affected.contains(&query_hash1));
        assert!(affected.contains(&query_hash2));
        
        let count = tracker.invalidate_for_pubkeys(&pubkeys[3..5]);
        assert_eq!(count, 2);
    }
}
