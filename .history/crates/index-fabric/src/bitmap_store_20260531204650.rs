//! Bitmap Store - Persistent storage for bitmap indexes
//!
//! Stores compressed RoaringBitmap indexes with fast random access.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::RwLock;
use hyperplane_types::PubkeyBitmap;

/// Bitmap store entry
#[derive(Debug, Clone)]
pub struct BitmapEntry {
    pub index_name: String,
    pub key: String,
    pub bitmap: PubkeyBitmap,
    pub last_updated_slot: u64,
}

/// Bitmap store state
#[derive(Debug, Default)]
pub struct BitmapStoreState {
    /// Map of (index_name, key) -> bitmap
    bitmaps: BTreeMap<(String, String), PubkeyBitmap>,
    /// Total bitmaps stored
    total_bitmaps: u64,
    /// Total compressed size in bytes
    compressed_size_bytes: u64,
}

/// Bitmap Store for persistent bitmap indexes
pub struct BitmapStore {
    state: Arc<RwLock<BitmapStoreState>>,
    #[allow(dead_code)]
    store_path: PathBuf,
}

impl BitmapStore {
    pub fn new(store_path: PathBuf) -> Self {
        Self {
            state: Arc::new(RwLock::new(BitmapStoreState::default())),
            store_path,
        }
    }
    
    pub fn store_bitmap(&self, index_name: &str, key: &str, bitmap: &PubkeyBitmap, _slot: u64) {
        let mut state = self.state.write();
        
        let map_key = (index_name.to_string(), key.to_string());
        let compressed_size = bitmap.compressed_size() as u64;
        
        state.bitmaps.insert(map_key, bitmap.clone());
        state.total_bitmaps = state.bitmaps.len() as u64;
        state.compressed_size_bytes = state.compressed_size_bytes.saturating_add(compressed_size);
    }
    
    pub fn get_bitmap(&self, index_name: &str, key: &str) -> Option<PubkeyBitmap> {
        let state = self.state.read();
        state.bitmaps.get(&(index_name.to_string(), key.to_string())).cloned()
    }
    
    pub fn remove_bitmap(&self, index_name: &str, key: &str) {
        let mut state = self.state.write();
        if let Some(bitmap) = state.bitmaps.remove(&(index_name.to_string(), key.to_string())) {
            state.compressed_size_bytes = state.compressed_size_bytes.saturating_sub(bitmap.compressed_size() as u64);
            state.total_bitmaps = state.bitmaps.len() as u64;
        }
    }
    
    pub fn stats(&self) -> BitmapStoreStats {
        let state = self.state.read();
        BitmapStoreStats {
            total_bitmaps: state.total_bitmaps,
            compressed_size_bytes: state.compressed_size_bytes,
        }
    }
}

/// Bitmap store statistics
#[derive(Debug, Clone, Default)]
pub struct BitmapStoreStats {
    pub total_bitmaps: u64,
    pub compressed_size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bitmap_store_basic() {
        let store = BitmapStore::new(PathBuf::from("/tmp/test_bitmap_store"));
        
        let mut bitmap = PubkeyBitmap::new();
        bitmap.insert(1);
        bitmap.insert(2);
        bitmap.insert(3);
        
        store.store_bitmap("program_index", "program1", &bitmap, 100);
        
        let retrieved = store.get_bitmap("program_index", "program1").unwrap();
        assert_eq!(retrieved.len(), 3);
        
        let stats = store.stats();
        assert_eq!(stats.total_bitmaps, 1);
    }
}
