//! Slot Reconciler Commitments
//!
//! Tracks commitment levels for account reads.

use solana_sdk::clock::Slot;

/// Commitment tracking state
#[derive(Debug, Clone, Default)]
pub struct CommitmentTracker {
    /// Last processed slot
    pub processed: Slot,
    /// Last confirmed slot
    pub confirmed: Slot,
    /// Last finalized slot
    pub finalized: Slot,
}

impl CommitmentTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, processed: Slot, confirmed: Slot, finalized: Slot) {
        self.processed = processed;
        self.confirmed = confirmed;
        self.finalized = finalized;
    }
}
