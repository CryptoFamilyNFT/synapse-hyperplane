//! Slot Reconciler Roots
//!
//! Manages root slot tracking for compaction and pruning.

use solana_sdk::clock::Slot;
use std::sync::atomic::{AtomicU64, Ordering};

/// Root tracker - safe compaction point
pub struct RootTracker {
    root: AtomicU64,
}

impl RootTracker {
    pub fn new(initial_root: Slot) -> Self {
        Self {
            root: AtomicU64::new(initial_root),
        }
    }

    pub fn get_root(&self) -> Slot {
        self.root.load(Ordering::SeqCst)
    }

    pub fn update_root(&self, new_root: Slot) -> Slot {
        self.root.swap(new_root, Ordering::SeqCst)
    }

    pub fn advance_root(&self, delta: Slot) -> Slot {
        let current = self.root.load(Ordering::SeqCst);
        let new_root = current + delta;
        self.root.store(new_root, Ordering::SeqCst);
        new_root
    }
}
