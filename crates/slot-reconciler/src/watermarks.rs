//! Slot Reconciler Watermarks
//!
//! Tracks high watermarks for various operations.

use solana_sdk::clock::Slot;
use std::sync::atomic::{AtomicU64, Ordering};

/// Watermark tracker
pub struct WatermarkTracker {
    watermark: AtomicU64,
}

impl WatermarkTracker {
    pub fn new(initial: Slot) -> Self {
        Self {
            watermark: AtomicU64::new(initial),
        }
    }

    pub fn get(&self) -> Slot {
        self.watermark.load(Ordering::SeqCst)
    }

    pub fn update(&self, new_value: Slot) -> bool {
        loop {
            let current = self.watermark.load(Ordering::SeqCst);
            if new_value <= current {
                return false;
            }
            if self.watermark.compare_exchange(current, new_value, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                return true;
            }
        }
    }

    pub fn advance(&self) -> Slot {
        self.watermark.fetch_add(1, Ordering::SeqCst)
    }
}
