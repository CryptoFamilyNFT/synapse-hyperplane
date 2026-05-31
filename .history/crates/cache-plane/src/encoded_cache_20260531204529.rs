//! Encoded Response Cache
//!
//! Caches fully encoded RPC responses to avoid re-encoding on repeated requests.
//! Key format: "{method}:{pubkey_or_program}:{filters_hash}:{encoding}:{commitment}"

use crate::l1::{CacheKey, L1CacheEntry, L1HotCache, L1CacheConfig};
use hyperplane_types::{AccountEncoding, AccountView, CommitmentLevel};
use hyperplane_types::rpc::account_view_to_rpc;
use sha2::{Digest, Sha256};
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;

/// Encoded response cache entry
#[derive(Debug, Clone)]
pub struct EncodedResponseEntry {
    /// Encoded response bytes (JSON)
    pub response: Arc<Vec<u8>>,
    /// Content-Type header
    pub content_type: &'static str,
    /// Slot when cached
    pub cached_slot: u64,
    /// Cache insertion time
    pub cached_at: std::time::Instant,
}

/// Response cache configuration
#[derive(Debug, Clone)]
pub struct ResponseCacheConfig {
    /// Max entries
    pub max_entries: usize,
    /// TTL (ms)
    pub ttl_ms: u64,
}

impl Default for ResponseCacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 500_000,
            ttl_ms: 60_000, // 1 minute
        }
    }
}

/// Encoded response cache
#[allow(dead_code)]
pub struct EncodedResponseCache {
    cache: L1HotCache,
    config: ResponseCacheConfig,
}

impl EncodedResponseCache {
    pub fn new(config: ResponseCacheConfig) -> Self {
        let l1_config = L1CacheConfig {
            max_entries: config.max_entries,
            processed_ttl_ms: config.ttl_ms,
            confirmed_ttl_ms: config.ttl_ms * 5,
            finalized_ttl_ms: config.ttl_ms * 60,
            eviction_batch_size: 500,
        };
        
        Self {
            cache: L1HotCache::new(l1_config),
            config,
        }
    }

    /// Generate cache key for getAccountInfo response
    pub fn get_account_info_key(
        pubkey: Pubkey,
        encoding: AccountEncoding,
        commitment: CommitmentLevel,
    ) -> CacheKey {
        CacheKey::new(pubkey, encoding, commitment)
    }

    /// Generate cache key for getProgramAccounts response
    pub fn get_program_accounts_key(
        program_id: Pubkey,
        filters_hash: &[u8],
        encoding: AccountEncoding,
        commitment: CommitmentLevel,
        limit: usize,
        cursor: Option<&str>,
    ) -> CacheKey {
        // Create composite key from all parameters
        let mut hasher = Sha256::new();
        hasher.update(&program_id.to_bytes());
        hasher.update(filters_hash);
        hasher.update(&[encoding as u8]);
        hasher.update(&[commitment as u8]);
        hasher.update(&limit.to_le_bytes());
        if let Some(c) = cursor {
            hasher.update(c.as_bytes());
        }
        
        let hash = hasher.finalize();
        // Generate deterministic pseudo-pubkey from hash
        let pseudo_pubkey = Pubkey::try_from(&hash[..32]).unwrap_or_default();
        
        CacheKey::new(pseudo_pubkey, encoding, commitment)
    }

    /// Cache encoded getAccountInfo response
    pub fn cache_account_info(
        &self,
        pubkey: Pubkey,
        account: &AccountView,
        encoding: AccountEncoding,
        commitment: CommitmentLevel,
        response_json: Vec<u8>,
    ) {
        let key = Self::get_account_info_key(pubkey, encoding, commitment);
        let entry = L1CacheEntry::new(
            Arc::new(response_json),
            encoding,
            account.slot,
            account.write_version,
        );
        
        self.cache.insert(key, entry);
    }

