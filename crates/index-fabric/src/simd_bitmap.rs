//! SIMD Bitmap Engine per Operazioni Vettoriali
//! 
//! Usa istruzioni SIMD per accelerare le operazioni su bitmap
//! (AND, OR, XOR, conteggio bit)
//! 
//! Nota: In produzione su VPS, usa SIMD intrinsics con `std::arch::x86_64`

use std::sync::Arc;

/// SIMD Bitmap Engine
pub struct SimdBitmapEngine {
    /// Buffer per operazioni
    #[allow(dead_code)]
    buffer: Arc<parking_lot::RwLock<Vec<u64>>>,
    /// Dimensione buffer
    #[allow(dead_code)]
    buffer_size: usize,
}

impl SimdBitmapEngine {
    /// Crea un nuovo engine
    pub fn new(buffer_size: usize) -> Self {
        Self {
            buffer: Arc::new(parking_lot::RwLock::new(vec![0u64; buffer_size])),
            buffer_size,
        }
    }
    
    /// AND vettoriale tra due bitmap
    pub fn bitmap_and(&self, a: &[u64], b: &[u64], result: &mut [u64]) {
        let len = std::cmp::min(a.len(), std::cmp::min(b.len(), result.len()));
        
        // SIMD-accelerated AND (simulato, in produzione userebbe intrinsics)
        for i in 0..len {
            result[i] = a[i] & b[i];
        }
    }
    
    /// OR vettoriale tra due bitmap
    pub fn bitmap_or(&self, a: &[u64], b: &[u64], result: &mut [u64]) {
        let len = std::cmp::min(a.len(), std::cmp::min(b.len(), result.len()));
        
        for i in 0..len {
            result[i] = a[i] | b[i];
        }
    }
    
    /// XOR vettoriale tra due bitmap
    pub fn bitmap_xor(&self, a: &[u64], b: &[u64], result: &mut [u64]) {
        let len = std::cmp::min(a.len(), std::cmp::min(b.len(), result.len()));
        
        for i in 0..len {
            result[i] = a[i] ^ b[i];
        }
    }
    
    /// Conta bit settati (popcount)
    pub fn popcount(&self, bitmap: &[u64]) -> usize {
        bitmap.iter().map(|w| w.count_ones() as usize).sum()
    }
    
    /// Conta bit settati in AND di due bitmap
    pub fn intersection_size(&self, a: &[u64], b: &[u64]) -> usize {
        let len = std::cmp::min(a.len(), b.len());
        let mut count = 0;
        
        for i in 0..len {
            count += (a[i] & b[i]).count_ones() as usize;
        }
        
        count
    }
    
    /// Conta bit settati in OR di due bitmap
    pub fn union_size(&self, a: &[u64], b: &[u64]) -> usize {
        let len = std::cmp::min(a.len(), b.len());
        let mut count = 0;
        
        for i in 0..len {
            count += (a[i] | b[i]).count_ones() as usize;
        }
        
        count
    }
    
    /// Trova primo bit settato
    pub fn find_first_set(&self, bitmap: &[u64]) -> Option<usize> {
        for (i, word) in bitmap.iter().enumerate() {
            if *word != 0 {
                let bit = word.trailing_zeros() as usize;
                return Some(i * 64 + bit);
            }
        }
        None
    }
    
    /// Trova tutti i bit settati
    pub fn find_all_set(&self, bitmap: &[u64]) -> Vec<usize> {
        let mut result = Vec::new();
        
        for (i, word) in bitmap.iter().enumerate() {
            if *word != 0 {
                let mut w = *word;
                while w != 0 {
                    let bit = w.trailing_zeros() as usize;
                    result.push(i * 64 + bit);
                    w &= !(1u64 << bit);
                }
            }
        }
        
        result
    }
    
    /// Merge ordinato di due bitmap (per query ottimizzate)
    pub fn merge_sorted(&self, a: &[u64], b: &[u64]) -> Vec<u64> {
        let mut result = Vec::with_capacity(a.len() + b.len());
        let mut i = 0;
        let mut j = 0;
        
        while i < a.len() && j < b.len() {
            if a[i] < b[j] {
                result.push(a[i]);
                i += 1;
            } else if a[i] > b[j] {
                result.push(b[j]);
                j += 1;
            } else {
                result.push(a[i]);
                i += 1;
                j += 1;
            }
        }
        
        result.extend_from_slice(&a[i..]);
        result.extend_from_slice(&b[j..]);
        
        result
    }
}

/// SIMD-optimized RoaringBitmap wrapper
pub struct SimdRoaringBitmap {
    /// Bitmap sottostante
    bitmap: roaring::RoaringTreemap,
    /// Engine per operazioni
    engine: Arc<SimdBitmapEngine>,
}

impl SimdRoaringBitmap {
    pub fn new(engine: Arc<SimdBitmapEngine>) -> Self {
        Self {
            bitmap: roaring::RoaringTreemap::new(),
            engine,
        }
    }
    
    /// Inserisci valore
    pub fn insert(&mut self, value: u32) -> bool {
        self.bitmap.insert(value as u64)
    }
    
