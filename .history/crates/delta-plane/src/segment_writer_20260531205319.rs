//! Append-only segment writer for delta layer
//!
//! Writes Geyser account updates to immutable segment files.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write, Seek, SeekFrom, Read};
use std::path::Path;

/// Segment file header
#[repr(C)]
#[derive(Debug)]
pub struct SegmentHeader {
    /// Magic number for validation
    pub magic: u32,
    /// Segment version
    pub version: u32,
    /// Starting slot for this segment
    pub start_slot: u64,
    /// Ending slot for this segment
    pub end_slot: u64,
    /// Number of entries in segment
    pub entry_count: u64,
    /// Total data size in bytes
    pub data_size: u64,
    /// CRC32 checksum of header
    pub checksum: u32,
}

impl SegmentHeader {
    pub const MAGIC: u32 = 0x5345474D; // "SEGM"
    pub const VERSION: u32 = 1;
    
    pub fn new(start_slot: u64) -> Self {
        Self {
            magic: Self::MAGIC,
            version: Self::VERSION,
            start_slot,
            end_slot: 0,
            entry_count: 0,
            data_size: 0,
            checksum: 0,
        }
    }
    
    pub fn validate(&self) -> Result<(), SegmentError> {
        if self.magic != Self::MAGIC {
            return Err(SegmentError::InvalidMagic(self.magic));
        }
        if self.version != Self::VERSION {
            return Err(SegmentError::UnsupportedVersion(self.version));
        }
        Ok(())
    }
}

/// Single delta entry
#[repr(C)]
pub struct DeltaEntry {
    /// Entry size (including this header)
    pub size: u32,
    /// Slot number
    pub slot: u64,
    /// Write version
    pub write_version: u64,
    /// Pubkey (32 bytes)
    pub pubkey: [u8; 32],
    /// Account data length
    pub data_len: u32,
    /// Reserved
    _reserved: u32,
}

/// Segment writer for appending delta entries
#[derive(Debug)]
pub struct SegmentWriter {
    file: BufWriter<File>,
    header: SegmentHeader,
    current_pos: u64,
}

impl SegmentWriter {
    /// Create a new segment file
    pub fn create<P: AsRef<Path>>(path: P, start_slot: u64) -> Result<Self, SegmentError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        
        let header = SegmentHeader::new(start_slot);
        let mut writer = Self {
            file: BufWriter::new(file),
            header,
            current_pos: std::mem::size_of::<SegmentHeader>() as u64,
        };
        
        // Write initial header
        writer.write_header()?;
        
        Ok(writer)
    }
    
    /// Append an account update to the segment
    pub fn append(&mut self, slot: u64, write_version: u64, pubkey: &[u8; 32], data: &[u8]) -> Result<u64, SegmentError> {
        let entry = DeltaEntry {
            size: (std::mem::size_of::<DeltaEntry>() + data.len()) as u32,
            slot,
            write_version,
            pubkey: *pubkey,
            data_len: data.len() as u32,
            _reserved: 0,
        };
        
        let offset = self.current_pos;
        
        // Write entry header
        let entry_bytes = unsafe {
            std::slice::from_raw_parts(
                &entry as *const DeltaEntry as *const u8,
                std::mem::size_of::<DeltaEntry>(),
            )
        };
        self.file.write_all(entry_bytes)?;
        
        // Write account data
        self.file.write_all(data)?;
        
        self.file.flush()?;
        
        // Update position and header stats
        self.current_pos += entry.size as u64;
        self.header.entry_count += 1;
        self.header.data_size += entry.size as u64;
        self.header.end_slot = self.header.end_slot.max(slot);
        
        Ok(offset)
    }
    
    /// Flush and finalize segment
    pub fn finalize(mut self) -> Result<SegmentInfo, SegmentError> {
        // Update header with final stats
        self.write_header()?;
        self.file.flush()?;
        
        Ok(SegmentInfo {
            start_slot: self.header.start_slot,
            end_slot: self.header.end_slot,
            entry_count: self.header.entry_count,
            data_size: self.header.data_size,
        })
    }
    
    fn write_header(&mut self) -> Result<(), SegmentError> {
        // Calculate checksum
        let header_bytes = unsafe {
            std::slice::from_raw_parts(
                &self.header as *const SegmentHeader as *const u8,
                std::mem::size_of::<SegmentHeader>() - 4, // Exclude checksum field
            )
        };
        self.header.checksum = crc32fast::hash(header_bytes);
        
        // Write header at beginning of file
        self.file.seek(SeekFrom::Start(0))?;
        let header_bytes = unsafe {
            std::slice::from_raw_parts(
                &self.header as *const SegmentHeader as *const u8,
                std::mem::size_of::<SegmentHeader>(),
            )
        };
        self.file.write_all(header_bytes)?;
        
        Ok(())
    }
}

/// Segment metadata for indexing
#[derive(Debug, Clone)]
pub struct SegmentInfo {
    pub start_slot: u64,
    pub end_slot: u64,
    pub entry_count: u64,
    pub data_size: u64,
}

/// Segment reader for reading delta entries
pub struct SegmentReader {
    #[allow(dead_code)]
    file: File,
    header: SegmentHeader,
}

impl SegmentReader {
    /// Open existing segment for reading
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, SegmentError> {
        let file = File::open(path)?;
        let mut reader = std::io::BufReader::new(&file);
        
        // Read header
        let mut header = SegmentHeader {
            magic: 0,
            version: 0,
            start_slot: 0,
            end_slot: 0,
            entry_count: 0,
            data_size: 0,
            checksum: 0,
        };
        
        let header_bytes = unsafe {
            std::slice::from_raw_parts_mut(
                &mut header as *mut SegmentHeader as *mut u8,
                std::mem::size_of::<SegmentHeader>(),
            )
        };
        reader.read_exact(header_bytes)?;
        
        // Validate header
        header.validate()?;
        
        Ok(Self { file, header })
    }
    
    /// Get segment metadata
    pub fn info(&self) -> SegmentInfo {
        SegmentInfo {
            start_slot: self.header.start_slot,
            end_slot: self.header.end_slot,
            entry_count: self.header.entry_count,
            data_size: self.header.data_size,
        }
    }
}

/// Segment errors
#[derive(Debug, thiserror::Error)]
pub enum SegmentError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Invalid magic number: {0:#X}")]
    InvalidMagic(u32),
    
    #[error("Unsupported segment version: {0}")]
    UnsupportedVersion(u32),
    
    #[error("Corrupted segment: checksum mismatch")]
    ChecksumMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    
    #[test]
    fn test_segment_write_read() {
        let path = "/tmp/test_segment.bin";
        let _ = fs::remove_file(path);
        
        // Write segment
        {
            let mut writer = SegmentWriter::create(path, 100).unwrap();
            
            let pubkey = [1u8; 32];
            let data = b"test account data";
            
            writer.append(100, 1, &pubkey, data).unwrap();
            writer.append(101, 2, &pubkey, data).unwrap();
            
            let info = writer.finalize().unwrap();
            assert_eq!(info.entry_count, 2);
            assert_eq!(info.start_slot, 100);
            assert_eq!(info.end_slot, 101);
        }
        
        // Read segment
        {
            let reader = SegmentReader::open(path).unwrap();
            let info = reader.info();
            
            assert_eq!(info.entry_count, 2);
            assert_eq!(info.start_slot, 100);
            assert_eq!(info.end_slot, 101);
        }
        
        // Cleanup
        let _ = fs::remove_file(path);
    }
}
