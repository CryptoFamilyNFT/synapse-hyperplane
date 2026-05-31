//! Program Account Index
//!
//! Bitmap index mapping program ID -> set of account pubkeys.
//! Uses RoaringTreemap for compression and fast intersections.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use solana_sdk::pubkey::Pubkey;
use hyperplane_types::PubkeyBitmap;

/// Program index entry
#[derive(Debug, Clone)]
pub struct ProgramIndexEntry {
    /// Program ID
    pub program_id: Pubkey,
    /// Bitmap of account pubkey_ids owned by this program
    pub accounts: PubkeyBitmap,
    /// Last updated slot
    pub last_updated_slot: u64,
}

/// Program index state
#[derive(Debug, Default)]
pub struct ProgramIndexState {
    /// Map of program_id -> bitmap of account pubkey_ids
    program_bitmaps: BTreeMap<Pubkey, PubkeyBitmap>,
    /// Reverse map: account pubkey -> program_id
    account_to_program: BTreeMap<Pubkey, Pubkey>,
    /// Pubkey dictionary for compression (pubkey -> pubkey_id)
    pubkey_dict: BTreeMap<Pubkey, u64>,
    /// Next pubkey_id
    next_pubkey_id: u64,
    /// Total indexed accounts
    total_accounts: u64,
}

/// Program Account Index
pub struct ProgramIndex {
    state: Arc<RwLock<ProgramIndexState>>,
    #[allow(dead_code)]
    index_path: PathBuf,
}

impl ProgramIndex {
    /// Create a new program index
    pub fn new(index_path: PathBuf) -> Self {
        Self {
            state: Arc::new(RwLock::new(ProgramIndexState::default())),
            index_path,
        }
    }
    
    /// Add an account to the index
    pub fn add_account(&self, account_pubkey: Pubkey, program_id: Pubkey, _slot: u64) {
        let mut state = self.state.write();
        
        // Get or assign pubkey_id for account
        let account_id = match state.pubkey_dict.get(&account_pubkey) {
            Some(&id) => id,
            None => {
                let id = state.next_pubkey_id;
                state.next_pubkey_id += 1;
                state.pubkey_dict.insert(account_pubkey, id);
                id
            }
        };
        
        // Use get_mut instead of entry to avoid borrow checker issues
        let bitmap = state.program_bitmaps.get_mut(&program_id);
        if let Some(bitmap) = bitmap {
            bitmap.insert(account_id);
        } else {
            let mut bitmap = PubkeyBitmap::new();
            bitmap.insert(account_id);
            state.program_bitmaps.insert(program_id, bitmap);
        }
        
        // Update reverse map
        state.account_to_program.insert(account_pubkey, program_id);
        state.total_accounts += 1;
    }
    
    /// Remove an account from the index
    pub fn remove_account(&self, account_pubkey: &Pubkey) {
        let mut state = self.state.write();
        
        if let Some(program_id) = state.account_to_program.remove(account_pubkey) {
            let account_id = state.pubkey_dict.get(account_pubkey).copied();
            if let Some(bitmap) = state.program_bitmaps.get_mut(&program_id) {
                if let Some(account_id) = account_id {
                    bitmap.remove(account_id);
                }
            }
            state.total_accounts = state.total_accounts.saturating_sub(1);
        }
    }
    
    /// Get all accounts for a program
    pub fn get_program_accounts(&self, program_id: &Pubkey) -> Option<Vec<Pubkey>> {
        let state = self.state.read();
        let bitmap = state.program_bitmaps.get(program_id)?;
        
        // Build reverse lookup: pubkey_id -> pubkey
        let id_to_pubkey: BTreeMap<u64, Pubkey> = state.pubkey_dict
            .iter()
            .map(|(k, v)| (*v, *k))
            .collect();
        
        // Convert pubkey_ids back to pubkeys
        Some(bitmap.iter().filter_map(|id| id_to_pubkey.get(&id).copied()).collect())
    }
    
    /// Returns all program IDs present in the index
    pub fn get_all_programs(&self) -> Vec<Pubkey> {
        let state = self.state.read();
        state.program_bitmaps.keys().copied().collect()
    }
    
    /// Get program ID for an account
    pub fn get_program_for_account(&self, account_pubkey: &Pubkey) -> Option<Pubkey> {
        let state = self.state.read();
        state.account_to_program.get(account_pubkey).copied()
    }
    
    /// Get statistics
    pub fn stats(&self) -> ProgramIndexStats {
        let state = self.state.read();
        ProgramIndexStats {
            total_programs: state.program_bitmaps.len() as u64,
            total_accounts: state.total_accounts,
        }
    }
}

/// Program index statistics
#[derive(Debug, Clone, Default)]
pub struct ProgramIndexStats {
    pub total_programs: u64,
    pub total_accounts: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_program_index_basic() {
        let index = ProgramIndex::new(PathBuf::from("/tmp/test_program_index"));
        
        let program_id = Pubkey::new_unique();
        let account1 = Pubkey::new_unique();
        let account2 = Pubkey::new_unique();
        
        index.add_account(account1, program_id, 100);
        index.add_account(account2, program_id, 101);
        
        let accounts = index.get_program_accounts(&program_id).unwrap();
        assert_eq!(accounts.len(), 2);
        
        assert_eq!(index.get_program_for_account(&account1), Some(program_id));
        
        let stats = index.stats();
        assert_eq!(stats.total_programs, 1);
        assert_eq!(stats.total_accounts, 2);
    }
}
