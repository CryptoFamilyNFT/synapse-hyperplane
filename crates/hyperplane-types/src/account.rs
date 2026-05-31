//! Account data structures and view merging logic
//!
//! Provides unified account representation merging delta and base layers.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use solana_sdk::{account::Account as SolanaAccount, clock::Slot, pubkey::Pubkey};
use std::sync::Arc;

use crate::location::{AccountLocation, StorageType};

/// Custom serializer for Arc<Vec<u8>>
fn serialize_arc_vec<S>(data: &Arc<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_bytes(data.as_slice())
}

/// Custom deserializer for Arc<Vec<u8>>
fn deserialize_arc_vec<'de, D>(deserializer: D) -> Result<Arc<Vec<u8>>, D::Error>
where
    D: Deserializer<'de>,
{
    let bytes = Vec::<u8>::deserialize(deserializer)?;
    Ok(Arc::new(bytes))
}

/// Unified account view that merges delta and base layers
/// 
/// This is the primary data structure returned by read operations.
/// It contains both the account data and metadata needed for commitment checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountView {
    /// Account pubkey
    pub pubkey: Pubkey,
    
    /// Account lamports
    pub lamports: u64,
    
    /// Account data
    #[serde(serialize_with = "serialize_arc_vec", deserialize_with = "deserialize_arc_vec")]
    pub data: Arc<Vec<u8>>,
    
    /// Account owner program
    pub owner: Pubkey,
    
    /// Whether account is executable
    pub executable: bool,
    
    /// Slot of last modification
    pub slot: Slot,
    
    /// Write version for ordering within slot
    pub write_version: u64,
    
    /// Rent epoch (deprecated but kept for compatibility)
    pub rent_epoch: u64,
    
    /// Source storage type (for debugging/observability)
    pub storage_type: StorageType,
    
    /// Location reference (for cache invalidation tracking)
    pub location: AccountLocation,
}

impl AccountView {
    /// Create AccountView from Solana native account
    pub fn from_solana_account(
        pubkey: Pubkey,
        account: &SolanaAccount,
        slot: Slot,
        write_version: u64,
        location: AccountLocation,
    ) -> Self {
        Self {
            pubkey,
            lamports: account.lamports,
            data: Arc::new(account.data.clone()),
            owner: account.owner,
            executable: account.executable,
            slot,
            write_version,
            rent_epoch: account.rent_epoch,
            storage_type: location.storage_type,
            location,
        }
    }

    /// Convert to Solana native account (for RPC responses)
    pub fn to_solana_account(&self) -> SolanaAccount {
        SolanaAccount {
            lamports: self.lamports,
            data: Vec::from(self.data.as_slice()),
            owner: self.owner,
            executable: self.executable,
            rent_epoch: self.rent_epoch,
        }
    }

    /// Get account data size
    #[inline]
    pub fn data_size(&self) -> usize {
        self.data.len()
    }

    /// Check if account is newer than a given slot/version
    #[inline]
    pub fn is_at_least(&self, slot: Slot, write_version: u64) -> bool {
        if self.slot != slot {
            self.slot >= slot
        } else {
            self.write_version >= write_version
        }
    }

    /// Get account discriminator (first 8 bytes) if present
    #[inline]
    pub fn discriminator(&self) -> Option<[u8; 8]> {
        if self.data.len() >= 8 {
            Some(self.data[0..8].try_into().unwrap())
        } else {
            None
        }
    }
}

/// Lightweight account metadata for indexing
/// 
/// Contains only the fields needed for index building and query planning,
/// not the full account data. Used to reduce memory pressure in indexes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AccountMetadata {
    pub pubkey: Pubkey,
    pub owner: Pubkey,
    pub data_len: u32,
    pub lamports: u64,
    pub executable: bool,
    pub slot: Slot,
    pub write_version: u64,
}

impl AccountMetadata {
    pub fn from_account_view(account: &AccountView) -> Self {
        Self {
            pubkey: account.pubkey,
            owner: account.owner,
            data_len: account.data.len() as u32,
            lamports: account.lamports,
            executable: account.executable,
            slot: account.slot,
            write_version: account.write_version,
        }
    }
}

/// Account update envelope from Geyser
/// 
/// Minimal wrapper for efficient Geyser update processing.
/// Designed to be copied quickly from Geyser callback to ring buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeyserAccountUpdate {
    pub pubkey: Pubkey,
    pub lamports: u64,
    pub owner: Pubkey,
    pub executable: bool,
    pub slot: Slot,
    pub write_version: u64,
    pub rent_epoch: u64,
    #[serde(serialize_with = "serialize_arc_vec", deserialize_with = "deserialize_arc_vec")]
    pub data: Arc<Vec<u8>>,
    pub tx_index: Option<usize>,
    pub is_startup: bool,
}

impl GeyserAccountUpdate {
    /// Convert to AccountView
    pub fn into_account_view(self, location: AccountLocation) -> AccountView {
        AccountView {
            pubkey: self.pubkey,
            lamports: self.lamports,
            data: self.data,
            owner: self.owner,
            executable: self.executable,
            slot: self.slot,
            write_version: self.write_version,
            rent_epoch: self.rent_epoch,
            storage_type: location.storage_type,
            location,
        }
    }

    /// Create from Solana account (for Geyser plugin)
    pub fn from_solana_account(
        pubkey: Pubkey,
        account: &SolanaAccount,
        slot: Slot,
        write_version: u64,
        is_startup: bool,
    ) -> Self {
        Self {
            pubkey,
            lamports: account.lamports,
            owner: account.owner,
            executable: account.executable,
            slot,
            write_version,
            rent_epoch: account.rent_epoch,
            data: Arc::new(account.data.clone()),
            tx_index: None,
            is_startup,
        }
    }
}

/// Batch of account updates for efficient processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountUpdateBatch {
    pub updates: Vec<GeyserAccountUpdate>,
    pub slot: Slot,
    pub parent_slot: Slot,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_view_conversion() {
        let solana_account = SolanaAccount {
            lamports: 1000,
            data: vec![1, 2, 3, 4],
            owner: Pubkey::default(),
            executable: false,
            rent_epoch: 0,
        };

        let location = AccountLocation::new_base(1, 0, 100, 0, 4, 100, 1);
        let view = AccountView::from_solana_account(
            Pubkey::default(),
            &solana_account,
            100,
            1,
            location,
        );

        assert_eq!(view.lamports, 1000);
        assert_eq!(view.data_size(), 4);
        assert_eq!(view.slot, 100);

        let converted = view.to_solana_account();
        assert_eq!(converted.lamports, 1000);
        assert_eq!(converted.data, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_discriminator_extraction() {
        let account = AccountView {
            pubkey: Pubkey::default(),
            lamports: 0,
            data: Arc::new(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
            owner: Pubkey::default(),
            executable: false,
            slot: 0,
            write_version: 0,
            rent_epoch: 0,
            storage_type: StorageType::Base,
            location: AccountLocation::new_base(0, 0, 0, 0, 10, 0, 0),
        };

        assert_eq!(account.discriminator(), Some([1, 2, 3, 4, 5, 6, 7, 8]));
    }
}
