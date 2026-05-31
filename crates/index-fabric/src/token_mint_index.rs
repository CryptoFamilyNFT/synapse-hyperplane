//! Token Mint Index
//!
//! Bitmap index mapping mint pubkey -> set of token account pubkeys.
//! Optimized for getTokenAccountsByDelegate and getTokenLargestAccounts queries.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use solana_sdk::pubkey::Pubkey;
use hyperplane_types::PubkeyBitmap;

/// Token mint index state
#[derive(Debug, Default)]
pub struct TokenMintIndexState {
    /// Map of mint -> token account bitmap (by pubkey_id)
    mint_bitmaps: BTreeMap<Pubkey, PubkeyBitmap>,
    /// Reverse map: token account -> mint
    account_to_mint: BTreeMap<Pubkey, Pubkey>,
    /// Pubkey dictionary
    pubkey_dict: BTreeMap<Pubkey, u64>,
    next_pubkey_id: u64,
    /// Total indexed token accounts
    total_accounts: u64,
}

/// Token Mint Index
pub struct TokenMintIndex {
    state: Arc<RwLock<TokenMintIndexState>>,
    index_path: PathBuf,
}

impl TokenMintIndex {
    pub fn new(index_path: PathBuf) -> Self {
        Self {
            state: Arc::new(RwLock::new(TokenMintIndexState::default())),
            index_path,
        }
    }
    
    pub fn add_token_account(&self, token_account: Pubkey, mint: Pubkey, _slot: u64) {
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
        let bitmap = state.mint_bitmaps.get_mut(&mint);
        if let Some(bitmap) = bitmap {
            bitmap.insert(token_id);
        } else {
            let mut bitmap = PubkeyBitmap::new();
            bitmap.insert(token_id);
            state.mint_bitmaps.insert(mint, bitmap);
        }
        
        state.account_to_mint.insert(token_account, mint);
        state.total_accounts += 1;
    }
    
    pub fn remove_token_account(&self, token_account: &Pubkey) {
        let mut state = self.state.write();
        
        if let Some(mint) = state.account_to_mint.remove(token_account) {
            let token_id = state.pubkey_dict.get(token_account).copied();
            if let Some(bitmap) = state.mint_bitmaps.get_mut(&mint) {
                if let Some(token_id) = token_id {
                    bitmap.remove(token_id);
                }
            }
            state.total_accounts = state.total_accounts.saturating_sub(1);
        }
    }
    
    pub fn get_token_accounts_by_mint(&self, mint: &Pubkey) -> Option<Vec<Pubkey>> {
        let state = self.state.read();
        let bitmap = state.mint_bitmaps.get(mint)?;
        
        let id_to_pubkey: BTreeMap<u64, Pubkey> = state.pubkey_dict
            .iter()
            .map(|(k, v)| (*v, *k))
            .collect();
        
        Some(bitmap.iter().filter_map(|id| id_to_pubkey.get(&id).copied()).collect())
    }
    
    pub fn get_mint_for_token_account(&self, token_account: &Pubkey) -> Option<Pubkey> {
        let state = self.state.read();
        state.account_to_mint.get(token_account).copied()
    }
    
    pub fn stats(&self) -> TokenMintIndexStats {
        let state = self.state.read();
        TokenMintIndexStats {
            total_mints: state.mint_bitmaps.len() as u64,
            total_accounts: state.total_accounts,
        }
    }
}

/// Token mint index statistics
#[derive(Debug, Clone, Default)]
pub struct TokenMintIndexStats {
    pub total_mints: u64,
    pub total_accounts: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_token_mint_index_basic() {
        let index = TokenMintIndex::new(PathBuf::from("/tmp/test_token_mint_index"));
        
        let mint = Pubkey::new_unique();
        let token1 = Pubkey::new_unique();
        let token2 = Pubkey::new_unique();
        
        index.add_token_account(token1, mint, 100);
        index.add_token_account(token2, mint, 101);
        
        let tokens = index.get_token_accounts_by_mint(&mint).unwrap();
        assert_eq!(tokens.len(), 2);
        
        assert_eq!(index.get_mint_for_token_account(&token1), Some(mint));
        
        let stats = index.stats();
        assert_eq!(stats.total_mints, 1);
        assert_eq!(stats.total_accounts, 2);
    }
}