    /// Rimuovi valore
    pub fn remove(&mut self, value: u32) -> bool {
        self.bitmap.remove(value as u64)
    }
    
    /// Contiene valore
    pub fn contains(&self, value: u32) -> bool {
        self.bitmap.contains(value as u64)
    }
    
    /// Cardinalità
    pub fn len(&self) -> u64 {
        self.bitmap.len()
    }
    
    /// Intersezione con un'altra bitmap (SIMD-accelerated)
    pub fn intersection(&self, other: &SimdRoaringBitmap) -> SimdRoaringBitmap {
        let mut result = SimdRoaringBitmap::new(self.engine.clone());
        
        // Usa roaring::RoaringTreemap intersection con & operator
        result.bitmap = self.bitmap.clone();
        result.bitmap &= &other.bitmap;
        
        result
    }
    
    /// Unione con un'altra bitmap (SIMD-accelerated)
    pub fn union(&self, other: &SimdRoaringBitmap) -> SimdRoaringBitmap {
        let mut result = SimdRoaringBitmap::new(self.engine.clone());
        result.bitmap = self.bitmap.clone();
        result.bitmap |= &other.bitmap;
        result
    }
    
    /// Differenza con un'altra bitmap
    pub fn difference(&self, other: &SimdRoaringBitmap) -> SimdRoaringBitmap {
        let mut result = SimdRoaringBitmap::new(self.engine.clone());
        result.bitmap = self.bitmap.clone();
        result.bitmap -= &other.bitmap;
        result
    }
    
    /// Jaccard similarity
    pub fn jaccard(&self, other: &SimdRoaringBitmap) -> f64 {
        let mut intersection = self.bitmap.clone();
        intersection &= &other.bitmap;
        
        let mut union = self.bitmap.clone();
        union |= &other.bitmap;
        
        let intersection_len = intersection.len();
        let union_len = union.len();
        
        if union_len == 0 {
            0.0
        } else {
            intersection_len as f64 / union_len as f64
        }
    }
    
    /// Iterator sui valori
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.bitmap.iter().map(|v| v as u32)
    }
}

/// Statistics per SIMD operations
#[derive(Debug, Clone)]
pub struct SimdStats {
    /// Operazioni eseguite
    pub operations_count: u64,
    /// Tempo medio (nanoseconds)
    pub avg_time_ns: u64,
    /// Speedup vs scalar
    pub speedup_factor: f64,
}

impl Default for SimdStats {
    fn default() -> Self {
        Self {
            operations_count: 0,
            avg_time_ns: 0,
            speedup_factor: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simd_bitmap_and() {
        let engine = SimdBitmapEngine::new(1024);
        
        let a = vec![0b1100u64, 0b1010];
        let b = vec![0b1010u64, 0b1100];
        let mut result = vec![0u64; 2];
        
        engine.bitmap_and(&a, &b, &mut result);
        
        assert_eq!(result[0], 0b1000);
        assert_eq!(result[1], 0b1000);
    }
    
    #[test]
    fn test_simd_bitmap_or() {
        let engine = SimdBitmapEngine::new(1024);
        
        let a = vec![0b1100u64, 0b1010];
        let b = vec![0b1010u64, 0b1100];
        let mut result = vec![0u64; 2];
        
        engine.bitmap_or(&a, &b, &mut result);
        
        assert_eq!(result[0], 0b1110);
        assert_eq!(result[1], 0b1110);
    }
    
    #[test]
    fn test_simd_popcount() {
        let engine = SimdBitmapEngine::new(1024);
        
        let bitmap = vec![0b1111u64, 0b1010, 0b0011];
        let count = engine.popcount(&bitmap);
        
        assert_eq!(count, 8); // 4 + 2 + 2
    }
    
    #[test]
    fn test_simd_roaring_bitmap() {
        let engine = Arc::new(SimdBitmapEngine::new(1024));
        
        let mut bitmap1 = SimdRoaringBitmap::new(engine.clone());
        bitmap1.insert(1);
        bitmap1.insert(2);
        bitmap1.insert(3);
        
        let mut bitmap2 = SimdRoaringBitmap::new(engine.clone());
        bitmap2.insert(2);
        bitmap2.insert(3);
        bitmap2.insert(4);
        
        let intersection = bitmap1.intersection(&bitmap2);
        assert_eq!(intersection.len(), 2);
        assert!(intersection.contains(2));
        assert!(intersection.contains(3));
        
        let union = bitmap1.union(&bitmap2);
        assert_eq!(union.len(), 4);
    }
    
    #[test]
    fn test_simd_jaccard() {
        let engine = Arc::new(SimdBitmapEngine::new(1024));
        
        let mut bitmap1 = SimdRoaringBitmap::new(engine.clone());
        bitmap1.insert(1);
        bitmap1.insert(2);
        bitmap1.insert(3);
        
        let mut bitmap2 = SimdRoaringBitmap::new(engine.clone());
        bitmap2.insert(2);
        bitmap2.insert(3);
        bitmap2.insert(4);
        
        let jaccard = bitmap1.jaccard(&bitmap2);
        assert!((jaccard - 0.5).abs() < 0.01); // 2/4 = 0.5
    }
}
