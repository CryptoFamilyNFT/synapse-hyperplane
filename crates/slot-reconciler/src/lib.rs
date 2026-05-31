//! Slot Reconciler - Commitment tracking and rollback handling
//!
//! Manages processed/confirmed/finalized views and handles slot rollbacks.
//! Tracks commitment levels and maintains consistent views across storage layers.

pub mod roots;
pub mod commitments;
pub mod rollback;
pub mod watermarks;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use parking_lot::RwLock;
use solana_sdk::clock::Slot;

/// Commitment levels in Solana
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommitmentLevel {
    /// Most recent slot, not yet confirmed
    Processed,
    /// Slot confirmed by 2/3 of cluster
    Confirmed,
    /// Slot finalized with supermajority
    Finalized,
}

/// Slot reconciler state
#[derive(Debug, Clone)]
pub struct SlotReconcilerState {
    /// Highest processed slot
    pub processed_slot: Slot,
    /// Highest confirmed slot
    pub confirmed_slot: Slot,
    /// Highest finalized slot
    pub finalized_slot: Slot,
    /// Root slot (safe to compact up to this point)
    pub root_slot: Slot,
    /// Total rollbacks handled
    pub rollback_count: u64,
}

/// Slot Reconciler - Manages commitment levels and rollbacks
pub struct SlotReconciler {
    state: Arc<RwLock<SlotReconcilerState>>,
    processed_slot: AtomicU64,
    confirmed_slot: AtomicU64,
    finalized_slot: AtomicU64,
    root_slot: AtomicU64,
}

impl SlotReconciler {
    /// Create a new slot reconciler
    pub fn new(root_slot: Slot) -> Self {
        let state = SlotReconcilerState {
            processed_slot: root_slot,
            confirmed_slot: root_slot,
            finalized_slot: root_slot,
            root_slot,
            rollback_count: 0,
        };

        Self {
            state: Arc::new(RwLock::new(state)),
            processed_slot: AtomicU64::new(root_slot),
            confirmed_slot: AtomicU64::new(root_slot),
            finalized_slot: AtomicU64::new(root_slot),
            root_slot: AtomicU64::new(root_slot),
        }
    }

    /// Update processed slot
    pub fn update_processed(&self, slot: Slot) {
        self.processed_slot.store(slot, Ordering::SeqCst);
        let mut state = self.state.write();
        state.processed_slot = slot;
    }

    /// Update confirmed slot
    pub fn update_confirmed(&self, slot: Slot) {
        self.confirmed_slot.store(slot, Ordering::SeqCst);
        let mut state = self.state.write();
        state.confirmed_slot = slot;
    }

    /// Update finalized slot
    pub fn update_finalized(&self, slot: Slot) {
        self.finalized_slot.store(slot, Ordering::SeqCst);
        let mut state = self.state.write();
        state.finalized_slot = slot;
        
        // Update root when finalized advances
        if slot > state.root_slot {
            state.root_slot = slot;
            self.root_slot.store(slot, Ordering::SeqCst);
        }
    }

    /// Get current root slot
    pub fn get_root(&self) -> Slot {
        self.root_slot.load(Ordering::SeqCst)
    }

    /// Get slot for commitment level
    pub fn get_slot(&self, level: CommitmentLevel) -> Slot {
        match level {
            CommitmentLevel::Processed => self.processed_slot.load(Ordering::SeqCst),
            CommitmentLevel::Confirmed => self.confirmed_slot.load(Ordering::SeqCst),
            CommitmentLevel::Finalized => self.finalized_slot.load(Ordering::SeqCst),
        }
    }

    /// Check if slot is finalized
    pub fn is_finalized(&self, slot: Slot) -> bool {
        slot <= self.finalized_slot.load(Ordering::SeqCst)
    }

    /// Check if slot is confirmed
    pub fn is_confirmed(&self, slot: Slot) -> bool {
        slot <= self.confirmed_slot.load(Ordering::SeqCst)
    }

    /// Handle rollback - revert to previous root
    pub fn handle_rollback(&self, new_root: Slot) {
        let mut state = self.state.write();
        state.rollback_count += 1;
        state.root_slot = new_root;
        self.root_slot.store(new_root, Ordering::SeqCst);

        // Ensure finalized doesn't exceed new root
        let finalized = self.finalized_slot.load(Ordering::SeqCst);
        if finalized > new_root {
            self.finalized_slot.store(new_root, Ordering::SeqCst);
            state.finalized_slot = new_root;
        }

        tracing::warn!("Rollback handled: new_root={}, rollbacks={}", 
            new_root, state.rollback_count);
    }

    /// Get reconciler statistics
    pub fn stats(&self) -> SlotReconcilerState {
        self.state.read().clone()
    }

    /// Get rollback count
    pub fn rollback_count(&self) -> u64 {
        self.state.read().rollback_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_reconciler_basic() {
        let reconciler = SlotReconciler::new(100);

        assert_eq!(reconciler.get_root(), 100);
        assert_eq!(reconciler.get_slot(CommitmentLevel::Processed), 100);
        assert_eq!(reconciler.get_slot(CommitmentLevel::Confirmed), 100);
        assert_eq!(reconciler.get_slot(CommitmentLevel::Finalized), 100);

        // Update slots
        reconciler.update_processed(101);
        reconciler.update_confirmed(100);
        reconciler.update_finalized(99);

        assert_eq!(reconciler.get_slot(CommitmentLevel::Processed), 101);
        assert_eq!(reconciler.get_slot(CommitmentLevel::Confirmed), 100);
        assert_eq!(reconciler.get_slot(CommitmentLevel::Finalized), 99);

        assert!(reconciler.is_finalized(99));
        assert!(!reconciler.is_finalized(100));
    }

    #[test]
    fn test_slot_reconciler_rollback() {
        let reconciler = SlotReconciler::new(100);

        reconciler.update_processed(105);
        reconciler.update_confirmed(104);
        reconciler.update_finalized(103);

        assert_eq!(reconciler.get_root(), 100);
        assert_eq!(reconciler.rollback_count(), 0);

        // Simulate rollback
        reconciler.handle_rollback(102);

        assert_eq!(reconciler.get_root(), 102);
        assert_eq!(reconciler.rollback_count(), 1);
        assert_eq!(reconciler.get_slot(CommitmentLevel::Finalized), 102);
    }
}
