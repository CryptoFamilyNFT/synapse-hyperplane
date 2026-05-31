//! Account location tracking - maps pubkey to physical storage location
//!
//! This is the core of the external pubkey locator system.
//! Each AccountLocation tells the engine exactly where to read an account's bytes.

use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::fmt;

/// Physical location of an account in storage
/// 
/// This structure replaces Agave's internal Account Index for the read-plane.
/// It maps a pubkey to a specific byte range in a storage file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountLocation {
    /// Storage file identifier (for base: file_id from /accounts, for delta: segment_id)
    pub file_id: u64,
    
    /// Byte offset within the file
    pub offset: u64,
    
    /// Total stored size including metadata (for base: includes header)
    pub stored_size: u32,
    
    /// Offset where actual account data starts (after header/metadata)
    pub data_offset: u32,
    
    /// Length of the account data payload
    pub data_len: u32,
    
    /// Slot when this account version was written
    pub slot: u64,
    
    /// Monotonic write version for ordering within same slot
    pub write_version: u64,
    
    /// Storage type discriminator
    pub storage_type: StorageType,
}

/// Discriminates between base (Agave files) and delta (Synapse segments) storage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StorageType {
    /// Base layer: read-only Agave /accounts files
    Base,
    /// Delta layer: Synapse append-only segment store
    Delta,
    /// Compacted Synapse base segments (future base)
    Compacted,
}

impl AccountLocation {
    /// Create a new base layer location from Agave account file
    #[inline]
    pub fn new_base(
        file_id: u64,
        offset: u64,
        stored_size: u32,
        data_offset: u32,
        data_len: u32,
        slot: u64,
        write_version: u64,
    ) -> Self {
        Self {
            file_id,
            offset,
            stored_size,
            data_offset,
            data_len,
            slot,
            write_version,
            storage_type: StorageType::Base,
        }
    }

    /// Create a new delta layer location from Geyser update
    #[inline]
    pub fn new_delta(
        segment_id: u64,
        offset: u64,
        data_len: u32,
        slot: u64,
        write_version: u64,
    ) -> Self {
        Self {
            file_id: segment_id,
            offset,
            stored_size: data_len,
            data_offset: 0,
            data_len,
            slot,
            write_version,
            storage_type: StorageType::Delta,
        }
    }

    /// Check if this location is newer than another for the same account
    /// 
    /// Uses slot-first, then write_version ordering as per Solana semantics
    #[inline]
    pub fn is_newer_than(&self, other: &Self) -> bool {
        if self.slot != other.slot {
            self.slot > other.slot
        } else {
            self.write_version > other.write_version
        }
    }

    /// Get the total byte range to read from storage
    #[inline]
    pub fn read_range(&self) -> std::ops::Range<u64> {
        self.offset..(self.offset + self.stored_size as u64)
    }

    /// Get the data payload range (after stripping metadata)
    #[inline]
    pub fn data_range(&self) -> std::ops::Range<u64> {
        (self.offset + self.data_offset as u64)
            ..(self.offset + self.data_offset as u64 + self.data_len as u64)
    }
}

/// Compressed location using pubkey_id instead of full pubkey
/// Used internally in bitmap indexes and cache keys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CompressedLocation {
    pub location: AccountLocation,
    pub pubkey_id: u64,
}

/// Batch of locations for efficient bulk operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationBatch {
    pub locations: Vec<(Pubkey, AccountLocation)>,
    pub slot_watermark: u64,
}

impl fmt::Display for StorageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Base => write!(f, "base"),
            Self::Delta => write!(f, "delta"),
            Self::Compacted => write!(f, "compacted"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_location_ordering() {
        let loc1 = AccountLocation::new_base(1, 0, 100, 0, 80, 100, 1);
        let loc2 = AccountLocation::new_base(1, 100, 100, 0, 80, 100, 2);
        let loc3 = AccountLocation::new_base(1, 200, 100, 0, 80, 101, 1);

        assert!(loc2.is_newer_than(&loc1)); // same slot, higher write_version
        assert!(loc3.is_newer_than(&loc1)); // higher slot
        assert!(loc3.is_newer_than(&loc2)); // higher slot
        assert!(!loc1.is_newer_than(&loc2));
    }

    #[test]
    fn test_read_ranges() {
        let loc = AccountLocation::new_base(1, 1000, 120, 20, 100, 100, 1);
        
        assert_eq!(loc.read_range(), 1000..1120);
        assert_eq!(loc.data_range(), 1020..1120);
    }
}
