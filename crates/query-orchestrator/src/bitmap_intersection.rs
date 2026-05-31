//! Bitmap Intersection Engine
//!
//! Efficiently intersects multiple bitmap indexes for getProgramAccounts filters.

use std::sync::Arc;
use hyperplane_types::PubkeyBitmap;
use solana_sdk::pubkey::Pubkey;

/// Bitmap intersection result
#[derive(Debug, Clone)]
pub struct IntersectionResult {
    /// Intersected bitmap
    pub bitmap: PubkeyBitmap,
    /// Number of bitmaps intersected
    pub num_bitmaps: usize,
    /// Time taken in microseconds (estimated)
    pub estimated_time_us: u64,
}

/// Bitmap Intersection Engine
pub struct BitmapIntersectionEngine {
    /// Pre-allocated buffer for intermediate results
    buffer: Arc<PubkeyBitmap>,
}

impl BitmapIntersectionEngine {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(PubkeyBitmap::new()),
        }
    }
    
    /// Intersect multiple bitmaps
    pub fn intersect(&self, bitmaps: &[&PubkeyBitmap]) -> IntersectionResult {
        if bitmaps.is_empty() {
            return IntersectionResult {
                bitmap: PubkeyBitmap::new(),
                num_bitmaps: 0,
                estimated_time_us: 0,
            };
        }
        
        // Start with first bitmap
        let mut result = bitmaps[0].clone();
        
        // Intersect with remaining bitmaps
        for bitmap in &bitmaps[1..] {
            result = result.intersection(bitmap);
        }
        
        // Estimate time based on bitmap size and number of intersections
        let estimated_time_us = (result.len() * bitmaps.len() as u64) / 1000;
        
        IntersectionResult {
            bitmap: result,
            num_bitmaps: bitmaps.len(),
            estimated_time_us,
        }
    }
    
    /// Intersect two bitmaps (optimized)
    pub fn intersect_two(&self, a: &PubkeyBitmap, b: &PubkeyBitmap) -> PubkeyBitmap {
        a.intersection(b)
    }
    
    /// Union multiple bitmaps (OR operation)
    pub fn union(&self, bitmaps: &[&PubkeyBitmap]) -> PubkeyBitmap {
        if bitmaps.is_empty() {
            return PubkeyBitmap::new();
        }
        
        let mut result = bitmaps[0].clone();
        for bitmap in &bitmaps[1..] {
            result = result.union(bitmap);
        }
        result
    }
    
    /// Difference (NOT operation)
    pub fn difference(&self, a: &PubkeyBitmap, b: &PubkeyBitmap) -> PubkeyBitmap {
        a.difference(b)
    }
}

impl Default for BitmapIntersectionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bitmap_intersection_basic() {
        let engine = BitmapIntersectionEngine::new();
        
        let mut bitmap1 = PubkeyBitmap::new();
        bitmap1.insert(1);
        bitmap1.insert(2);
        bitmap1.insert(3);
        
        let mut bitmap2 = PubkeyBitmap::new();
        bitmap2.insert(2);
        bitmap2.insert(3);
        bitmap2.insert(4);
        
        let result = engine.intersect(&[&bitmap1, &bitmap2]);
        
        assert_eq!(result.bitmap.len(), 2);
        assert!(result.bitmap.contains(2));
        assert!(result.bitmap.contains(3));
        assert_eq!(result.num_bitmaps, 2);
    }
    
    #[test]
    fn test_bitmap_union_basic() {
        let engine = BitmapIntersectionEngine::new();
        
        let mut bitmap1 = PubkeyBitmap::new();
        bitmap1.insert(1);
        bitmap1.insert(2);
        
        let mut bitmap2 = PubkeyBitmap::new();
        bitmap2.insert(3);
        bitmap2.insert(4);
        
        let result = engine.union(&[&bitmap1, &bitmap2]);
        
        assert_eq!(result.len(), 4);
    }
}
