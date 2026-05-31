//! L1 Hot Cache - In-memory cache for hot accounts
//!
//! Uses DashMap for lock-free concurrent access.
//! Optimized for getAccountInfo hot paths.

use dashmap::DashMap;
use hyperplane_types::{AccountEncoding, AccountView, CommitmentLevel};
use parking_lot::RwLock;
use solana_sdk::pubkey::Pubkey;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// L1 cache entry
#[derive(Debug, Clone)]
pub struct L1CacheEntry {
    /// Encoded account data
    pub data: Arc<Vec<u8>>,
    /// Encoding format
    pub encoding: AccountEncoding,
    /// Slot when cached
    pub cached_slot: u64,
    /// Write version when cached
    pub cached_write_version: u64,
    /// Cache insertion time
    pub cached_at: Instant,
    /// Access count (for LRU)
    pub access_count: Arc<AtomicU64>,
}

impl L1CacheEntry {
    pub fn new(
        data: Arc<Vec<u8>>,
        encoding: AccountEncoding,
        slot: u64,
        write_version: u64,
    ) -> Self {
        Self {
            data,
            encoding,
            cached_slot: slot,
            cached_write_version: write_version,
            cached_at: Instant::now(),
            access_count: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Check if entry is stale
    pub fn is_stale(&self, current_slot: u64, current_write_version: u64) -> bool {
        if current_slot > self.cached_slot {
            return true;
        }
        if current_slot == self.cached_slot && current_write_version > self.cached_write_version {
            return true;
        }
        false
    }

    /// Increment access count
    pub fn touch(&self) {
        self.access_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get access count
    pub fn access_count(&self) -> u64 {
        self.access_count.load(Ordering::Relaxed)
    }

    /// Get age in milliseconds
    pub fn age_ms(&self) -> u128 {
        self.cached_at.elapsed().as_millis()
    }
}

/// Cache key (pubkey + encoding + commitment)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub pubkey: Pubkey,
    pub encoding: AccountEncoding,
    pub commitment: CommitmentLevel,
}

impl CacheKey {
    pub fn new(pubkey: Pubkey, encoding: AccountEncoding, commitment: CommitmentLevel) -> Self {
        Self {
            pubkey,
            encoding,
            commitment,
        }
    }

    /// Generate cache key from account view and context
    pub fn from_account(account: &AccountView, encoding: AccountEncoding, commitment: CommitmentLevel) -> Self {
        Self::new(account.pubkey, encoding, commitment)
    }
}

/// L1 hot cache configuration
#[derive(Debug, Clone)]
pub struct L1CacheConfig {
    /// Max entries before eviction
    pub max_entries: usize,
    /// TTL for processed commitment (ms)
    pub processed_ttl_ms: u64,
    /// TTL for confirmed commitment (ms)
    pub confirmed_ttl_ms: u64,
    /// TTL for finalized commitment (ms)
    pub finalized_ttl_ms: u64,
    /// Eviction batch size
    pub eviction_batch_size: usize,
}

impl Default for L1CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 1_000_000, // 1M entries
            processed_ttl_ms: 60_000, // 1 minute
            confirmed_ttl_ms: 300_000, // 5 minutes
            finalized_ttl_ms: 3_600_000, // 1 hour
            eviction_batch_size: 1000,
        }
    }
}

/// L1 hot account cache
pub struct L1HotCache {
    /// Cache storage
    cache: DashMap<CacheKey, L1CacheEntry>,
    /// Config
    config: L1CacheConfig,
    /// Stats
    stats: L1CacheStats,
    /// Eviction tracker (pubkey -> last_access)
    access_tracker: RwLock<DashMap<CacheKey, u64>>,
}

impl L1HotCache {
    pub fn new(config: L1CacheConfig) -> Self {
        Self {
            cache: DashMap::with_capacity(config.max_entries / 10),
            config,
            stats: L1CacheStats::default(),
            access_tracker: RwLock::new(DashMap::new()),
        }
    }

    /// Get cached entry
    pub fn get(&self, key: &CacheKey) -> Option<L1CacheEntry> {
        if let Some(entry) = self.cache.get(key) {
            entry.touch();
            self.stats.increment_hits();
            
            // Update access tracker (simple increment)
            let mut tracker = self.access_tracker.write();
            if let Some(mut access) = tracker.get_mut(key) {
                *access += 1;
            }
            
            Some(entry.clone())
        } else {
            self.stats.increment_misses();
            None
        }
    }

    /// Insert cache entry
    pub fn insert(&self, key: CacheKey, entry: L1CacheEntry) {
        // Check if we need eviction
        if self.cache.len() >= self.config.max_entries {
            self.evict_stale_entries();
        }
        
        // Update access tracker first (before key is moved)
        {
            let tracker = self.access_tracker.write();
            tracker.insert(key.clone(), 1);
        }
        
        self.cache.insert(key, entry);
        self.stats.increment_inserts();
    }

