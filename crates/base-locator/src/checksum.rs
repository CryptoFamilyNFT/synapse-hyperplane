//! Checksum utilities for data integrity verification
//!
//! Provides checksums for account data to detect corruption
//! and verify data consistency across layers.

use blake3::Hasher;
use solana_sdk::pubkey::Pubkey;

/// Compute Blake3 hash of account data
#[inline]
pub fn hash_account_data(data: &[u8]) -> [u8; 32] {
    blake3::hash(data).into()
}

/// Compute Blake3 hash of location metadata
pub fn hash_location_metadata(
    pubkey: Pubkey,
    slot: u64,
    write_version: u64,
    data_len: u32,
) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(&pubkey.to_bytes());
    hasher.update(&slot.to_le_bytes());
    hasher.update(&write_version.to_le_bytes());
    hasher.update(&data_len.to_le_bytes());
    hasher.finalize().into()
}

/// Verify account data integrity
pub fn verify_account_integrity(
    pubkey: Pubkey,
    data: &[u8],
    expected_hash: [u8; 32],
) -> bool {
    let computed = hash_account_data_with_pubkey(pubkey, data);
    computed == expected_hash
}

/// Compute Blake3 hash of account data with pubkey prefix
pub fn hash_account_data_with_pubkey(pubkey: Pubkey, data: &[u8]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(&pubkey.to_bytes());
    hasher.update(data);
    hasher.finalize().into()
}

/// Compute checksum for a batch of accounts (for compaction verification)
pub fn compute_batch_checksum(
    accounts: &[(Pubkey, Vec<u8>)],
) -> [u8; 32] {
    let mut hasher = Hasher::new();
    
    for (pubkey, data) in accounts {
        hasher.update(&pubkey.to_bytes());
        hasher.update(data);
    }
    
    hasher.finalize().into()
}

/// Merkle root computation for receipt inscriptions (SAP compatibility)
pub mod merkle {
    use sha2::{Digest, Sha256};
    
    /// Compute leaf hash for Merkle tree
    pub fn leaf_hash(data: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }
    
    /// Compute internal node hash
    pub fn node_hash(left: [u8; 32], right: [u8; 32]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(&left);
        hasher.update(&right);
        hasher.finalize().into()
    }
    
    /// Compute Merkle root from leaves
    pub fn compute_root(leaves: &[[u8; 32]]) -> [u8; 32] {
        if leaves.is_empty() {
            return [0u8; 32];
        }
        
        if leaves.len() == 1 {
            return leaves[0];
        }
        
        let mut level = leaves.to_vec();
        
        while level.len() > 1 {
            let mut next_level = Vec::new();
            
            for chunk in level.chunks(2) {
                if chunk.len() == 2 {
                    next_level.push(node_hash(chunk[0], chunk[1]));
                } else {
                    // Odd element, promote it
                    next_level.push(chunk[0]);
                }
            }
            
            level = next_level;
        }
        
        level[0]
    }
    
    /// Generate Merkle proof for leaf at index
    pub fn generate_proof(leaves: &[[u8; 32]], index: usize) -> Vec<[u8; 32]> {
        let mut proof = Vec::new();
        let mut level = leaves.to_vec();
        let mut idx = index;
        
        while level.len() > 1 {
            let mut next_level = Vec::new();
            
            for (i, chunk) in level.chunks(2).enumerate() {
                if chunk.len() == 2 {
                    let hash = node_hash(chunk[0], chunk[1]);
                    next_level.push(hash);
                    
                    // If this chunk contains our index, add sibling to proof
                    if i == idx / 2 {
                        let sibling_idx = if idx % 2 == 0 { 1 } else { 0 };
                        if chunk.len() > sibling_idx {
                            proof.push(chunk[sibling_idx]);
                        }
                    }
                } else {
                    next_level.push(chunk[0]);
                }
            }
            
            level = next_level;
            idx /= 2;
        }
        
        proof
    }
    
    /// Verify Merkle proof
    pub fn verify_proof(
        leaf: [u8; 32],
        proof: &[[u8; 32]],
        root: [u8; 32],
    ) -> bool {
        let mut current = leaf;
        
        for sibling in proof {
            current = node_hash(current, *sibling);
        }
        
        current == root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use merkle::*;

    #[test]
    fn test_account_hash() {
        let pubkey = Pubkey::new_unique();
        let data = vec![1, 2, 3, 4, 5];
        
        let hash1 = hash_account_data_with_pubkey(pubkey, &data);
        let hash2 = hash_account_data_with_pubkey(pubkey, &data);
        
        assert_eq!(hash1, hash2);
        
        // Different pubkey should produce different hash
        let pubkey2 = Pubkey::new_unique();
        let hash3 = hash_account_data_with_pubkey(pubkey2, &data);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_merkle_root() {
        let leaves: Vec<[u8; 32]> = (0..4)
            .map(|i| {
                let mut leaf = [0u8; 32];
                leaf[0] = i as u8;
                leaf
            })
            .collect();
        
        let root = compute_root(&leaves);
        assert_ne!(root, [0u8; 32]);
    }

    #[test]
    fn test_merkle_proof() {
        let leaves: Vec<[u8; 32]> = (0..8)
            .map(|i| {
                let mut leaf = [0u8; 32];
                leaf[0] = i as u8;
                leaf
            })
            .collect();
        
        let root = compute_root(&leaves);
        
        for i in 0..leaves.len() {
            let proof = generate_proof(&leaves, i);
            assert!(verify_proof(leaves[i], &proof, root));
        }
    }
}
