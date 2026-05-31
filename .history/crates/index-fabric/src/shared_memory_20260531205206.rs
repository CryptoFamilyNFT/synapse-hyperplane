//! Shared Memory API per Zero-Copy Query Results
//! 
//! Permette di condividere i risultati delle query tra processi
//! senza copiare i dati, usando memory-mapped files.

use std::path::PathBuf;
use std::sync::Arc;
use memmap2::MmapMut;
use std::fs::OpenOptions;

/// Magic number per validare shared memory
const SHARED_MEM_MAGIC: u32 = 0x53484D45; // "SHME"

/// Versione del formato
const SHARED_MEM_VERSION: u32 = 1;

/// Header della shared memory (64 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SharedMemHeader {
    /// Magic number
    pub magic: u32,
    /// Versione formato
    pub version: u32,
    /// Numero di entries
    pub entry_count: u64,
    /// Offset dati
    pub data_offset: u64,
    /// Dimensione totale
    pub total_size: u64,
    /// Timestamp creazione
    pub created_at: u64,
    /// Slot corrente
    pub slot: u64,
    /// Reserved
    pub reserved: [u64; 4],
}

impl Default for SharedMemHeader {
    fn default() -> Self {
        Self {
            magic: SHARED_MEM_MAGIC,
            version: SHARED_MEM_VERSION,
            entry_count: 0,
            data_offset: 64, // Header size
            total_size: 64,
            created_at: 0,
            slot: 0,
            reserved: [0; 4],
        }
    }
}

/// Shared Memory Region per query results
pub struct SharedMemoryRegion {
    /// Memory-mapped file
    mmap: MmapMut,
    /// Path del file
    #[allow(dead_code)]
    file_path: PathBuf,
    /// Header
    header: SharedMemHeader,
}

impl SharedMemoryRegion {
    /// Crea una nuova shared memory region
    pub fn create(file_path: PathBuf, capacity: usize) -> std::io::Result<Self> {
        // Crea file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&file_path)?;
        
        // Set dimensione
        file.set_len(capacity as u64)?;
        
        // Mappa in memoria
        let mut mmap = unsafe { MmapMut::map_mut(&file)? };
        
        // Inizializza header
        let header = SharedMemHeader {
            total_size: capacity as u64,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            ..Default::default()
        };
        
        // Scrivi header
        unsafe {
            let header_bytes = std::slice::from_raw_parts_mut(
                mmap.as_mut_ptr(),
                std::mem::size_of::<SharedMemHeader>(),
            );
            let header_ptr = &header as *const SharedMemHeader as *const u8;
            header_bytes.copy_from_slice(std::slice::from_raw_parts(header_ptr, std::mem::size_of::<SharedMemHeader>()));
        }
        
        Ok(Self {
            mmap,
            file_path,
            header,
        })
    }
    
    /// Apre una shared memory region esistente
    pub fn open(file_path: PathBuf) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&file_path)?;
        
        let mmap = unsafe { MmapMut::map_mut(&file)? };
        
        // Leggi header
        let header_bytes = &mmap[..std::mem::size_of::<SharedMemHeader>()];
        let header = unsafe {
            std::ptr::read_unaligned(header_bytes.as_ptr() as *const SharedMemHeader)
        };
        
        // Validata
        if header.magic != SHARED_MEM_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid shared memory magic",
            ));
        }
        
        Ok(Self {
            mmap,
            file_path,
            header,
        })
    }
    
    /// Scrivi dati nella shared memory
    pub fn write_data(&mut self, offset: usize, data: &[u8]) -> std::io::Result<()> {
        let write_offset = self.header.data_offset as usize + offset;
        if write_offset + data.len() > self.mmap.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "Shared memory capacity exceeded",
            ));
        }
        
        self.mmap[write_offset..write_offset + data.len()].copy_from_slice(data);
        self.mmap.flush()?;
        Ok(())
    }
    
    /// Leggi dati dalla shared memory
    pub fn read_data(&self, offset: usize, len: usize) -> &[u8] {
        let read_offset = self.header.data_offset as usize + offset;
        &self.mmap[read_offset..read_offset + len]
    }
    
    /// Aggiorna header
    pub fn update_header(&mut self, entry_count: u64, slot: u64) {
        self.header.entry_count = entry_count;
        self.header.slot = slot;
        
        // Scrivi header aggiornato
        unsafe {
            let header_bytes = std::slice::from_raw_parts_mut(
                self.mmap.as_mut_ptr(),
                std::mem::size_of::<SharedMemHeader>(),
            );
            let header_ptr = &self.header as *const SharedMemHeader as *const u8;
            header_bytes.copy_from_slice(std::slice::from_raw_parts(header_ptr, std::mem::size_of::<SharedMemHeader>()));
        }
    }
    
    /// Ottieni capacity disponibile
    pub fn available_capacity(&self) -> usize {
        self.mmap.len() - self.header.data_offset as usize
    }
    
    /// Ottieni entry count
    pub fn entry_count(&self) -> u64 {
        self.header.entry_count
    }
    
    /// Ottieni slot
    pub fn slot(&self) -> u64 {
        self.header.slot
    }
}

