//! Compressed bitmap indexes for efficient pubkey set operations
//!
//! Uses RoaringBitmap for compression and fast intersections.
//! Combined with pubkey dictionary (pubkey <-> pubkey_id) for memory efficiency.

use roaring::RoaringTreemap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;

/// Compressed bitmap wrapper for pubkey sets
/// 
/// This is the core data structure for all secondary indexes.
/// Provides efficient storage and fast set operations (AND, OR, NOT).
#[derive(Debug, Clone)]
pub struct PubkeyBitmap {
    /// Roaring bitmap of pubkey_ids
    bitmap: RoaringTreemap,
    /// Cardinality cache (for quick size checks)
    cardinality: u64,
}

impl Serialize for PubkeyBitmap {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let bytes = self.serialize();
        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for PubkeyBitmap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        Self::deserialize(&bytes).map_err(serde::de::Error::custom)
    }
}

impl PubkeyBitmap {
    pub fn new() -> Self {
        Self {
            bitmap: RoaringTreemap::new(),
            cardinality: 0,
        }
    }

    /// Insert a pubkey_id into the bitmap
    #[inline]
    pub fn insert(&mut self, pubkey_id: u64) -> bool {
        if self.bitmap.insert(pubkey_id) {
            self.cardinality = self.bitmap.len();
            true
        } else {
            false
        }
    }

    /// Remove a pubkey_id from the bitmap
    #[inline]
    pub fn remove(&mut self, pubkey_id: u64) -> bool {
        if self.bitmap.remove(pubkey_id) {
            self.cardinality = self.bitmap.len();
            true
        } else {
            false
        }
    }

    /// Check if bitmap contains a pubkey_id
    #[inline]
    pub fn contains(&self, pubkey_id: u64) -> bool {
        self.bitmap.contains(pubkey_id)
    }

    /// Get number of pubkeys in bitmap
    #[inline]
    pub fn len(&self) -> u64 {
        self.cardinality
    }

    /// Check if bitmap is empty
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.cardinality == 0
    }

    /// Get cardinality without updating cache (for reads)
    pub fn cardinality(&self) -> u64 {
        self.bitmap.len()
    }

    /// Intersect with another bitmap (AND operation) - returns new bitmap
    pub fn intersection(&self, other: &Self) -> Self {
        let mut result = self.bitmap.clone();
        // roaring 0.10 uses & for intersection
        result &= &other.bitmap;
        let cardinality = result.len();
        Self {
            bitmap: result,
            cardinality,
        }
    }

    /// Union with another bitmap (OR operation) - returns new bitmap
    pub fn union(&self, other: &Self) -> Self {
        let mut result = self.bitmap.clone();
        // roaring 0.10 uses | for union
        result |= &other.bitmap;
        let cardinality = result.len();
        Self {
            bitmap: result,
            cardinality,
        }
    }

    /// Difference (NOT operation) - returns new bitmap
    pub fn difference(&self, other: &Self) -> Self {
        let mut result = self.bitmap.clone();
        // roaring 0.10 uses - for difference
        result -= &other.bitmap;
        let cardinality = result.len();
        Self {
            bitmap: result,
            cardinality,
        }
    }

    /// Iterate over all pubkey_ids
    pub fn iter(&self) -> impl Iterator<Item = u64> + '_ {
        self.bitmap.iter()
    }

    /// Get first N pubkey_ids (for pagination)
    pub fn first_n(&self, n: usize) -> Vec<u64> {
        self.bitmap.iter().take(n).collect()
    }

    /// Serialize to bytes (for storage)
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.bitmap.serialize_into(&mut bytes).expect("serialize should succeed");
        bytes
    }

    /// Deserialize from bytes
    pub fn deserialize(bytes: &[u8]) -> Result<Self, std::io::Error> {
        let bitmap = RoaringTreemap::deserialize_from(bytes)?;
        let cardinality = bitmap.len();
        Ok(Self { bitmap, cardinality })
    }

    /// Estimate compressed size in bytes
    pub fn compressed_size(&self) -> usize {
        self.serialize().len()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.bitmap.clear();
        self.cardinality = 0;
    }
}

impl Default for PubkeyBitmap {
    fn default() -> Self {
        Self::new()
    }
}

/// Pubkey dictionary for compression
/// 
/// Maps pubkey <-> pubkey_id (u64) for efficient bitmap indexing.
/// Reduces memory usage from 32 bytes per pubkey to 8 bytes in indexes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PubkeyDictionary {
    /// pubkey -> pubkey_id mapping
    pubkey_to_id: HashMap<Pubkey, u64>,
    /// pubkey_id -> pubkey reverse mapping
    id_to_pubkey: HashMap<u64, Pubkey>,
    /// Next available pubkey_id
    next_id: u64,
}

