//! Memcmp Index
//!
//! Index for filtering accounts by byte sequence at specific offset.
//! Optimized for getProgramAccounts memcmp filters.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use solana_sdk::pubkey::Pubkey;
use hyperplane_types::PubkeyBitmap;

/// Memcmp filter definition
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct MemcmpFilter {
    /// Byte offset in account data
    pub offset: usize,
    /// Bytes to match
    pub bytes: Vec<u8>,
}

/// Memcmp index state
#[derive(Debug, Default)]
pub struct MemcmpIndexState {
    /// Map of (offset, bytes) -> bitmap of account pubkey_ids
    memcmp_bitmaps: BTreeMap<MemcmpFilter, PubkeyBitmap>,
    /// Reverse map: account pubkey -> set of (offset, bytes)
    account_memcmps: BTreeMap<Pubkey, Vec<MemcmpFilter>>,
    /// Pubkey dictionary
    pubkey_dict: BTreeMap<Pubkey, u64>,
    next_pubkey_id: u64,
    /// Total indexed accounts
    total_accounts: u64,
}

/// Memcmp Index
pub struct MemcmpIndex {
    state: Arc<RwLock<MemcmpIndexState>>,
    #[allow(dead_code)]
    index_path: PathBuf,
}

impl MemcmpIndex {
    pub fn new(index_path: PathBuf) -> Self {
        Self {
            state: Arc::new(RwLock::new(MemcmpIndexState::default())),
            index_path,
        }
    }
    
    pub fn add_account_memcmp(&self, account: Pubkey, offset: usize, bytes: &[u8], _slot: u64) {
        let mut state = self.state.write();
        
        let account_id = match state.pubkey_dict.get(&account) {
            Some(&id) => id,
            None => {
                let id = state.next_pubkey_id;
                state.next_pubkey_id += 1;
                state.pubkey_dict.insert(account, id);
                id
            }
        };
        
        let filter = MemcmpFilter {
            offset,
            bytes: bytes.to_vec(),
        };
        
        // Use get_mut instead of entry to avoid borrow checker issues
        let bitmap = state.memcmp_bitmaps.get_mut(&filter);
        if let Some(bitmap) = bitmap {
            bitmap.insert(account_id);
        } else {
            let mut bitmap = PubkeyBitmap::new();
            bitmap.insert(account_id);
            state.memcmp_bitmaps.insert(filter.clone(), bitmap);
        }
        
        state.account_memcmps.entry(account).or_default().push(filter);
        state.total_accounts += 1;
    }
    
    pub fn get_accounts_by_memcmp(&self, offset: usize, bytes: &[u8]) -> Option<Vec<Pubkey>> {
        let state = self.state.read();
        let filter = MemcmpFilter {
            offset,
            bytes: bytes.to_vec(),
        };
        let bitmap = state.memcmp_bitmaps.get(&filter)?;
        
        let id_to_pubkey: BTreeMap<u64, Pubkey> = state.pubkey_dict
            .iter()
            .map(|(k, v)| (*v, *k))
            .collect();
        
        Some(bitmap.iter().filter_map(|id| id_to_pubkey.get(&id).copied()).collect())
    }
    
    /// Returns all memcmp keys present in the index
    pub fn get_all_memcmp_keys(&self) -> Vec<(usize, Vec<u8>)> {
        let state = self.state.read();
        state.memcmp_bitmaps
            .keys()
            .map(|f| (f.offset, f.bytes.clone()))
            .collect()
    }
    
    pub fn stats(&self) -> MemcmpIndexStats {
        let state = self.state.read();
        MemcmpIndexStats {
            unique_memcmps: state.memcmp_bitmaps.len() as u64,
            total_accounts: state.total_accounts,
        }
    }
}

/// Memcmp index statistics
#[derive(Debug, Clone, Default)]
pub struct MemcmpIndexStats {
    pub unique_memcmps: u64,
    pub total_accounts: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memcmp_index_basic() {
        let index = MemcmpIndex::new(PathBuf::from("/tmp/test_memcmp_index"));
        
        let account1 = Pubkey::new_unique();
        let account2 = Pubkey::new_unique();
        
        index.add_account_memcmp(account1, 0, &[1, 2, 3], 100);
        index.add_account_memcmp(account2, 0, &[1, 2, 3], 101);
        
        let accounts = index.get_accounts_by_memcmp(0, &[1, 2, 3]).unwrap();
        assert_eq!(accounts.len(), 2);
        
        let stats = index.stats();
        assert_eq!(stats.unique_memcmps, 1);
    }
}
