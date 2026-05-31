//! Index Manager - Real-time index updates from Geyser stream
//!
//! Bridges Delta Plane updates to Index Fabric:
//! - Listens to account updates from Geyser
//! - Updates program/token/data_size/memcmp/discriminator indexes
//! - Maintains index consistency with delta segments

use std::sync::{Arc, RwLock};
use std::path::PathBuf;
use solana_sdk::pubkey::Pubkey;

use index_fabric::{
    ProgramIndex,
    TokenOwnerIndex,
    TokenMintIndex,
    DataSizeIndex,
    MemcmpIndex,
    DiscriminatorIndex,
};
use hyperplane_types::AccountView;

/// Index Manager configuration
#[derive(Debug, Clone)]
pub struct IndexManagerConfig {
    /// Path to index storage directory
    pub index_path: PathBuf,
    /// Enable program index
    pub enable_program_index: bool,
    /// Enable token owner index
    pub enable_token_owner: bool,
    /// Enable token mint index
    pub enable_token_mint: bool,
    /// Enable data size index
    pub enable_data_size: bool,
    /// Enable memcmp index (adaptive)
    pub enable_memcmp: bool,
    /// Enable discriminator index (Anchor)
    pub enable_discriminator: bool,
}

impl Default for IndexManagerConfig {
    fn default() -> Self {
        Self {
            index_path: PathBuf::from("/tmp/synapse/indexes"),
            enable_program_index: true,
            enable_token_owner: true,
            enable_token_mint: true,
            enable_data_size: true,
            enable_memcmp: true,
            enable_discriminator: true,
        }
    }
}

/// Index Manager state
#[derive(Debug, Clone)]
pub struct IndexManagerState {
    /// Total accounts indexed
    pub total_indexed: u64,
    /// Total updates processed
    pub total_updates: u64,
}

/// Index Manager - Real-time index updater
pub struct IndexManager {
    config: IndexManagerConfig,
    program_index: Option<Arc<ProgramIndex>>,
    token_owner_index: Option<Arc<TokenOwnerIndex>>,
    token_mint_index: Option<Arc<TokenMintIndex>>,
    data_size_index: Option<Arc<DataSizeIndex>>,
    memcmp_index: Option<Arc<MemcmpIndex>>,
    discriminator_index: Option<Arc<DiscriminatorIndex>>,
    state: Arc<RwLock<IndexManagerState>>,
}

impl IndexManager {
    /// Create a new Index Manager
    pub fn new(config: IndexManagerConfig) -> Self {
        let state = Arc::new(RwLock::new(IndexManagerState {
            total_indexed: 0,
            total_updates: 0,
        }));

        let program_index = if config.enable_program_index {
            Some(Arc::new(ProgramIndex::new(config.index_path.join("program_index"))))
        } else {
            None
        };

        let token_owner_index = if config.enable_token_owner {
            Some(Arc::new(TokenOwnerIndex::new(config.index_path.join("token_owner_index"))))
        } else {
            None
        };

        let token_mint_index = if config.enable_token_mint {
            Some(Arc::new(TokenMintIndex::new(config.index_path.join("token_mint_index"))))
        } else {
            None
        };

        let data_size_index = if config.enable_data_size {
            Some(Arc::new(DataSizeIndex::new(config.index_path.join("data_size_index"))))
        } else {
            None
        };

        let memcmp_index = if config.enable_memcmp {
            Some(Arc::new(MemcmpIndex::new(config.index_path.join("memcmp_index"))))
        } else {
            None
        };

        let discriminator_index = if config.enable_discriminator {
            Some(Arc::new(DiscriminatorIndex::new(config.index_path.join("discriminator_index"))))
        } else {
            None
        };

        Self {
            config,
            program_index,
            token_owner_index,
            token_mint_index,
            data_size_index,
            memcmp_index,
            discriminator_index,
            state,
        }
    }

