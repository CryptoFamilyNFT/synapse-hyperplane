//! High-level account file scanner
//!
//! Orchestrates the scanning of Agave /accounts directory,
//! building the base locator and pubkey dictionary.

use crate::account_parser::ParsedAccount;
use crate::appendvec_compat::AppendVecScanner;
use crate::mmap_reader::MmapAccountFile;
use hyperplane_types::{AccountLocation, PubkeyDictionary};
use parking_lot::RwLock;
use rayon::prelude::*;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tracing::{error, info};

/// Scanner errors
#[derive(Debug, Error)]
pub enum ScannerError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Mmap error: {0}")]
    MmapError(#[from] crate::mmap_reader::MmapError),
    
    #[error("Parse error: {0}")]
    ParseError(#[from] crate::account_parser::ParseError),
    
    #[error("Directory not found: {0}")]
    DirectoryNotFound(PathBuf),
    
    #[error("No account files found in {0}")]
    NoAccountFiles(PathBuf),
}

/// Scan result for a single file
pub struct FileScanResult {
    pub file_id: u64,
    pub accounts: Vec<ParsedAccount>,
    pub file_size: usize,
    pub scan_duration_ms: u128,
}

/// Complete scan result
pub struct ScanResult {
    /// All parsed accounts
    pub accounts: Vec<ParsedAccount>,
    /// File scan results
    pub file_results: Vec<FileScanResult>,
    /// Total scan duration
    pub total_duration_ms: u128,
    /// Statistics
    pub stats: ScanStats,
}

/// Scan statistics
#[derive(Debug, Clone)]
pub struct ScanStats {
    pub files_scanned: usize,
    pub total_accounts: usize,
    pub total_bytes: u64,
    pub avg_account_size: f64,
    pub accounts_per_second: f64,
}

/// Configuration for account file scanner
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    /// Path to Agave accounts directory
    pub accounts_path: PathBuf,
    /// Number of parallel scan threads
    pub num_threads: usize,
    /// Whether to detect AppendVec headers
    pub detect_appendvec: bool,
    /// Whether to validate account records
    pub validate_accounts: bool,
    /// Log progress every N files
    pub progress_interval: usize,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            accounts_path: PathBuf::from("/mnt/accounts"),
            num_threads: num_cpus::get(),
            detect_appendvec: false,
            validate_accounts: true,
            progress_interval: 10,
        }
    }
}

/// Account file scanner
/// 
/// Scans Agave /accounts directory and extracts all accounts
pub struct AccountFileScanner {
    config: ScannerConfig,
    mmap_files: Arc<RwLock<HashMap<u64, MmapAccountFile>>>,
}

