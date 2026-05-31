//! Shared memory ring buffer for Geyser → Hyperplane communication
//!
//! Lock-free single-producer multi-consumer (SPMC) ring buffer using mmap.

use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::ptr;
use memmap2::MmapMut;

/// Ring buffer header (cache-line aligned)
#[repr(C, align(64))]
pub struct RingBufferHeader {
    /// Write position (producer only)
    pub write_pos: AtomicU64,
    /// Read position (consumer aggregates)
    pub read_pos: AtomicU64,
    /// Buffer capacity in bytes
    pub capacity: u64,
    /// Number of entries written
    pub entry_count: AtomicU64,
    /// Ring buffer state
    pub state: AtomicU8,
    /// Padding to 64 bytes
    _padding: [u8; 47],
}

/// Ring buffer state
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingBufferState {
    Uninitialized = 0,
    Active = 1,
    Draining = 2,
    Stopped = 3,
}

/// Single entry in the ring buffer
#[repr(C)]
pub struct RingEntry {
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
    pub _reserved: u32,
}

impl RingEntry {
    /// Get size of entry header (without data)
    pub const fn header_size() -> usize {
        std::mem::size_of::<Self>()
    }
    
    /// Get total size including data
    pub fn total_size(&self) -> usize {
        Self::header_size() + self.data_len as usize
    }
}

/// Ring buffer writer (producer)
#[derive(Debug)]
pub struct RingBufferWriter {
    mmap: MmapMut,
    header: *mut RingBufferHeader,
    capacity: usize,
}

unsafe impl Send for RingBufferWriter {}
unsafe impl Sync for RingBufferWriter {}

impl RingBufferWriter {
    /// Create a new ring buffer with given capacity
    pub fn create(path: &str, capacity: usize) -> Result<Self, RingBufferError> {
        use std::fs::OpenOptions;
        
        // Create and mmap file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        
        file.set_len(capacity as u64)?;
        
        let mut mmap = unsafe { MmapMut::map_mut(&file)? };
        
        // Initialize header
        let header_ptr = mmap.as_mut_ptr() as *mut RingBufferHeader;
        unsafe {
            ptr::write(
                header_ptr,
                RingBufferHeader {
                    write_pos: AtomicU64::new(0),
                    read_pos: AtomicU64::new(0),
                    capacity: capacity as u64,
                    entry_count: AtomicU64::new(0),
                    state: AtomicU8::new(RingBufferState::Active as u8),
                    _padding: [0; 47],
                },
            );
        }
        
        Ok(Self {
            mmap,
            header: header_ptr,
            capacity,
        })
    }
    
    /// Open existing ring buffer
    pub fn open(path: &str) -> Result<Self, RingBufferError> {
        use std::fs::OpenOptions;
        
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;
        
        let mut mmap = unsafe { MmapMut::map_mut(&file)? };
        
        if mmap.len() < std::mem::size_of::<RingBufferHeader>() {
            return Err(RingBufferError::InvalidSize);
        }
        
        let header_ptr = mmap.as_mut_ptr() as *mut RingBufferHeader;
        let capacity = unsafe { (*header_ptr).capacity as usize };
        
        Ok(Self {
            mmap,
            header: header_ptr,
            capacity,
        })
    }
    
    /// Write an entry to the ring buffer
    pub fn write(&self, slot: u64, write_version: u64, pubkey: &[u8; 32], data: &[u8]) -> Result<u64, RingBufferError> {
        let header = unsafe { &*self.header };
        
        // Check state
        if header.state.load(Ordering::Relaxed) != RingBufferState::Active as u8 {
            return Err(RingBufferError::NotActive);
        }
        
        // Calculate entry size (header + data)
        let entry_size = (std::mem::size_of::<RingEntry>() + data.len()) as u32;
        
        // Get current write position
        let write_pos = header.write_pos.load(Ordering::Relaxed);
        let new_pos = write_pos + entry_size as u64;
        
        // Check if we have space (simple wrap-around not implemented yet)
        if new_pos > self.capacity as u64 {
            return Err(RingBufferError::BufferFull);
        }
        
        // Write entry
        let entry_ptr = unsafe {
            (self.header as *mut u8).add(std::mem::size_of::<RingBufferHeader>())
                .add(write_pos as usize) as *mut RingEntry
        };
        
        unsafe {
            ptr::write(
                entry_ptr,
                RingEntry {
                    size: entry_size,
                    slot,
                    write_version,
                    pubkey: *pubkey,
                    data_len: data.len() as u32,
                    _reserved: 0,
                },
            );
            
            let data_ptr = (entry_ptr as *mut u8).add(RingEntry::header_size());
            ptr::copy_nonoverlapping(data.as_ptr(), data_ptr, data.len());
        }
        
        // Memory barrier and update write position
        header.write_pos.store(new_pos, Ordering::Release);
        header.entry_count.fetch_add(1, Ordering::Relaxed);
        
        Ok(write_pos)
    }
    
