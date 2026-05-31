//! Data Size Index
//!
//! Index mapping account data size -> set of account pubkeys.
//! Optimized for filtering accounts by data size in getProgramAccounts.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use solana_sdk::pubkey::Pubkey;
use hyperplane_types::PubkeyBitmap;

/// Data size index state
#[derive(Debug, Default)]
pub struct DataSizeIndexState {
    /// Map of data_size -> bitmap of account pubkey_ids
    size_bitmaps: BTreeMap<u64, PubkeyBitmap>,
    /// Reverse map: account pubkey -> data size
    account_to_size: BTreeMap<Pubkey, u64>,
    /// Pubkey dictionary
    pubkey_dict: BTreeMap<Pubkey, u64>,
    next_pubkey_id: u64,
    /// Total indexed accounts
    total_accounts: u64,
}

/// Data Size Index
pub struct DataSizeIndex {
    state: Arc<RwLock<DataSizeIndexState>>,
    index_path: PathBuf,
}

impl DataSizeIndex {
    pub fn new(index_path: PathBuf) -> Self {
        Self {
            state: Arc::new(RwLock::new(DataSizeIndexState::default())),
            index_path,
        }
    }
    
    pub fn add_account(&self, account: Pubkey, data_size: u64, _slot: u64) {
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
        
        // Use get_mut instead of entry to avoid borrow checker issues
        let bitmap = state.size_bitmaps.get_mut(&data_size);
        if let Some(bitmap) = bitmap {
            bitmap.insert(account_id);
        } else {
            let mut bitmap = PubkeyBitmap::new();
            bitmap.insert(account_id);
            state.size_bitmaps.insert(data_size, bitmap);
        }
        
        state.account_to_size.insert(account, data_size);
        state.total_accounts += 1;
    }
    
    pub fn remove_account(&self, account: &Pubkey) {
        let mut state = self.state.write();
        
        if let Some(data_size) = state.account_to_size.remove(account) {
            let account_id = state.pubkey_dict.get(account).copied();
            if let Some(bitmap) = state.size_bitmaps.get_mut(&data_size) {
                if let Some(account_id) = account_id {
                    bitmap.remove(account_id);
                }
            }
            state.total_accounts = state.total_accounts.saturating_sub(1);
        }
    }
    
    pub fn get_accounts_by_size(&self, data_size: u64) -> Option<Vec<Pubkey>> {
        let state = self.state.read();
        let bitmap = state.size_bitmaps.get(&data_size)?;
        
        let id_to_pubkey: BTreeMap<u64, Pubkey> = state.pubkey_dict
            .iter()
            .map(|(k, v)| (*v, *k))
            .collect();
        
        Some(bitmap.iter().filter_map(|id| id_to_pubkey.get(&id).copied()).collect())
    }
    
    pub fn get_size_for_account(&self, account: &Pubkey) -> Option<u64> {
        let state = self.state.read();
        state.account_to_size.get(account).copied()
    }
    
    /// Returns all sizes present in the index
    pub fn get_all_sizes(&self) -> Vec<u64> {
        let state = self.state.read();
        state.size_bitmaps.keys().copied().collect()
    }
    
    pub fn stats(&self) -> DataSizeIndexStats {
        let state = self.state.read();
        DataSizeIndexStats {
            unique_sizes: state.size_bitmaps.len() as u64,
            total_accounts: state.total_accounts,
        }
    }
}

/// Data size index statistics
#[derive(Debug, Clone, Default)]
pub struct DataSizeIndexStats {
    pub unique_sizes: u64,
    pub total_accounts: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_data_size_index_basic() {
        let index = DataSizeIndex::new(PathBuf::from("/tmp/test_data_size_index"));
        
        let account1 = Pubkey::new_unique();
        let account2 = Pubkey::new_unique();
        
        index.add_account(account1, 100, 100);
        index.add_account(account2, 100, 101);
        
        let accounts = index.get_accounts_by_size(100).unwrap();
        assert_eq!(accounts.len(), 2);
        
        assert_eq!(index.get_size_for_account(&account1), Some(100));
        
        let stats = index.stats();
        assert_eq!(stats.unique_sizes, 1);
        assert_eq!(stats.total_accounts, 2);
    }
}