impl PubkeyDictionary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a pubkey and get its ID (assigns new ID if not exists)
    pub fn insert(&mut self, pubkey: Pubkey) -> u64 {
        *self.pubkey_to_id.entry(pubkey).or_insert_with(|| {
            let id = self.next_id;
            self.next_id += 1;
            self.id_to_pubkey.insert(id, pubkey);
            id
        })
    }

    /// Get pubkey_id for a pubkey (returns None if not found)
    pub fn get_id(&self, pubkey: &Pubkey) -> Option<u64> {
        self.pubkey_to_id.get(pubkey).copied()
    }

    /// Get pubkey for a pubkey_id (returns None if not found)
    pub fn get_pubkey(&self, pubkey_id: u64) -> Option<Pubkey> {
        self.id_to_pubkey.get(&pubkey_id).copied()
    }

    /// Check if pubkey exists in dictionary
    pub fn contains(&self, pubkey: &Pubkey) -> bool {
        self.pubkey_to_id.contains_key(pubkey)
    }

    /// Get total number of pubkeys in dictionary
    pub fn len(&self) -> usize {
        self.pubkey_to_id.len()
    }

    /// Check if dictionary is empty
    pub fn is_empty(&self) -> bool {
        self.pubkey_to_id.is_empty()
    }

    /// Batch insert pubkeys and get their IDs
    pub fn batch_insert(&mut self, pubkeys: &[Pubkey]) -> Vec<u64> {
        pubkeys.iter().map(|pk| self.insert(*pk)).collect()
    }

    /// Batch get pubkey_ids (returns None for missing pubkeys)
    pub fn batch_get_id(&self, pubkeys: &[Pubkey]) -> Vec<Option<u64>> {
        pubkeys.iter().map(|pk| self.get_id(pk)).collect()
    }

    /// Batch resolve pubkey_ids to pubkeys
    pub fn batch_get_pubkey(&self, pubkey_ids: &[u64]) -> Vec<Option<Pubkey>> {
        pubkey_ids.iter().map(|id| self.get_pubkey(*id)).collect()
    }

    /// Serialize dictionary to bytes (for persistence)
    pub fn serialize(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap_or_default()
    }

    /// Deserialize dictionary from bytes
    pub fn deserialize(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }

    /// Get memory usage estimate in bytes
    pub fn memory_usage(&self) -> usize {
        // Approximate: each entry is 32 (pubkey) + 8 (u64) + HashMap overhead
        let entry_size = 32 + 8 + 64; // HashMap overhead estimate
        self.pubkey_to_id.len() * entry_size * 2 // forward + reverse maps
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.pubkey_to_id.clear();
        self.id_to_pubkey.clear();
        self.next_id = 0;
    }
}

impl FromIterator<Pubkey> for PubkeyDictionary {
    fn from_iter<I: IntoIterator<Item = Pubkey>>(iter: I) -> Self {
        let mut dict = Self::new();
        for pubkey in iter {
            dict.insert(pubkey);
        }
        dict
    }
}

/// Index entry with bitmap reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    /// Index key (e.g., program_id, token_owner, etc.)
    pub key: Vec<u8>,
    /// Bitmap of matching pubkey_ids
    pub bitmap: PubkeyBitmap,
    /// Last update slot
    pub last_updated_slot: u64,
    /// Number of entries
    pub entry_count: u64,
}

impl IndexEntry {
    pub fn new(key: Vec<u8>) -> Self {
        Self {
            key,
            bitmap: PubkeyBitmap::new(),
            last_updated_slot: 0,
            entry_count: 0,
        }
    }

    pub fn update(&mut self, bitmap: PubkeyBitmap, slot: u64) {
        self.bitmap = bitmap;
        self.entry_count = self.bitmap.len();
        self.last_updated_slot = slot;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitmap_operations() {
        let mut bitmap1 = PubkeyBitmap::new();
        bitmap1.insert(1);
        bitmap1.insert(2);
        bitmap1.insert(3);

        let mut bitmap2 = PubkeyBitmap::new();
        bitmap2.insert(2);
        bitmap2.insert(3);
        bitmap2.insert(4);

        let intersection = bitmap1.intersection(&bitmap2);
        assert_eq!(intersection.len(), 2);
        assert!(intersection.contains(2));
        assert!(intersection.contains(3));

        let union = bitmap1.union(&bitmap2);
        assert_eq!(union.len(), 4);

        let diff = bitmap1.difference(&bitmap2);
        assert_eq!(diff.len(), 1);
        assert!(diff.contains(1));
    }

    #[test]
    fn test_pubkey_dictionary() {
        let mut dict = PubkeyDictionary::new();
        let pk1 = Pubkey::new_unique();
        let pk2 = Pubkey::new_unique();

        let id1 = dict.insert(pk1);
        let id2 = dict.insert(pk2);

        assert_eq!(dict.get_id(&pk1), Some(id1));
        assert_eq!(dict.get_id(&pk2), Some(id2));
        assert_eq!(dict.get_pubkey(id1), Some(pk1));
        assert_eq!(dict.get_pubkey(id2), Some(pk2));
        assert_eq!(dict.len(), 2);
    }

    #[test]
    fn test_bitmap_serialization() {
        let mut bitmap = PubkeyBitmap::new();
        bitmap.insert(1);
        bitmap.insert(100);
        bitmap.insert(1000);

        let bytes = bitmap.serialize();
        let deserialized = PubkeyBitmap::deserialize(&bytes).unwrap();

        assert_eq!(deserialized.len(), 3);
        assert!(deserialized.contains(1));
        assert!(deserialized.contains(100));
        assert!(deserialized.contains(1000));
    }
}
