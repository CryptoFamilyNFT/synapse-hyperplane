//! Memory-mapped file reader for zero-copy account access
//!
//! Uses memmap2 for efficient memory mapping of large account files.
//! Supports concurrent reads without locks (mmap is naturally read-safe).

use hyperplane_types::AccountLocation;
use memmap2::Mmap;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

/// Error types for mmap operations
#[derive(Debug, Error)]
pub enum MmapError {
    #[error("Failed to open file: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("File too small: expected {expected} bytes, got {actual}")]
    FileTooSmall { expected: usize, actual: usize },
    
    #[error("Invalid account header at offset {offset}")]
    InvalidHeader { offset: u64 },
    
    #[error("Read beyond file bounds: offset={offset}, len={len}, file_size={file_size}")]
    OutOfBounds {
        offset: u64,
        len: usize,
        file_size: usize,
    },
}

impl From<MmapError> for std::io::Error {
    fn from(err: MmapError) -> Self {
        match err {
            MmapError::IoError(e) => e,
            MmapError::FileTooSmall { expected, actual } => {
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, format!("File too small: expected {} bytes, got {}", expected, actual))
            }
            MmapError::InvalidHeader { offset } => {
                std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Invalid account header at offset {}", offset))
            }
            MmapError::OutOfBounds { offset, len, file_size } => {
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, format!("Read beyond file bounds: offset={}, len={}, file_size={}", offset, len, file_size))
            }
        }
    }
}

/// Memory-mapped account file
/// 
/// Provides zero-copy read access to account data.
/// Thread-safe via Arc (multiple readers can share the same mmap).
#[derive(Debug, Clone)]
pub struct MmapAccountFile {
    /// File identifier (matches AccountLocation.file_id)
    pub file_id: u64,
    
    /// Memory-mapped data
    pub mmap: Arc<Mmap>,
    
    /// File size in bytes
    pub file_size: usize,
    
    /// Path to the file (for debugging/logging)
    pub path: String,
}

impl MmapAccountFile {
    /// Open and mmap a file
    pub fn open<P: AsRef<Path>>(file_id: u64, path: P) -> Result<Self, MmapError> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        let file = std::fs::File::open(path.as_ref())?;
        let metadata = file.metadata()?;
        let file_size = metadata.len() as usize;
        
        // Safety: We only read from the mmap, never write.
        // Multiple threads can safely read concurrently.
        let mmap = unsafe { Mmap::map(&file)? };
        
        Ok(Self {
            file_id,
            mmap: Arc::new(mmap),
            file_size,
            path: path_str,
        })
    }

    /// Read raw bytes at a location
    /// 
    /// Returns a copy of the bytes (not zero-copy, but safe for async contexts).
    /// For true zero-copy, use `get_slice()` with lifetime constraints.
    pub fn read_bytes(&self, location: &AccountLocation) -> Result<Vec<u8>, MmapError> {
        let start = location.offset as usize;
        let end = start + location.stored_size as usize;
        
        if end > self.file_size {
            return Err(MmapError::OutOfBounds {
                offset: location.offset,
                len: location.stored_size as usize,
                file_size: self.file_size,
            });
        }
        
        Ok(self.mmap[start..end].to_vec())
    }

    /// Get zero-copy slice at a location
    /// 
    /// WARNING: The returned slice is tied to the lifetime of self.
    /// Do not use in async contexts where the mmap might be dropped.
    pub fn get_slice(&self, location: &AccountLocation) -> Result<&[u8], MmapError> {
        let start = location.offset as usize;
        let end = start + location.stored_size as usize;
        
        if end > self.file_size {
            return Err(MmapError::OutOfBounds {
                offset: location.offset,
                len: location.stored_size as usize,
                file_size: self.file_size,
            });
        }
        
        Ok(&self.mmap[start..end])
    }

    /// Get account data payload only (strips metadata/header)
    pub fn get_account_data(&self, location: &AccountLocation) -> Result<Vec<u8>, MmapError> {
        let start = (location.offset + location.data_offset as u64) as usize;
        let end = start + location.data_len as usize;
        
        if end > self.file_size {
            return Err(MmapError::OutOfBounds {
                offset: location.offset + location.data_offset as u64,
                len: location.data_len as usize,
                file_size: self.file_size,
            });
        }
        
        Ok(self.mmap[start..end].to_vec())
    }

    /// Get file size
    #[inline]
    pub fn file_size(&self) -> usize {
        self.file_size
    }

    /// Check if location is within file bounds
    #[inline]
    pub fn is_valid_location(&self, location: &AccountLocation) -> bool {
        let end = location.offset as usize + location.stored_size as usize;
        end <= self.file_size
    }

    /// Advise kernel about access pattern (optional optimization)
    pub fn advise_random(&self) {
        let _ = self.mmap.advise(memmap2::Advice::Random);
    }

    /// Advise sequential access pattern
    pub fn advise_sequential(&self) {
        let _ = self.mmap.advise(memmap2::Advice::Sequential);
    }

    /// Advise will-need for prefetching
    pub fn advise_willneed(&self, offset: usize, len: usize) {
        let _ = self.mmap.advise_range(memmap2::Advice::WillNeed, offset, len);
    }
}

/// Batch reader for efficient multi-account reads
pub struct MmapBatchReader {
    files: Vec<MmapAccountFile>,
}

impl MmapBatchReader {
    pub fn new(files: Vec<MmapAccountFile>) -> Self {
        Self { files }
    }

    /// Read multiple accounts in parallel
    pub fn read_batch(
        &self,
        locations: &[(u64, AccountLocation)], // (file_id, location)
    ) -> Vec<Result<Vec<u8>, MmapError>> {
        use rayon::prelude::*;
        
        locations
            .par_iter()
            .map(|(file_id, location)| {
                if let Some(file) = self.files.iter().find(|f| f.file_id == *file_id) {
                    file.read_bytes(location)
                } else {
                    Err(MmapError::IoError(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("File {} not found", file_id),
                    )))
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_mmap_open_and_read() {
        // Create test file
        let mut file = NamedTempFile::new().unwrap();
        let test_data = vec![0u8; 1024];
        file.write_all(&test_data).unwrap();
        
        let mmap_file = MmapAccountFile::open(0, file.path()).unwrap();
        assert_eq!(mmap_file.file_size(), 1024);
        
        let location = AccountLocation::new_base(0, 0, 100, 0, 100, 1, 1);
        let bytes = mmap_file.read_bytes(&location).unwrap();
        assert_eq!(bytes.len(), 100);
    }

    #[test]
    fn test_out_of_bounds_detection() {
        let mut file = NamedTempFile::new().unwrap();
        let test_data = vec![0u8; 100];
        file.write_all(&test_data).unwrap();
        
        let mmap_file = MmapAccountFile::open(0, file.path()).unwrap();
        
        // Try to read beyond file end
        let location = AccountLocation::new_base(0, 50, 100, 0, 100, 1, 1);
        let result = mmap_file.read_bytes(&location);
        assert!(result.is_err());
        
        if let Err(MmapError::OutOfBounds { .. }) = result {
            // Expected
        } else {
            panic!("Expected OutOfBounds error");
        }
    }
}
