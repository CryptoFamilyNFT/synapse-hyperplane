//! Account data parser for Agave account file format
//!
//! Parses account records from Agave's account storage format.
//! Compatible with AppendVec/StorableAccounts layout.
//!
//! # Account File Layout (simplified)
//!
//! ```text
//! Account Record:
//!   - Stored size (u64): total bytes including header
//!   - Account metadata (88 bytes):
//!       - pubkey (32 bytes)
//!       - owner (32 bytes)
//!       - executable (bool)
//!       - rent_epoch (u64)
//!       - lamports (u64)
//!       - slot (u64)
//!   - Account data (variable)
//!   - Padding to 8-byte alignment
//! ```

use hyperplane_types::{AccountLocation, StorageType};
use solana_sdk::{account::Account, pubkey::Pubkey};
use thiserror::Error;

/// Account parsing errors
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("Invalid stored size: {0}")]
    InvalidStoredSize(u64),
    
    #[error("Account data too small for metadata")]
    MetadataOverflow,
    
    #[error("Invalid pubkey")]
    InvalidPubkey,
    
    #[error("Data length mismatch: expected {expected}, got {actual}")]
    DataLengthMismatch { expected: usize, actual: usize },
    
    #[error("Alignment error: offset {offset} not aligned to {alignment}")]
    AlignmentError { offset: u64, alignment: usize },
}

/// Parsed account record from file
#[derive(Debug, Clone)]
pub struct ParsedAccount {
    pub pubkey: Pubkey,
    pub owner: Pubkey,
    pub lamports: u64,
    pub executable: bool,
    pub rent_epoch: u64,
    pub slot: u64,
    pub write_version: u64,
    pub data: Vec<u8>,
    pub location: AccountLocation,
}

impl ParsedAccount {
    /// Convert to Solana Account
    pub fn to_account(&self) -> Account {
        Account {
            lamports: self.lamports,
            data: self.data.clone(),
            owner: self.owner,
            executable: self.executable,
            rent_epoch: self.rent_epoch,
        }
    }
}

/// Account metadata structure (88 bytes in Agave)
#[derive(Debug, Clone)]
pub struct AccountMetadata {
    pub pubkey: Pubkey,
    pub owner: Pubkey,
    pub executable: bool,
    pub rent_epoch: u64,
    pub lamports: u64,
}

/// Parse account record from raw bytes
/// 
/// Expected format:
/// - bytes 0..8: stored_size (u64 LE)
/// - bytes 8..40: pubkey
/// - bytes 40..72: owner
/// - bytes 72: executable (bool)
/// - bytes 73..81: rent_epoch (u64 LE)
/// - bytes 81..89: lamports (u64 LE)
/// - bytes 89..97: slot (u64 LE)
/// - bytes 97..: data
pub fn parse_account_record(
    file_id: u64,
    offset: u64,
    bytes: &[u8],
) -> Result<ParsedAccount, ParseError> {
    if bytes.len() < 8 {
        return Err(ParseError::MetadataOverflow);
    }
    
    // Read stored size (first 8 bytes)
    let stored_size = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    
    if stored_size as usize > bytes.len() {
        return Err(ParseError::InvalidStoredSize(stored_size));
    }
    
    // Parse metadata starting at byte 8
    if bytes.len() < 97 {
        return Err(ParseError::MetadataOverflow);
    }
    
    let pubkey = Pubkey::try_from(&bytes[8..40])
        .map_err(|_| ParseError::InvalidPubkey)?;
    
    let owner = Pubkey::try_from(&bytes[40..72])
        .map_err(|_| ParseError::InvalidPubkey)?;
    
    let executable = bytes[72] != 0;
    let rent_epoch = u64::from_le_bytes(bytes[73..81].try_into().unwrap());
    let lamports = u64::from_le_bytes(bytes[81..89].try_into().unwrap());
    let slot = u64::from_le_bytes(bytes[89..97].try_into().unwrap());
    
    // Data starts at byte 97
    let data_offset = 97u32;
    let data = bytes[97..].to_vec();
    let data_len = data.len() as u32;
    
    // Write version is not stored in account file, use 0 for base layer
    // (will be overridden by Geyser updates in delta layer)
    let write_version = 0;
    
    let location = AccountLocation {
        file_id,
        offset,
        stored_size: stored_size as u32,
        data_offset,
        data_len,
        slot,
        write_version,
        storage_type: StorageType::Base,
    };
    
    Ok(ParsedAccount {
        pubkey,
        owner,
        lamports,
        executable,
        rent_epoch,
        slot,
        write_version,
        data,
        location,
    })
}