    /// Get number of entries written
    pub fn entry_count(&self) -> u64 {
        let header = unsafe { &*self.header };
        header.entry_count.load(Ordering::Relaxed)
    }
    
    /// Close and stop the ring buffer
    pub fn close(&self) {
        let header = unsafe { &*self.header };
        header.state.store(RingBufferState::Stopped as u8, Ordering::Release);
    }
}

/// Ring buffer reader (consumer)
pub struct RingBufferReader {
    mmap: MmapMut,
    header: *const RingBufferHeader,
    read_pos: u64,
}

unsafe impl Send for RingBufferReader {}
unsafe impl Sync for RingBufferReader {}

impl RingBufferReader {
    /// Open ring buffer for reading
    pub fn open(path: &str) -> Result<Self, RingBufferError> {
        use std::fs::OpenOptions;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;
        
        let mmap = unsafe { MmapMut::map_mut(&file)? };
        
        if mmap.len() < std::mem::size_of::<RingBufferHeader>() {
            return Err(RingBufferError::InvalidSize);
        }
        
        let header_ptr = mmap.as_ptr() as *const RingBufferHeader;
        
        Ok(Self {
            mmap,
            header: header_ptr,
            read_pos: 0,
        })
    }
    
    /// Read next entry
    pub fn read_next(&mut self) -> Result<Option<RingEntryView<'_>>, RingBufferError> {
        let header = unsafe { &*self.header };
        
        let write_pos = header.write_pos.load(Ordering::Acquire);
        
        if self.read_pos >= write_pos {
            return Ok(None); // No new data
        }
        
        let entry_ptr = unsafe {
            (self.header as *const u8).add(std::mem::size_of::<RingBufferHeader>())
                .add(self.read_pos as usize) as *const RingEntry
        };
        
        let entry = unsafe { &*entry_ptr };
        
        // Validate entry
        if entry.size == 0 || self.read_pos + entry.size as u64 > write_pos {
            return Err(RingBufferError::CorruptedEntry);
        }
        
        let data_ptr = unsafe { (entry_ptr as *const u8).add(RingEntry::header_size()) };
        
        let data = unsafe {
            std::slice::from_raw_parts(data_ptr, entry.data_len as usize)
        };
        
        let view = RingEntryView {
            slot: entry.slot,
            write_version: entry.write_version,
            pubkey: &entry.pubkey,
            data,
        };
        
        self.read_pos += entry.size as u64;
        
        Ok(Some(view))
    }
    
    /// Update read position
    pub fn advance(&mut self, pos: u64) {
        self.read_pos = pos;
        
        // Update header read position
        let header = unsafe { &*self.header };
        header.read_pos.store(pos, Ordering::Release);
    }
}

/// View of a ring buffer entry (zero-copy)
pub struct RingEntryView<'a> {
    pub slot: u64,
    pub write_version: u64,
    pub pubkey: &'a [u8; 32],
    pub data: &'a [u8],
}

/// Ring buffer errors
#[derive(Debug, thiserror::Error)]
pub enum RingBufferError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Ring buffer not active")]
    NotActive,
    
    #[error("Ring buffer full")]
    BufferFull,
    
    #[error("Invalid ring buffer size")]
    InvalidSize,
    
    #[error("Corrupted entry")]
    CorruptedEntry,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    
    #[test]
    fn test_ring_buffer_write_read() {
        let path = "/tmp/test_ring_buffer.bin";
        let _ = fs::remove_file(path);
        
        // Create writer
        let writer = RingBufferWriter::create(path, 1024 * 1024).unwrap();
        
        // Write entry
        let pubkey = [1u8; 32];
        let data = b"test account data";
        writer.write(100, 1, &pubkey, data).unwrap();
        
        assert_eq!(writer.entry_count(), 1);
        
        // Read entry
        let mut reader = RingBufferReader::open(path).unwrap();
        let entry = reader.read_next().unwrap().unwrap();
        
        assert_eq!(entry.slot, 100);
        assert_eq!(entry.write_version, 1);
        assert_eq!(entry.pubkey, &pubkey);
        assert_eq!(entry.data, data);
        
        // Cleanup
        drop(writer);
        drop(reader);
        let _ = fs::remove_file(path);
    }
}
