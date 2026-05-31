//! Integration Tests for Shared Memory, NUMA, and Query Cache
//! 
//! Tests zero-copy results, NUMA allocation, and multi-level caching

use std::sync::Arc;
use index_fabric::{
    SharedMemoryManager, ZeroCopyResult,
    NumaAllocator, NumaPolicy, NumaBufferPool,
    QueryResultCache,
};

#[test]
fn test_shared_memory_zero_copy() {
    let temp_dir = std::env::temp_dir().join("shm_zerocopy");
    let manager = SharedMemoryManager::new(temp_dir).unwrap();
    
    // Create shared memory region
    let mut region = manager.get_or_create_region("query_results", 1024 * 1024).unwrap();
    
    // Write large result set (1MB)
    let data = vec![42u8; 1024 * 1024];
    region.write_data(0, &data).unwrap();
    
    // Create zero-copy wrapper
    let zero_copy = ZeroCopyResult::new(region, 0, data.len());
    
    // Access data without copying
    let slice = zero_copy.as_slice();
    assert_eq!(slice.len(), 1024 * 1024);
    assert_eq!(slice[0], 42);
    
    println!("Shared Memory: Zero-copy access to 1MB data");
}

#[test]
fn test_shared_memory_multi_process_simulation() {
    let temp_dir = std::env::temp_dir().join("shm_multi");
    let manager = SharedMemoryManager::new(temp_dir).unwrap();
    
    // Writer process simulation
    let mut region = manager.get_or_create_region("shared", 1024 * 1024).unwrap();
    let test_data = b"Hello from writer!";
    region.write_data(0, test_data).unwrap();
    
    // Reader process simulation (re-open same region)
    let region2 = manager.get_or_create_region("shared", 1024 * 1024).unwrap();
    let read_data = region2.read_data(0, test_data.len());
    
    assert_eq!(read_data, test_data);
    
    println!("Shared Memory: Multi-process read successful");
}

#[test]
fn test_numa_allocator_preferred_node() {
    let allocator = NumaAllocator::new(0, NumaPolicy::Preferred);
    
    // Should always return preferred node
    for core in 0..16 {
        assert_eq!(allocator.optimal_node(core), 0);
    }
    
    println!("NUMA: Preferred node policy working");
}

#[test]
fn test_numa_buffer_pool() {
    let allocator = NumaAllocator::new(0, NumaPolicy::Local);
    let pool = NumaBufferPool::new(2, 4096, allocator);
    
    // Acquire buffers from different cores
    let buffer1 = pool.acquire_buffer(0); // Core 0 → Node 0
    let buffer2 = pool.acquire_buffer(10); // Core 10 → Node 1
    
    assert_eq!(buffer1.len(), 4096);
    assert_eq!(buffer2.len(), 4096);
    
    // Release back to pool
    pool.release_buffer(buffer1, 0);
    pool.release_buffer(buffer2, 10);
    
    let stats = pool.node_stats();
    assert!(stats.len() >= 1);
    
    println!("NUMA: Buffer pool with {} nodes", stats.len());
}

#[test]
fn test_query_cache_hit_rate() {
    let temp_dir = std::env::temp_dir().join("cache_test");
    let cache = QueryResultCache::new(temp_dir, 100, 1000, 10000).unwrap();
    
    // Insert some cached results
    for i in 0..50 {
        let key = cache.generate_key(&format!("query_{}", i));
        cache.insert(key, vec![i as u8; 100], 1000, 3600);
    }
    
    // Query cache (should hit)
    let mut hits = 0;
    for i in 0..50 {
        let key = cache.generate_key(&format!("query_{}", i));
        if cache.get(key).is_some() {
            hits += 1;
        }
    }
    
    let hit_rate = hits as f64 / 50.0;
    println!("Cache hit rate: {:.2}%", hit_rate * 100);
    
    // Should have high hit rate for repeated queries
    assert!(hit_rate > 0.8);
}

#[test]
fn test_query_cache_ttl_expiration() {
    let temp_dir = std::env::temp_dir().join("cache_ttl");
    let cache = QueryResultCache::new(temp_dir, 100, 1000, 10000).unwrap();
    
    // Insert with short TTL
    let key = cache.generate_key(&"short_ttl");
    cache.insert(key, vec![1, 2, 3], 1000, 0); // TTL 0 = immediate expiration
    
    // Should be expired immediately
    assert!(cache.get(key).is_none());
    
    println!("Cache: TTL expiration working");
}

#[test]
fn test_query_cache_slot_invalidation() {
    let temp_dir = std::env::temp_dir().join("cache_slot");
    let cache = QueryResultCache::new(temp_dir, 100, 1000, 10000).unwrap();
    
    // Insert at slot 1000
    let key = cache.generate_key(&"slot_query");
    cache.insert(key, vec![100], 1000, 3600);
    
    // Update slot (should invalidate old entries)
    cache.update_slot(20000); // 10k slots later
    
    // Entry should be invalidated
    let result = cache.get(key);
    assert!(result.is_none());
    
    println!("Cache: Slot-based invalidation working");
}

#[test]
#[ignore] // Performance test
fn test_shared_memory_throughput() {
    let temp_dir = std::env::temp_dir().join("shm_perf");
    let manager = SharedMemoryManager::new(temp_dir).unwrap();
    
    let mut region = manager.get_or_create_region("perf", 10 * 1024 * 1024).unwrap();
    
    // Benchmark write throughput
    let data = vec![42u8; 1024 * 1024]; // 1MB
    let start = std::time::Instant::now();
    for _ in 0..100 {
        region.write_data(0, &data).unwrap();
    }
    let elapsed = start.elapsed();
    
    let throughput = (100 * 1024 * 1024) as f64 / elapsed.as_secs_f64();
    println!("Shared Memory Write: {:.2} MB/s", throughput / (1024.0 * 1024.0));
    
    // Should achieve >1 GB/s
    assert!(throughput > 1024.0 * 1024.0 * 1024.0);
}

#[test]
#[ignore] // Performance test
fn test_query_cache_latency() {
    let temp_dir = std::env::temp_dir().join("cache_perf");
    let cache = QueryResultCache::new(temp_dir, 1000, 10000, 10000).unwrap();
    
    // Pre-populate cache
    for i in 0..1000 {
        let key = cache.generate_key(&format!("query_{}", i));
        cache.insert(key, vec![i as u8; 100], 1000, 3600);
    }
    
    // Benchmark cache lookup latency
    let start = std::time::Instant::now();
    for i in 0..10000 {
        let key = cache.generate_key(&format!("query_{}", i % 1000));
        let _ = cache.get(key);
    }
    let elapsed = start.elapsed();
    
    let avg_latency = elapsed / 10000;
    println!("Cache lookup latency: {:?}", avg_latency);
    
    // Should be <10 microseconds per lookup
    assert!(avg_latency < std::time::Duration::from_micros(10));
}
