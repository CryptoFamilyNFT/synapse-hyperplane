//! AppendVec compatibility layer for Agave account file format
//!
//! This module provides compatibility with Agave's AppendVec format
//! for reading account files. The goal is to read Agave's format
//! without depending on internal Agave APIs that may change.
//!
//! # References
//!
//! Based on analysis from:
//! - https://www.anza.xyz/blog/a-deep-dive-into-solana-s-accountsdb
//! - Agave source code (accounts-db/src/append_vec.rs)

use crate::account_parser::{parse_account_record, ParsedAccount};
use hyperplane_types::AccountLocation;
use std::path::Path;
use tracing::{debug, warn};

/// AppendVec magic number (if present in header)
const APPENDVEC_MAGIC: u32 = 0xC000_0000;

/// AppendVec version (varies by Agave version)
const APPENDVEC_VERSION: u32 = 0;

/// Header size for AppendVec files (if versioned)
const APPENDVEC_HEADER_SIZE: usize = 8; // magic (4) + version (4)

/// AppendVec file scanner
/// 
/// Scans account files that may have AppendVec headers
pub struct AppendVecScanner {
    has_header: bool,
    header_size: usize,
}

impl AppendVecScanner {
    pub fn new() -> Self {
        Self {
            has_header: false, // Default: no header (plain account files)
            header_size: 0,
        }
    }

    /// Enable AppendVec header parsing
    pub fn with_header(mut self) -> Self {
        self.has_header = true;
        self.header_size = APPENDVEC_HEADER_SIZE;
        self
    }

    /// Check if file has AppendVec header
    pub fn detect_header(bytes: &[u8]) -> bool {
        if bytes.len() < APPENDVEC_HEADER_SIZE {
            return false;
        }
        
        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        
        magic == APPENDVEC_MAGIC && version == APPENDVEC_VERSION
    }

    /// Scan account file and extract all accounts
    /// 
    /// Format:
    /// ```text
    /// [Optional: AppendVec header (8 bytes)]
    /// [Account Record 1]
    /// [Account Record 2]
    /// ...
    /// [Account Record N]
    /// ```
    pub fn scan_file(
        &self,
        file_id: u64,
        bytes: &[u8],
    ) -> Vec<Result<ParsedAccount, crate::account_parser::ParseError>> {
        let mut accounts = Vec::new();
        let mut offset = if self.has_header { self.header_size as u64 } else { 0 };
        
        while offset < bytes.len() as u64 {
            // Check if we have enough bytes for stored_size
            if offset + 8 > bytes.len() as u64 {
                break;
            }
            
            // Read stored_size to determine record length
            let stored_size = u64::from_le_bytes(
                bytes[offset as usize..(offset + 8) as usize]
                    .try_into()
                    .unwrap(),
            );
            
            if stored_size == 0 || stored_size > (bytes.len() as u64 - offset) {
                // End of valid accounts or corrupted file
                debug!(
                    "End of accounts at offset {}: stored_size={}",
                    offset, stored_size
                );
                break;
            }
            
            // Parse account record
            let record_bytes = &bytes[offset as usize..(offset + stored_size) as usize];
            match parse_account_record(file_id, offset, record_bytes) {
                Ok(account) => {
                    accounts.push(Ok(account));
                }
                Err(e) => {
                    warn!("Failed to parse account at offset {}: {}", offset, e);
                    // Continue scanning next record
                }
            }
            
            // Move to next record (align to 8 bytes)
            offset += crate::account_parser::align_to_8(stored_size);
        }
        
        accounts
    }

    /// Get account at specific offset
    pub fn get_account_at_offset(
        &self,
        file_id: u64,
        offset: u64,
        bytes: &[u8],
    ) -> Result<ParsedAccount, crate::account_parser::ParseError> {
        let actual_offset = if self.has_header {
            offset + self.header_size as u64
        } else {
            offset
        };
        
        if actual_offset + 8 > bytes.len() as u64 {
            return Err(crate::account_parser::ParseError::MetadataOverflow);
        }
        
        let stored_size = u64::from_le_bytes(
            bytes[actual_offset as usize..(actual_offset + 8) as usize]
                .try_into()
                .unwrap(),
        );
        
        let record_bytes = &bytes[actual_offset as usize..(actual_offset + stored_size) as usize];
        parse_account_record(file_id, actual_offset, record_bytes)
    }
}