    /// Invalidate cache entry for pubkey
    pub fn invalidate(&self, pubkey: Pubkey) {
        // Remove all encodings/commitments for this pubkey
        let keys_to_remove: Vec<CacheKey> = self
            .cache
            .iter()
            .filter(|entry| entry.key().pubkey == pubkey)
            .map(|entry| entry.key().clone())
            .collect();
        
        let count = keys_to_remove.len();
        for key in keys_to_remove {
            self.cache.remove(&key);
        }
        
        self.stats.increment_invalidations_by(count);
    }

    /// Invalidate multiple pubkeys
    pub fn invalidate_batch(&self, pubkeys: &[Pubkey]) {
        let keys_to_remove: Vec<CacheKey> = self
            .cache
            .iter()
            .filter(|entry| pubkeys.contains(&entry.key().pubkey))
            .map(|entry| entry.key().clone())
            .collect();
        
        let count = keys_to_remove.len();
        for key in keys_to_remove {
            self.cache.remove(&key);
        }
        
        self.stats.increment_invalidations_by(count);
    }

    /// Evict stale entries
    fn evict_stale_entries(&self) {
        let mut removed = 0;
        
        // Remove entries older than TTL
        self.cache.retain(|key, entry| {
            let ttl_ms = match key.commitment {
                CommitmentLevel::Processed => self.config.processed_ttl_ms,
                CommitmentLevel::Confirmed => self.config.confirmed_ttl_ms,
                CommitmentLevel::Finalized => self.config.finalized_ttl_ms,
            };
            
            if entry.age_ms() > ttl_ms as u128 {
                removed += 1;
                false
            } else {
                true
            }
        });
        
        if removed > 0 {
            tracing::debug!("Evicted {} stale cache entries", removed);
        }
    }

    /// Get cache size
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Get stats
    pub fn stats(&self) -> &L1CacheStats {
        &self.stats
    }

    /// Clear all entries
    pub fn clear(&self) {
        self.cache.clear();
        self.access_tracker.write().clear();
    }

    /// Get hit rate
    pub fn hit_rate(&self) -> f64 {
        self.stats.hit_rate()
    }
}

/// Cache statistics
#[derive(Debug, Default)]
pub struct L1CacheStats {
    hits: AtomicU64,
    misses: AtomicU64,
    inserts: AtomicU64,
    evictions: AtomicU64,
    invalidations: AtomicU64,
}

impl L1CacheStats {
    fn increment_hits(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }
    
    fn increment_misses(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }
    
    fn increment_inserts(&self) {
        self.inserts.fetch_add(1, Ordering::Relaxed);
    }
    
    fn increment_invalidations(&self) {
        self.invalidations.fetch_add(1, Ordering::Relaxed);
    }
    
    fn increment_invalidations_by(&self, count: usize) {
        self.invalidations.fetch_add(count as u64, Ordering::Relaxed);
    }
    
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }
    
    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }
    
    pub fn inserts(&self) -> u64 {
        self.inserts.load(Ordering::Relaxed)
    }
    
    pub fn invalidations(&self) -> u64 {
        self.invalidations.load(Ordering::Relaxed)
    }
    
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits.load(Ordering::Relaxed) + self.misses.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            self.hits.load(Ordering::Relaxed) as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l1_cache_basic() {
        let cache = L1HotCache::new(L1CacheConfig::default());
        
        let pubkey = Pubkey::new_unique();
        let key = CacheKey::new(pubkey, AccountEncoding::Base64, CommitmentLevel::Processed);
        let entry = L1CacheEntry::new(
            Arc::new(vec![1, 2, 3, 4]),
            AccountEncoding::Base64,
            100,
            1,
        );
        
        // Insert
        cache.insert(key.clone(), entry);
        
        // Get
        let retrieved = cache.get(&key).unwrap();
        assert_eq!(retrieved.data.len(), 4);
        assert_eq!(retrieved.cached_slot, 100);
        
        // Stats
        assert_eq!(cache.stats.hits(), 1);
        assert_eq!(cache.stats.inserts(), 1);
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = L1HotCache::new(L1CacheConfig::default());
        
        let pubkey = Pubkey::new_unique();
        
        // Insert multiple encodings
        for encoding in [AccountEncoding::Base64, AccountEncoding::Base58] {
            let key = CacheKey::new(pubkey, encoding, CommitmentLevel::Processed);
            let entry = L1CacheEntry::new(
                Arc::new(vec![1, 2, 3]),
                encoding,
                100,
                1,
            );
            cache.insert(key, entry);
        }
        
        assert_eq!(cache.len(), 2);
        
        // Invalidate
        cache.invalidate(pubkey);
        
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_stale_detection() {
        let entry = L1CacheEntry::new(
            Arc::new(vec![1, 2, 3]),
            AccountEncoding::Base64,
            100,
            1,
        );
        
        assert!(entry.is_stale(101, 1)); // newer slot
        assert!(entry.is_stale(100, 2)); // newer write_version
        assert!(!entry.is_stale(100, 1)); // same
        assert!(!entry.is_stale(99, 1)); // older slot
    }
}