    /// Get cached getAccountInfo response
    pub fn get_account_info(
        &self,
        pubkey: Pubkey,
        encoding: AccountEncoding,
        commitment: CommitmentLevel,
    ) -> Option<Arc<Vec<u8>>> {
        let key = Self::get_account_info_key(pubkey, encoding, commitment);
        self.cache.get(&key).map(|entry| entry.data)
    }

    /// Encode account view to JSON
    pub fn encode_account_info(
        account: &AccountView,
        encoding: AccountEncoding,
        slot: u64,
    ) -> Vec<u8> {
        let rpc_account = account_view_to_rpc(account, encoding);
        
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "result": {
                "context": {
                    "slot": slot,
                    "apiVersion": "synapse-hyperplane/0.1.0"
                },
                "value": rpc_account
            },
            "id": 1
        });
        
        serde_json::to_vec(&response).unwrap_or_default()
    }

    /// Invalidate cached responses for pubkey
    pub fn invalidate(&self, pubkey: Pubkey) {
        self.cache.invalidate(pubkey);
    }

    /// Invalidate batch
    pub fn invalidate_batch(&self, pubkeys: &[Pubkey]) {
        self.cache.invalidate_batch(pubkeys);
    }

    /// Get cache size
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Get hit rate
    pub fn hit_rate(&self) -> f64 {
        self.cache.hit_rate()
    }

    /// Get stats
    pub fn stats(&self) -> &crate::l1::L1CacheStats {
        self.cache.stats()
    }
}

/// Compute filters hash for getProgramAccounts cache key
pub fn compute_filters_hash(filters: &[hyperplane_types::AccountFilter]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    
    for filter in filters {
        if let Some(size) = filter.data_size {
            hasher.update(b"data_size");
            hasher.update(&size.to_le_bytes());
        }
        
        if let Some(memcmp) = &filter.memcmp {
            hasher.update(b"memcmp");
            hasher.update(&memcmp.offset.to_le_bytes());
            hasher.update(memcmp.bytes.as_bytes());
        }
        
        if let Some(mint) = &filter.mint {
            hasher.update(b"mint");
            hasher.update(&mint.to_bytes());
        }
        
        if let Some(owner) = &filter.token_owner {
            hasher.update(b"token_owner");
            hasher.update(&owner.to_bytes());
        }
    }
    
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoded_cache() {
        let cache = EncodedResponseCache::new(ResponseCacheConfig::default());
        
        let pubkey = Pubkey::new_unique();
        let account = AccountView {
            pubkey,
            lamports: 1000,
            data: Arc::new(vec![1, 2, 3, 4]),
            owner: Pubkey::default(),
            executable: false,
            slot: 100,
            write_version: 1,
            rent_epoch: 0,
            storage_type: hyperplane_types::StorageType::Base,
            location: hyperplane_types::AccountLocation::new_base(0, 0, 0, 0, 4, 100, 1),
        };
        
        // Encode and cache
        let encoded = EncodedResponseCache::encode_account_info(
            &account,
            AccountEncoding::Base64,
            100,
        );
        
        cache.cache_account_info(
            pubkey,
            &account,
            AccountEncoding::Base64,
            CommitmentLevel::Processed,
            encoded.clone(),
        );
        
        // Retrieve
        let retrieved = cache.get_account_info(pubkey, AccountEncoding::Base64, CommitmentLevel::Processed);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().as_slice(), encoded.as_slice());
    }

    #[test]
    fn test_filters_hash() {
        let filters1 = vec![
            hyperplane_types::AccountFilter {
                data_size: Some(165),
                memcmp: None,
                mint: None,
                token_owner: None,
            }
        ];
        
        let filters2 = vec![
            hyperplane_types::AccountFilter {
                data_size: Some(165),
                memcmp: None,
                mint: None,
                token_owner: None,
            }
        ];
        
        let hash1 = compute_filters_hash(&filters1);
        let hash2 = compute_filters_hash(&filters2);
        
        assert_eq!(hash1, hash2);
    }
}
