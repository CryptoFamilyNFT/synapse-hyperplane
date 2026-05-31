//! Discriminator Index
//!
//! Index for Anchor program discriminators (first 8 bytes of account data).
//! Optimized for filtering accounts by type in getProgramAccounts.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use solana_sdk::pubkey::Pubkey;
use hyperplane_types::PubkeyBitmap;

/// Discriminator (8 bytes for Anchor types)
pub type Discriminator = [u8; 8];

/// Discriminator index state
#[derive(Debug, Default)]
pub struct DiscriminatorIndexState {
    /// Map of discriminator -> bitmap of account pubkey_ids
    discriminator_bitmaps: BTreeMap<Discriminator, PubkeyBitmap>,
    /// Reverse map: account pubkey -> discriminator
    account_to_discriminator: BTreeMap<Pubkey, Discriminator>,
    /// Pubkey dictionary
    pubkey_dict: BTreeMap<Pubkey, u64>,
    next_pubkey_id: u64,
    /// Total indexed accounts
    total_accounts: u64,
}

/// Discriminator Index
pub struct DiscriminatorIndex {
    state: Arc<RwLock<DiscriminatorIndexState>>,
    index_path: PathBuf,
}

impl DiscriminatorIndex {
    pub fn new(index_path: PathBuf) -> Self {
        Self {
            state: Arc::new(RwLock::new(DiscriminatorIndexState::default())),
            index_path,
        }
    }
    
    pub fn add_account(&self, account: Pubkey, discriminator: Discriminator, _slot: u64) {
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
        let bitmap = state.discriminator_bitmaps.get_mut(&discriminator);
        if let Some(bitmap) = bitmap {
            bitmap.insert(account_id);
        } else {
            let mut bitmap = PubkeyBitmap::new();
            bitmap.insert(account_id);
            state.discriminator_bitmaps.insert(discriminator, bitmap);
        }
        
        state.account_to_discriminator.insert(account, discriminator);
        state.total_accounts += 1;
    }
    
    pub fn remove_account(&self, account: &Pubkey) {
        let mut state = self.state.write();
        
        if let Some(discriminator) = state.account_to_discriminator.remove(account) {
            let account_id = state.pubkey_dict.get(account).copied();
            if let Some(bitmap) = state.discriminator_bitmaps.get_mut(&discriminator) {
                if let Some(account_id) = account_id {
                    bitmap.remove(account_id);
                }
            }
            state.total_accounts = state.total_accounts.saturating_sub(1);
        }
    }
    
    pub fn get_accounts_by_discriminator(&self, discriminator: Discriminator) -> Option<Vec<Pubkey>> {
        let state = self.state.read();
        let bitmap = state.discriminator_bitmaps.get(&discriminator)?;
        
        let id_to_pubkey: BTreeMap<u64, Pubkey> = state.pubkey_dict
            .iter()
            .map(|(k, v)| (*v, *k))
            .collect();
        
        Some(bitmap.iter().filter_map(|id| id_to_pubkey.get(&id).copied()).collect())
    }
    
    pub fn get_discriminator_for_account(&self, account: &Pubkey) -> Option<Discriminator> {
        let state = self.state.read();
        state.account_to_discriminator.get(account).copied()
    }
    
    /// Returns all discriminators present in the index
    pub fn get_all_discriminators(&self) -> Vec<Discriminator> {
        let state = self.state.read();
        state.discriminator_bitmaps.keys().copied().collect()
    }
    
    pub fn stats(&self) -> DiscriminatorIndexStats {
        let state = self.state.read();
        DiscriminatorIndexStats {
            unique_discriminators: state.discriminator_bitmaps.len() as u64,
            total_accounts: state.total_accounts,
        }
    }
}

/// Discriminator index statistics
#[derive(Debug, Clone, Default)]
pub struct DiscriminatorIndexStats {
    pub unique_discriminators: u64,
    pub total_accounts: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_discriminator_index_basic() {
        let index = DiscriminatorIndex::new(PathBuf::from("/tmp/test_discriminator_index"));
        
        let account1 = Pubkey::new_unique();
        let account2 = Pubkey::new_unique();
        let discriminator = [1, 2, 3, 4, 5, 6, 7, 8];
        
        index.add_account(account1, discriminator, 100);
        index.add_account(account2, discriminator, 101);
        
        let accounts = index.get_accounts_by_discriminator(discriminator).unwrap();
        assert_eq!(accounts.len(), 2);
        
        let stats = index.stats();
        assert_eq!(stats.unique_discriminators, 1);
    }
}
