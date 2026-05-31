//! Slot tracking and commitment management
//!
//! Handles slot watermarks, commitment views (processed/confirmed/finalized),
//! and rollback detection for the read engine.

use solana_sdk::clock::Slot;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Commitment level for account reads
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CommitmentLevel {
    /// Most recent state, may be rolled back
    Processed,
    /// Aggregated across 66%+ of cluster stake
    Confirmed,
    /// Immutable, part of finalized block
    Finalized,
}

impl CommitmentLevel {
    /// Get minimum confirmation requirement for each level
    pub fn min_confirmations(&self) -> u64 {
        match self {
            Self::Processed => 0,
            Self::Confirmed => 1,
            Self::Finalized => 31, // ~32 slots for finalization
        }
    }

    /// Check if this commitment satisfies another
    pub fn satisfies(&self, other: &Self) -> bool {
        self >= other
    }
}

impl Default for CommitmentLevel {
    fn default() -> Self {
        Self::Processed
    }
}

/// Slot context for read operations
/// 
/// Tracks current slot watermarks for each commitment level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotContext {
    /// Current processed slot
    pub processed_slot: Slot,
    /// Current confirmed slot
    pub confirmed_slot: Slot,
    /// Current finalized slot (root)
    pub finalized_slot: Slot,
    /// Parent slot of current processed
    pub parent_slot: Slot,
}

impl SlotContext {
    pub fn new() -> Self {
        Self {
            processed_slot: 0,
            confirmed_slot: 0,
            finalized_slot: 0,
            parent_slot: 0,
        }
    }

    /// Update slot context after slot processing
    pub fn update(&mut self, new_slot: Slot, _parent_slot: Slot) {
        self.parent_slot = self.processed_slot;
        self.processed_slot = new_slot;
        
        // Confirmed lags by ~1 slot
        if new_slot > 1 {
            self.confirmed_slot = new_slot - 1;
        }
        
        // Finalized lags by ~31 slots
        if new_slot > 31 {
            self.finalized_slot = new_slot - 31;
        }
    }

    /// Get the appropriate slot watermark for a commitment level
    pub fn get_watermark(&self, commitment: CommitmentLevel) -> Slot {
        match commitment {
            CommitmentLevel::Processed => self.processed_slot,
            CommitmentLevel::Confirmed => self.confirmed_slot,
            CommitmentLevel::Finalized => self.finalized_slot,
        }
    }

    /// Check if a slot is visible at given commitment
    pub fn is_visible(&self, slot: Slot, commitment: CommitmentLevel) -> bool {
        slot <= self.get_watermark(commitment)
    }
}

impl Default for SlotContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Atomic slot tracker for lock-free updates
/// 
/// Used by the read mesh to check slot watermarks without locks
pub struct AtomicSlotTracker {
    processed_slot: AtomicU64,
    confirmed_slot: AtomicU64,
    finalized_slot: AtomicU64,
}

impl AtomicSlotTracker {
    pub fn new() -> Self {
        Self {
            processed_slot: AtomicU64::new(0),
            confirmed_slot: AtomicU64::new(0),
            finalized_slot: AtomicU64::new(0),
        }
    }

    pub fn update(&self, context: &SlotContext) {
        self.processed_slot.store(context.processed_slot, Ordering::Release);
        self.confirmed_slot.store(context.confirmed_slot, Ordering::Release);
        self.finalized_slot.store(context.finalized_slot, Ordering::Release);
    }

    pub fn get_watermark(&self, commitment: CommitmentLevel) -> Slot {
        match commitment {
            CommitmentLevel::Processed => self.processed_slot.load(Ordering::Acquire),
            CommitmentLevel::Confirmed => self.confirmed_slot.load(Ordering::Acquire),
            CommitmentLevel::Finalized => self.finalized_slot.load(Ordering::Acquire),
        }
    }

    pub fn get_context(&self) -> SlotContext {
        SlotContext {
            processed_slot: self.processed_slot.load(Ordering::Acquire),
            confirmed_slot: self.confirmed_slot.load(Ordering::Acquire),
            finalized_slot: self.finalized_slot.load(Ordering::Acquire),
            parent_slot: self.processed_slot.load(Ordering::Acquire).saturating_sub(1),
        }
    }
}

impl Default for AtomicSlotTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Slot range for batch operations
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SlotRange {
    pub start_slot: Slot,
    pub end_slot: Slot,
}

impl SlotRange {
    pub fn new(start_slot: Slot, end_slot: Slot) -> Self {
        Self { start_slot, end_slot }
    }

    pub fn contains(&self, slot: Slot) -> bool {
        slot >= self.start_slot && slot <= self.end_slot
    }

    pub fn len(&self) -> u64 {
        self.end_slot - self.start_slot + 1
    }

    pub fn is_empty(&self) -> bool {
        self.end_slot < self.start_slot
    }
}

/// Root status for compaction decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootStatus {
    pub current_root: Slot,
    pub pending_compaction_root: Slot,
    pub last_compaction_root: Slot,
    pub slots_since_last_compaction: u64,
}

impl RootStatus {
    pub fn needs_compaction(&self, threshold_slots: u64) -> bool {
        self.slots_since_last_compaction >= threshold_slots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_context_update() {
        let mut ctx = SlotContext::new();
        ctx.update(100, 99);

        assert_eq!(ctx.processed_slot, 100);
        assert_eq!(ctx.confirmed_slot, 99);
        assert_eq!(ctx.finalized_slot, 69); // 100 - 31

        ctx.update(200, 199);
        assert_eq!(ctx.processed_slot, 200);
        assert_eq!(ctx.confirmed_slot, 199);
        assert_eq!(ctx.finalized_slot, 169);
    }

    #[test]
    fn test_commitment_satisfaction() {
        assert!(CommitmentLevel::Finalized.satisfies(&CommitmentLevel::Processed));
        assert!(CommitmentLevel::Finalized.satisfies(&CommitmentLevel::Confirmed));
        assert!(CommitmentLevel::Confirmed.satisfies(&CommitmentLevel::Processed));
        assert!(!CommitmentLevel::Processed.satisfies(&CommitmentLevel::Confirmed));
    }

    #[test]
    fn test_atomic_slot_tracker() {
        let tracker = AtomicSlotTracker::new();
        let ctx = SlotContext {
            processed_slot: 100,
            confirmed_slot: 99,
            finalized_slot: 69,
            parent_slot: 99,
        };
        tracker.update(&ctx);

        assert_eq!(tracker.get_watermark(CommitmentLevel::Processed), 100);
        assert_eq!(tracker.get_watermark(CommitmentLevel::Confirmed), 99);
        assert_eq!(tracker.get_watermark(CommitmentLevel::Finalized), 69);
    }
}