/// Get account data size from raw bytes without full parsing
pub fn get_account_data_size(bytes: &[u8]) -> Result<usize, ParseError> {
    if bytes.len() < 97 {
        return Err(ParseError::MetadataOverflow);
    }
    
    let stored_size = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    if stored_size as usize > bytes.len() {
        return Err(ParseError::InvalidStoredSize(stored_size));
    }
    
    // Data size = stored_size - metadata_size (97 bytes)
    // But we need to account for padding
    Ok(stored_size as usize - 97)
}

/// Get owner from raw bytes without full parsing (for index building)
pub fn get_account_owner(bytes: &[u8]) -> Result<Pubkey, ParseError> {
    if bytes.len() < 72 {
        return Err(ParseError::MetadataOverflow);
    }
    
    Pubkey::try_from(&bytes[40..72])
        .map_err(|_| ParseError::InvalidPubkey)
}

/// Get pubkey from raw bytes without full parsing
pub fn get_account_pubkey(bytes: &[u8]) -> Result<Pubkey, ParseError> {
    if bytes.len() < 40 {
        return Err(ParseError::MetadataOverflow);
    }
    
    Pubkey::try_from(&bytes[8..40])
        .map_err(|_| ParseError::InvalidPubkey)
}

/// Check if bytes represent a valid account record
pub fn validate_account_record(bytes: &[u8]) -> Result<(), ParseError> {
    if bytes.len() < 8 {
        return Err(ParseError::MetadataOverflow);
    }
    
    let stored_size = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    if stored_size as usize > bytes.len() {
        return Err(ParseError::InvalidStoredSize(stored_size));
    }
    
    Ok(())
}

/// Align offset to 8-byte boundary
#[inline]
pub fn align_to_8(offset: u64) -> u64 {
    (offset + 7) & !7
}

/// Calculate padding needed for alignment
#[inline]
pub fn padding_needed(size: usize, alignment: usize) -> usize {
    let remainder = size % alignment;
    if remainder == 0 {
        0
    } else {
        alignment - remainder
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey::Pubkey;

    fn create_test_account_record(
        pubkey: Pubkey,
        owner: Pubkey,
        lamports: u64,
        data: &[u8],
        slot: u64,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        
        // Metadata size: 8 (stored_size) + 88 (metadata) = 96
        // But we include stored_size in the count, so: 8 + 88 + data_len
        let stored_size = 97 + data.len();
        
        // Stored size
        bytes.extend_from_slice(&(stored_size as u64).to_le_bytes());
        
        // Pubkey
        bytes.extend_from_slice(&pubkey.to_bytes());
        
        // Owner
        bytes.extend_from_slice(&owner.to_bytes());
        
        // Executable
        bytes.push(0);
        
        // Rent epoch
        bytes.extend_from_slice(&0u64.to_le_bytes());
        
        // Lamports
        bytes.extend_from_slice(&lamports.to_le_bytes());
        
        // Slot
        bytes.extend_from_slice(&slot.to_le_bytes());
        
        // Data
        bytes.extend_from_slice(data);
        
        bytes
    }

    #[test]
    fn test_parse_account_record() {
        let pubkey = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let data = vec![1, 2, 3, 4, 5];
        let lamports = 1000;
        let slot = 100;
        
        let record = create_test_account_record(pubkey, owner, lamports, &data, slot);
        
        let parsed = parse_account_record(0, 0, &record).unwrap();
        
        assert_eq!(parsed.pubkey, pubkey);
        assert_eq!(parsed.owner, owner);
        assert_eq!(parsed.lamports, lamports);
        assert_eq!(parsed.data, data);
        assert_eq!(parsed.slot, slot);
    }

    #[test]
    fn test_get_account_data_size() {
        let pubkey = Pubkey::new_unique();
        let owner = Pubkey::new_unique();
        let data = vec![1, 2, 3, 4, 5];
        
        let record = create_test_account_record(pubkey, owner, 1000, &data, 100);
        
        let size = get_account_data_size(&record).unwrap();
        assert_eq!(size, data.len());
    }

    #[test]
    fn test_validate_account_record() {
        let pubkey = Pubkey::new_unique();
        let record = create_test_account_record(pubkey, Pubkey::default(), 1000, &[], 100);
        
        assert!(validate_account_record(&record).is_ok());
        
        // Invalid: too small
        assert!(validate_account_record(&[0u8; 5]).is_err());
    }
}