impl AccountFileScanner {
    pub fn new(config: ScannerConfig) -> Self {
        Self {
            config,
            mmap_files: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Scan all account files in directory
    pub fn scan(&self) -> Result<ScanResult, ScannerError> {
        let start = std::time::Instant::now();
        
        // Find all account files
        let files = self.discover_account_files()?;
        info!("Found {} account files", files.len());
        
        if files.is_empty() {
            return Err(ScannerError::NoAccountFiles(self.config.accounts_path.clone()));
        }
        
        // Scan files in parallel
        let file_results: Vec<FileScanResult> = files
            .par_iter()
            .enumerate()
            .filter_map(|(idx, (file_id, path))| {
                if idx % self.config.progress_interval == 0 {
                    info!("Scanning file {}/{}", idx + 1, files.len());
                }
                
                match self.scan_file(*file_id, path) {
                    Ok(result) => Some(result),
                    Err(e) => {
                        error!("Failed to scan file {:?}: {}", path, e);
                        None
                    }
                }
            })
            .collect();
        
        // Collect all accounts
        let accounts: Vec<ParsedAccount> = file_results
            .iter()
            .flat_map(|fr| fr.accounts.iter().cloned())
            .collect();
        
        let total_duration = start.elapsed().as_millis();
        
        let stats = ScanStats {
            files_scanned: file_results.len(),
            total_accounts: accounts.len(),
            total_bytes: file_results.iter().map(|fr| fr.file_size as u64).sum(),
            avg_account_size: accounts
                .iter()
                .map(|a| a.data.len() as f64)
                .sum::<f64>()
                / accounts.len() as f64,
            accounts_per_second: if total_duration > 0 {
                (accounts.len() as f64 / total_duration as f64) * 1000.0
            } else {
                0.0
            },
        };
        
        info!(
            "Scan complete: {} accounts from {} files in {}ms ({:.2} accounts/sec)",
            stats.total_accounts,
            stats.files_scanned,
            total_duration,
            stats.accounts_per_second
        );
        
        Ok(ScanResult {
            accounts,
            file_results,
            total_duration_ms: total_duration,
            stats,
        })
    }

    /// Scan a single file
    fn scan_file(&self, file_id: u64, path: &Path) -> Result<FileScanResult, ScannerError> {
        let start = std::time::Instant::now();
        
        // Open and mmap file
        let mmap_file = MmapAccountFile::open(file_id, path)?;
        let file_size = mmap_file.file_size();
        
        // Store mmap for later use
        self.mmap_files.write().insert(file_id, mmap_file.clone());
        
        // Scan file
        let scanner = AppendVecScanner::new();
        let bytes = mmap_file.mmap.as_ref();
        let accounts: Vec<ParsedAccount> = scanner
            .scan_file(file_id, bytes)
            .into_iter()
            .filter_map(|result| result.ok())
            .collect();
        
        let duration = start.elapsed().as_millis();
        
        Ok(FileScanResult {
            file_id,
            accounts,
            file_size,
            scan_duration_ms: duration,
        })
    }

    /// Discover account files in directory
    fn discover_account_files(&self) -> Result<Vec<(u64, PathBuf)>, ScannerError> {
        if !self.config.accounts_path.exists() {
            return Err(ScannerError::DirectoryNotFound(
                self.config.accounts_path.clone(),
            ));
        }
        
        let mut files = Vec::new();
        
        for entry in std::fs::read_dir(&self.config.accounts_path)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() {
                // Parse file ID from filename (e.g., "123456789.0")
                if let Some(file_id) = self.parse_file_id(&path) {
                    files.push((file_id, path));
                }
            }
        }
        
        // Sort by file_id for deterministic ordering
        files.sort_by_key(|(file_id, _)| *file_id);
        
        Ok(files)
    }

    /// Parse file ID from path
    fn parse_file_id(&self, path: &Path) -> Option<u64> {
        // Expected format: "{file_id}.{version}" e.g., "123456789.0"
        let filename = path.file_name()?.to_str()?;
        let parts: Vec<&str> = filename.split('.').collect();
        
        if parts.len() >= 2 {
            parts[0].parse().ok()
        } else {
            None
        }
    }

    /// Get mmap file by ID
    pub fn get_mmap_file(&self, file_id: u64) -> Option<MmapAccountFile> {
        self.mmap_files.read().get(&file_id).cloned()
    }

    /// Clear mmap cache (free memory)
    pub fn clear_mmap_cache(&self) {
        self.mmap_files.write().clear();
    }
}

/// Build pubkey dictionary from scan result
pub fn build_pubkey_dictionary(accounts: &[ParsedAccount]) -> PubkeyDictionary {
    let mut dict = PubkeyDictionary::new();
    
    for account in accounts {
        dict.insert(account.pubkey);
    }
    
    dict
}

/// Build base locator from scan result
pub fn build_base_locator(
    accounts: &[ParsedAccount],
) -> HashMap<Pubkey, AccountLocation> {
    let mut locator = HashMap::with_capacity(accounts.len());
    
    for account in accounts {
        locator.insert(account.pubkey, account.location);
    }
    
    locator
}

// Helper: get number of CPUs if num_cpus crate not available
mod num_cpus {
    pub fn get() -> usize {
        std::thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_accounts_dir(num_files: usize, accounts_per_file: usize) -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        
        for file_id in 0..num_files {
            let path = temp_dir.path().join(format!("{}.0", file_id));
            let mut file = std::fs::File::create(&path).unwrap();
            
            for _ in 0..accounts_per_file {
                let pubkey = Pubkey::new_unique();
                let owner = Pubkey::default();
                let data = vec![0u8; 100];
                let lamports = 1000;
                let slot = 100;
                
                // Write account record
                let stored_size = 97 + data.len();
                file.write_all(&(stored_size as u64).to_le_bytes()).unwrap();
                file.write_all(&pubkey.to_bytes()).unwrap();
                file.write_all(&owner.to_bytes()).unwrap();
                file.write_all(&[0]).unwrap(); // executable
                file.write_all(&0u64.to_le_bytes()).unwrap(); // rent_epoch
                let lamports: u64 = 1000;
                file.write_all(&lamports.to_le_bytes()).unwrap();
                let slot: u64 = 100;
                file.write_all(&slot.to_le_bytes()).unwrap();
                file.write_all(&data).unwrap();
                
                // Padding
                let padding = crate::account_parser::padding_needed(stored_size, 8);
                if padding > 0 {
                    file.write_all(&vec![0u8; padding]).unwrap();
                }
            }
        }
        
        temp_dir
    }

    #[test]
    fn test_scanner_discovery() {
        let temp_dir = create_test_accounts_dir(3, 10);
        
        let config = ScannerConfig {
            accounts_path: temp_dir.path().to_path_buf(),
            num_threads: 2,
            ..Default::default()
        };
        
        let scanner = AccountFileScanner::new(config);
        let result = scanner.scan().unwrap();
        
        assert_eq!(result.stats.files_scanned, 3);
        assert_eq!(result.stats.total_accounts, 30);
    }

    #[test]
    fn test_file_id_parsing() {
        let scanner = AccountFileScanner::new(ScannerConfig::default());
        
        let path = Path::new("/test/123456789.0");
        assert_eq!(scanner.parse_file_id(path), Some(123456789));
        
        let path = Path::new("/test/987654321.1");
        assert_eq!(scanner.parse_file_id(path), Some(987654321));
        
        let path = Path::new("/test/invalid");
        assert_eq!(scanner.parse_file_id(path), None);
    }
}