/// Shared Memory Manager (gestisce multiple regions)
pub struct SharedMemoryManager {
    /// Regions attive (path)
    regions: Arc<parking_lot::RwLock<std::collections::HashMap<String, PathBuf>>>,
    /// Directory base
    base_dir: PathBuf,
}

impl SharedMemoryManager {
    pub fn new(base_dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&base_dir)?;
        
        Ok(Self {
            regions: Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new())),
            base_dir,
        })
    }
    
    /// Crea o ottieni una shared memory region
    pub fn get_or_create_region(
        &self,
        name: &str,
        capacity: usize,
    ) -> std::io::Result<SharedMemoryRegion> {
        {
            let regions = self.regions.read();
            if let Some(file_path) = regions.get(name) {
                return SharedMemoryRegion::open(file_path.clone());
            }
        }
        
        let file_path = self.base_dir.join(format!("{}.shm", name));
        let region = SharedMemoryRegion::create(file_path.clone(), capacity)?;
        
        let mut regions = self.regions.write();
        regions.insert(name.to_string(), file_path);
        
        Ok(region)
    }
    
    /// Rimuovi una region
    pub fn remove_region(&self, name: &str) -> std::io::Result<()> {
        let mut regions = self.regions.write();
        if let Some(file_path) = regions.remove(name) {
            std::fs::remove_file(&file_path)?;
        }
        Ok(())
    }
    
    /// Cleanup di tutte le regions
    pub fn cleanup(&self) -> std::io::Result<()> {
        let mut regions = self.regions.write();
        for (_, file_path) in regions.drain() {
            let _ = std::fs::remove_file(&file_path);
        }
        Ok(())
    }
}

/// Zero-Copy Query Result Wrapper
pub struct ZeroCopyResult {
    /// Shared memory region
    region: SharedMemoryRegion,
    /// Offset inizio dati
    data_offset: usize,
    /// Dimensione dati
    data_size: usize,
}

impl ZeroCopyResult {
    pub fn new(
        region: SharedMemoryRegion,
        offset: usize,
        size: usize,
    ) -> Self {
        Self {
            region,
            data_offset: offset,
            data_size: size,
        }
    }
    
    /// Ottieni slice ai dati (zero-copy)
    pub fn as_slice(&self) -> &[u8] {
        self.region.read_data(self.data_offset, self.data_size)
    }
    
    /// Ottieni entry count
    pub fn len(&self) -> usize {
        self.data_size
    }
    
    /// Check se vuoto
    pub fn is_empty(&self) -> bool {
        self.data_size == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_shared_memory_create() {
        let temp_dir = std::env::temp_dir().join("shm_test");
        let manager = SharedMemoryManager::new(temp_dir).unwrap();
        
        let region = manager
            .get_or_create_region("test_query", 1024 * 1024)
            .unwrap();
        
        assert!(region.available_capacity() > 0);
        
        // Cleanup
        let _ = manager.cleanup();
    }
    
    #[test]
    fn test_shared_memory_write_read() {
        let temp_dir = std::env::temp_dir().join("shm_test_rw");
        let manager = SharedMemoryManager::new(temp_dir).unwrap();
        
        let mut region = manager
            .get_or_create_region("test_rw", 1024 * 1024)
            .unwrap();
        
        // Scrivi dati
        let test_data = b"Hello, Shared Memory!";
        region.write_data(0, test_data).unwrap();
        region.update_header(1, 1000);
        
        assert_eq!(region.entry_count(), 1);
        assert_eq!(region.slot(), 1000);
        
        // Cleanup
        drop(region);
        let _ = manager.cleanup();
    }
    
    #[test]
    fn test_zero_copy_result() {
        let temp_dir = std::env::temp_dir().join("shm_test_zc");
        let manager = SharedMemoryManager::new(temp_dir).unwrap();
        
        let mut region = manager
            .get_or_create_region("test_zc", 1024 * 1024)
            .unwrap();
        
        // Scrivi prima i dati
        let test_data = b"Zero-Copy Test Data";
        region.write_data(0, test_data).unwrap();
        
        let zero_copy = ZeroCopyResult::new(region, 0, test_data.len());
        let slice = zero_copy.as_slice();
        
        assert_eq!(slice, test_data);
        assert_eq!(zero_copy.len(), test_data.len());
        
        // Cleanup
        let _ = manager.cleanup();
    }
}
