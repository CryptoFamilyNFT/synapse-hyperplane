//! Token Owner Index
//!
//! Bitmap index mapping owner pubkey -> set of token account pubkeys.
//! Optimized for getTokenAccountsByOwner queries.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use solana_sdk::pubkey::Pubkey;
use hyperplane_types::PubkeyBitmap;

/// Token owner index state
#[derive(Debug, Default)]
pub struct TokenOwnerIndexState {
    /// Map of owner -> token account bitmap (by pubkey_id)
    owner_bitmaps: BTreeMap<Pubkey, PubkeyBitmap>,
    /// Reverse map: token account -> owner
    account_to_owner: BTreeMap<Pubkey, Pubkey>,
    /// Pubkey dictionary
    pubkey_dict: BTreeMap<Pubkey, u64>,
    next_pubkey_id: u64,
    /// Total indexed token accounts
    total_accounts: u64,
}

/// Token Owner Index
pub struct TokenOwnerIndex {
    state: Arc<RwLock<TokenOwnerIndexState>>,
    index_path: PathBuf,
}

impl TokenOwnerIndex {
    pub fn new(index_path: PathBuf) -> Self {
        Self {
            state: Arc::new(RwLock::new(TokenOwnerIndexState::default())),
            index_path,
        }
    }
    
    pub fn add_token_account(&self, token_account: Pubkey, owner: Pubkey, _slot: u64) {
        let mut state = self.state.write();
        
        let token_id = match state.pubkey_dict.get(&token_account) {
            Some(&id) => id,
            None => {
                let id = state.next_pubkey_id;
                state.next_pubkey_id += 1;
                state.pubkey_dict.insert(token_account, id);
                id
            }
        };
        
        // Use get_mut instead of entry to avoid borrow checker issues
        let bitmap = state.owner_bitmaps.get_mut(&owner);
        if let Some(bitmap) = bitmap {
            bitmap.insert(token_id);
        } else {
            let mut bitmap = PubkeyBitmap::new();
            bitmap.insert(token_id);
            state.owner_bitmaps.insert(owner, bitmap);
        }
        
        state.account_to_owner.insert(token_account, owner);
        state.total_accounts += 1;
    }
    
    pub fn remove_token_account(&self, token_account: &Pubkey) {
        let mut state = self.state.write();
        
        if let Some(owner) = state.account_to_owner.remove(token_account) {
            let token_id = state.pubkey_dict.get(token_account).copied();
            if let Some(bitmap) = state.owner_bitmaps.get_mut(&owner) {
                if let Some(token_id) = token_id {
                    bitmap.remove(token_id);
                }
            }
            state.total_accounts = state.total_accounts.saturating_sub(1);
        }
    }
    
    pub fn get_token_accounts_by_owner(&self, owner: &Pubkey) -> Option<Vec<Pubkey>> {
        let state = self.state.read();
        let bitmap = state.owner_bitmaps.get(owner)?;
        
        let id_to_pubkey: BTreeMap<u64, Pubkey> = state.pubkey_dict
            .iter()
            .map(|(k, v)| (*v, *k))
            .collect();
        
        Some(bitmap.iter().filter_map(|id| id_to_pubkey.get(&id).copied()).collect())
    }
    
    pub fn get_owner_for_token_account(&self, token_account: &Pubkey) -> Option<Pubkey> {
        let state = self.state.read();
        state.account_to_owner.get(token_account).copied()
    }
    
    pub fn stats(&self) -> TokenOwnerIndexStats {
        let state = self.state.read();
        TokenOwnerIndexStats {
            total_owners: state.owner_bitmaps.len() as u64,
            total_accounts: state.total_accounts,
        }
    }
}

/// Token owner index statistics
#[derive(Debug, Clone, Default)]
pub struct TokenOwnerIndexStats {
    pub total_owners: u64,
    pub total_accounts: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_token_owner_index_basic() {
        let index = TokenOwnerIndex::new(PathBuf::from("/tmp/test_token_owner_index"));
        
        let owner = Pubkey::new_unique();
        let token1 = Pubkey::new_unique();
        let token2 = Pubkey::new_unique();
        
        index.add_token_account(token1, owner, 100);
        index.add_token_account(token2, owner, 101);
        
        let tokens = index.get_token_accounts_by_owner(&owner).unwrap();
        assert_eq!(tokens.len(), 2);
        
        assert_eq!(index.get_owner_for_token_account(&token1), Some(owner));
        
        let stats = index.stats();
        assert_eq!(stats.total_owners, 1);
        assert_eq!(stats.total_accounts, 2);
    }
}
