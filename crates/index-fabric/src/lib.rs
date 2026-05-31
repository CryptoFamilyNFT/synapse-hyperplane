//! Index Fabric - Compressed bitmap indexes
//!
//! Provides secondary indexes using RoaringBitmap + pubkey dictionary:
//! - Program index
//! - Token owner index
//! - Token mint index
//! - Data size index
//! - Adaptive memcmp indexes
//! - Discriminator index (for Anchor programs)
//! - Memcmp Accelerator (pre-indicizzazione offset comuni)
//! - LSM Bitmap (Bitmap Delta Architecture)
//! - Hot/Cold Index Separation
//! - Shared Memory API (zero-copy results)
//! - NUMA-Aware Fabric (memory pinning)
//! - Adaptive Secondary Indexes
//! - Tiered Dictionary
//! - SIMD Bitmap Engine
//! - Query Result Cache

pub mod bitmap_store;
pub mod program_index;
pub mod token_owner_index;
pub mod token_mint_index;
pub mod data_size_index;
pub mod memcmp_index;
pub mod discriminator_index;
pub mod memcmp_accelerator;
pub mod lsm_bitmap;  // Bitmap Delta Architecture (LSM)
pub mod hot_cold_index;  // Hot/Cold Index Separation
pub mod shared_memory;  // Shared Memory API
pub mod numa_fabric;  // NUMA-Aware Fabric
pub mod adaptive_index;  // Adaptive Secondary Indexes
pub mod tiered_dictionary;  // Tiered Dictionary
pub mod simd_bitmap;  // SIMD Bitmap Engine
pub mod query_cache;  // Query Result Cache

// Re-export main types
pub use program_index::ProgramIndex;
pub use token_owner_index::TokenOwnerIndex;
pub use token_mint_index::TokenMintIndex;
pub use data_size_index::DataSizeIndex;
pub use memcmp_index::MemcmpIndex;
pub use discriminator_index::DiscriminatorIndex;

// Re-export memcmp accelerator types
pub use memcmp_accelerator::{
    MemcmpAccelerator,
    ProgramMemcmpAccelerator,
    OffsetIndex,
    CommonOffset,
    predefined_configs,
};

// Re-export LSM bitmap
pub use lsm_bitmap::LsmBitmap;

// Re-export Hot/Cold index
pub use hot_cold_index::{
    HotColdIndexManager,
    TieredIndex,
    StorageTier,
    AccountMetadata,
    TieredIndexStats,
    HotColdGlobalStats,
};

// Re-export Shared Memory types
pub use shared_memory::{
    SharedMemoryRegion,
    SharedMemoryManager,
    ZeroCopyResult,
    SharedMemHeader,
};

// Re-export NUMA Fabric types
pub use numa_fabric::{
    NumaAllocator,
    NumaPolicy,
    NumaBufferPool,
    NumaIndexStorage,
    NumaNodeInfo,
    NumaPoolStats,
    detect_numa_config,
    current_core_id,
};

// Re-export Adaptive Index types
pub use adaptive_index::{
    AdaptiveIndex,
    AdaptiveIndexManager,
    AdaptiveIndexType,
    IndexStats,
};

// Re-export Tiered Dictionary types
pub use tiered_dictionary::{
    TieredDictionary,
    DictEntry,
    DictTier,
    TieredDictStats,
};

// Re-export SIMD Bitmap types
pub use simd_bitmap::{
    SimdBitmapEngine,
    SimdRoaringBitmap,
    SimdStats,
};

// Re-export Query Cache types
pub use query_cache::{
    QueryResultCache,
    CacheEntry,
    CacheStats,
};
