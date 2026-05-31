//! Slot Reconciler Rollback Handling
//!
//! Manages slot rollbacks and state reversion.

use solana_sdk::clock::Slot;

/// Rollback event
#[derive(Debug, Clone)]
pub struct RollbackEvent {
    /// Slot we're rolling back from
    pub from_slot: Slot,
    /// Slot we're rolling back to
    pub to_slot: Slot,
    /// Timestamp of rollback
    pub timestamp: u64,
}

/// Rollback handler
pub struct RollbackHandler {
    rollback_count: u64,
    last_rollback: Option<RollbackEvent>,
}

impl RollbackHandler {
    pub fn new() -> Self {
        Self {
            rollback_count: 0,
            last_rollback: None,
        }
    }

    pub fn handle_rollback(&mut self, from_slot: Slot, to_slot: Slot) {
        self.rollback_count += 1;
        self.last_rollback = Some(RollbackEvent {
            from_slot,
            to_slot,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        });

        tracing::warn!("Rollback: {} -> {} (total: {})", 
            from_slot, to_slot, self.rollback_count);
    }

    pub fn rollback_count(&self) -> u64 {
        self.rollback_count
    }

    pub fn last_rollback(&self) -> Option<&RollbackEvent> {
        self.last_rollback.as_ref()
    }
}

impl Default for RollbackHandler {
    fn default() -> Self {
        Self::new()
    }
}