    /// Process an account update and update all indexes
    pub fn update_account(&self, account: &AccountView, slot: u64) {
        {
            let mut state = self.state.write().unwrap();
            state.total_updates += 1;
        }

        let pubkey = account.pubkey;
        let owner = account.owner;
        let data = &account.data;

        // Update program index (owner = program_id)
        if let Some(ref idx) = self.program_index {
            idx.add_account(pubkey, owner, slot);
        }

        // Check if this is a Token Account (check for Token Program ownership)
        // Token Program ID: TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
        const TOKEN_PROGRAM_ID: Pubkey = Pubkey::new_from_array([
            6, 167, 213, 23, 166, 155, 76, 121, 104, 160, 73, 198, 153, 124, 57, 121,
            87, 101, 152, 199, 246, 121, 126, 178, 126, 126, 39, 39, 34, 118, 153, 81,
        ]);

        if owner == TOKEN_PROGRAM_ID && data.len() >= 72 {
            // Token Account structure:
            // 0-31: mint (Pubkey)
            // 32-63: owner (Pubkey)
            // 64-71: amount (u64)
            
            let mint = Pubkey::new_from_array(data[0..32].try_into().unwrap_or([0u8; 32]));
            let token_owner = Pubkey::new_from_array(data[32..64].try_into().unwrap_or([0u8; 32]));

            // Update token owner index
            if let Some(ref idx) = self.token_owner_index {
                idx.add_token_account(pubkey, token_owner, slot);
            }

            // Update token mint index
            if let Some(ref idx) = self.token_mint_index {
                idx.add_token_account(pubkey, mint, slot);
            }
        }

        // Update data size index
        if let Some(ref idx) = self.data_size_index {
            idx.add_account(pubkey, data.len() as u64, slot);
        }

        // Update memcmp index (adaptive - track common offsets)
        // For now, track first 8 bytes (discriminator) and first 32 bytes (common pubkeys)
        if let Some(ref idx) = self.memcmp_index {
            if data.len() >= 8 {
                idx.add_account_memcmp(pubkey, 0, &data[0..8], slot);
            }
            if data.len() >= 32 {
                idx.add_account_memcmp(pubkey, 0, &data[0..32], slot);
            }
        }

        // Update discriminator index (first 8 bytes for Anchor programs)
        if let Some(ref idx) = self.discriminator_index {
            if data.len() >= 8 {
                let mut disc = [0u8; 8];
                disc.copy_from_slice(&data[0..8]);
                idx.add_account(pubkey, disc, slot);
            }
        }

        {
            let mut state = self.state.write().unwrap();
            state.total_indexed += 1;
        }
    }

    /// Remove an account from all indexes
    pub fn remove_account(&self, pubkey: &Pubkey) {
        if let Some(ref idx) = self.program_index {
            idx.remove_account(pubkey);
        }
        if let Some(ref idx) = self.token_owner_index {
            idx.remove_token_account(pubkey);
        }
        if let Some(ref idx) = self.token_mint_index {
            idx.remove_token_account(pubkey);
        }
        if let Some(ref idx) = self.data_size_index {
            idx.remove_account(pubkey);
        }
        // Note: memcmp and discriminator indexes don't have remove_account yet
    }

    /// Get index manager statistics
    pub fn stats(&self) -> IndexManagerState {
        let state = self.state.read().unwrap();
        IndexManagerState {
            total_indexed: state.total_indexed,
            total_updates: state.total_updates,
            
        }
    }

    /// Get program index reference
    pub fn program_index(&self) -> Option<Arc<ProgramIndex>> {
        self.program_index.clone()
    }

    /// Get data size index reference
    pub fn data_size_index(&self) -> Option<Arc<DataSizeIndex>> {
        self.data_size_index.clone()
    }

    /// Get memcmp index reference
    pub fn memcmp_index(&self) -> Option<Arc<MemcmpIndex>> {
        self.memcmp_index.clone()
    }

    /// Get discriminator index reference
    pub fn discriminator_index(&self) -> Option<Arc<DiscriminatorIndex>> {
        self.discriminator_index.clone()
    }
}