impl Default for AppendVecScanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Scan account file from disk
pub fn scan_account_file<P: AsRef<Path>>(
    file_id: u64,
    path: P,
) -> std::io::Result<Vec<ParsedAccount>> {
    use crate::mmap_reader::MmapAccountFile;
    
    let mmap_file = MmapAccountFile::open(file_id, path)?;
    let scanner = AppendVecScanner::new();
    
    let bytes = mmap_file.mmap.as_ref();
    let accounts = scanner
        .scan_file(file_id, bytes)
        .into_iter()
        .filter_map(|result| result.ok())
        .collect();
    
    Ok(accounts)
}

/// Get account location from file
pub fn locate_account(
    file_id: u64,
    bytes: &[u8],
    target_pubkey: solana_sdk::pubkey::Pubkey,
) -> Option<AccountLocation> {
    let scanner = AppendVecScanner::new();
    let accounts = scanner.scan_file(file_id, bytes);
    
    for result in accounts {
        if let Ok(account) = result {
            if account.pubkey == target_pubkey {
                return Some(account.location);
            }
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey::Pubkey;
    use std::io::Write;
    use tempfile::tempdir;
    use create::std::PathBuf;
    use std::fs::File;

    fn create_test_account_file(
        dir: &Path,
        file_id: u64,
        accounts: &[(Pubkey, Vec<u8>)],
    ) -> (PathBuf, Vec<Pubkey>) {
        let path = dir.join(format!("{:024}.bin", file_id));
        let mut file = File::create(&path).unwrap();
        let mut pubkeys = Vec::new();
        
        let owner = Pubkey::new_unique();
        
        for (pubkey, data) in accounts {
            pubkeys.push(*pubkey);
            
            let lamports = 1000u64;
            let slot = 100u64;
            
            // Build account record (matches account_parser format)
            let mut record = Vec::new();
            
            // First write stored_size (8 bytes)
            let account_data_size = 97 + data.len(); // pubkey(32) + owner(32) + flags(1) + rent_epoch(8) + lamports(8) + slot(8) + data
            record.extend_from_slice(&(account_data_size as u64).to_le_bytes());
            
            // Then account data
            record.extend_from_slice(&pubkey.to_bytes());
            record.extend_from_slice(&owner.to_bytes());
            record.push(0); // executable
            record.extend_from_slice(&0u64.to_le_bytes()); // rent_epoch
            record.extend_from_slice(&lamports.to_le_bytes());
            record.extend_from_slice(&slot.to_le_bytes());
            record.extend_from_slice(&data);
            
            file.write_all(&record).unwrap();
            
            // Add padding to 8-byte alignment
            let padding = crate::account_parser::padding_needed(record.len(), 8);
            if padding > 0 {
                file.write_all(&vec![0u8; padding]).unwrap();
            }
        }
        
        file.flush().unwrap();
        (path, pubkeys)
    }

    #[test]
    fn test_scan_account_file() {
        let dir = tempdir().unwrap();
        let accounts_data: Vec<(Pubkey, Vec<u8>)> = (0..5)
            .map(|i| (Pubkey::new_unique(), vec![i as u8; 10]))
            .collect();
        
        let (path, pubkeys) = create_test_account_file(dir.path(), 5, &accounts_data);
        
        let accounts = scan_account_file(0, &path).unwrap();
        assert_eq!(accounts.len(), 5);
        
        for (i, account) in accounts.iter().enumerate() {
            assert_eq!(account.pubkey, pubkeys[i]);
            assert_eq!(account.data.len(), 10);
        }
    }

    #[test]
    fn test_appendvec_header_detection() {
        let mut bytes_with_header = Vec::new();
        bytes_with_header.extend_from_slice(&APPENDVEC_MAGIC.to_le_bytes());
        bytes_with_header.extend_from_slice(&APPENDVEC_VERSION.to_le_bytes());
        bytes_with_header.extend_from_slice(&[0u8; 100]); // dummy data
        
        assert!(AppendVecScanner::detect_header(&bytes_with_header));
        
        let bytes_without_header = vec![0u8; 100];
        assert!(!AppendVecScanner::detect_header(&bytes_without_header));
    }
}
