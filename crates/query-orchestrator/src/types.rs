//! Query types condivisi per il Query Orchestrator

/// getProgramAccounts filter types
#[derive(Debug, Clone)]
pub enum GpaFilter {
    /// Filter by account data size
    DataSize(u64),
    /// Filter by memcmp (offset + bytes)
    Memcmp { offset: usize, bytes: Vec<u8> },
    /// Filter by Anchor discriminator
    Discriminator([u8; 8]),
}
