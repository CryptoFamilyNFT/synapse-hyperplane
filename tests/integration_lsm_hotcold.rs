//! Integration Tests for LSM Bitmap & Hot/Cold Index Separation
//! 
//! Tests lock-free writes, compaction, and tier migration

use std::sync::Arc;
use std::thread;
use index_fabric::{LsmBitmap, HotColdIndexManager, TieredIndex, StorageTier};
use solana_sdk::pubkey::Pubkey;

#[test]
fn test_lsm_bitmap_concurrent_inserts() {
    let bitmap = Arc::new(LsmBitmap::new());
    
    // Spawn 100 threads, each inserting 1000 entries
    let mut handles = vec![];
    for t in 0..100 {
        let bitmap_clone = Arc::clone(&bitmap);
        let handle = thread::spawn(move || {
            for i in 0..1000 {
                bitmap_clone.insert(t * 1000 + i);
            }
        });
        handles.push(handle);
    }
    
    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Verify all entries are present
    let stats = bitmap.stats();
    assert_eq!(stats.estimated_total, 100_000);
    println!("LSM Bitmap: 100K concurrent inserts completed");
}

#[test]
fn test_lsm_bitmap_compaction_triggers() {
    let bitmap = LsmBitmap::with_config(2, 100); // Trigger after 100 entries
    
    // Insert 250 entries (should trigger compaction)
    for i in 0..250 {
        bitmap.insert(i);
    }
    
    // Wait for async compaction
    thread::sleep(std::time::Duration::from_millis(200));
    
    let stats = bitmap.stats();
    
    // Should have compacted (1 delta after compaction)
    assert_eq!(stats.delta_count, 1);
    assert!(stats.estimated_total >= 250);
    
    println!("LSM Compaction: triggered successfully after 250 inserts");
}

#[test]
fn test_lsm_bitmap_query_during_compaction() {
    let bitmap = Arc::new(LsmBitmap::with_config(2, 50));
    
    // Insert initial data
    for i in 0..100 {
        bitmap.insert(i);
    }
    
    // Start compaction in background
    let bitmap_clone = Arc::clone(&bitmap);
    let compact_handle = thread::spawn(move || {
        bitmap_clone.compact();
    });
    
    // Query while compaction is running (should not block)
    let start = std::time::Instant::now();
    for i in 0..100 {
        assert!(bitmap.contains(i));
    }
    let query_time = start.elapsed();
    
    // Queries should complete in <10ms even during compaction
    assert!(query_time < std::time::Duration::from_millis(10));
    
    compact_handle.join().unwrap();
    
    println!("LSM Query during compaction: {:?} (non-blocking)", query_time);
}

#[test]
fn test_hot_cold_tier_migration() {
    let manager = HotColdIndexManager::new();
    manager.update_slot(1000);
    
    // Get tiered index for a program
    let program_id = Pubkey::new_unique();
    let index = manager.get_or_create_index(program_id);
    
    // Insert account (starts in Hot tier)
    let account_id = 1u32;
    index.insert_account(account_id, program_id);
    
    // Verify it's in Hot tier
    let hot_results = index.query_hot();
    assert!(hot_results.contains(account_id));
    
    // Simulate time passing (slot advances)
    manager.update_slot(1500); // 500 slots later
    
    // Access account (keeps it hot)
    index.access_account(account_id, program_id);
    
    // Should still be hot (recently accessed)
    let stats = index.stats();
    assert_eq!(stats.hot_cardinality, 1);
    
    println!("Hot/Cold: Account stays hot with recent access");
}

#[test]
fn test_hot_cold_demotion() {
    let manager = HotColdIndexManager::new();
    manager.update_slot(1000);
    
    let program_id = Pubkey::new_unique();
    let index = manager.get_or_create_index(program_id);
    
    // Insert multiple accounts
    for i in 0..10 {
        index.insert_account(i, program_id);
    }
    
    // Advance slot significantly (triggers demotion)
    manager.update_slot(15000); // 14000 slots later (>10k threshold)
    
    // Access some accounts (keeps them hot)
    index.access_account(0, program_id);
    index.access_account(1, program_id);
    
    let stats = index.stats();
    
    // Accessed accounts should be hot, others cold
    assert_eq!(stats.hot_cardinality, 2);
    assert!(stats.cold_cardinality >= 8);
    
    println!("Hot/Cold: {} hot, {} cold (demotion working)", 
             stats.hot_cardinality, stats.cold_cardinality);
}

#[test]
#[ignore] // Performance test
fn test_lsm_bitmap_throughput() {
    let bitmap = LsmBitmap::new();
    
    // Benchmark insert throughput
    let start = std::time::Instant::now();
    for i in 0..1_000_000 {
        bitmap.insert(i);
    }
    let elapsed = start.elapsed();
    
    let throughput = 1_000_000 / elapsed.as_secs() as u64;
    println!("LSM Throughput: {} inserts/sec", throughput);
    
    // Should achieve >100K inserts/sec
    assert!(throughput > 100_000);
}

#[test]
#[ignore] // Performance test
fn test_hot_cold_query_latency() {
    let manager = HotColdIndexManager::new();
    manager.update_slot(1000);
    
    let program_id = Pubkey::new_unique();
    let index = manager.get_or_create_index(program_id);
    
    // Insert 10K accounts in Hot tier
    for i in 0..10_000 {
        index.insert_account(i, program_id);
    }
    
    // Benchmark query latency for Hot tier
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let _results = index.query_hot();
    }
    let elapsed = start.elapsed();
    
    let avg_latency = elapsed / 1000;
    println!("Hot tier query latency: {:?} (avg)", avg_latency);
    
    // Should be <100 microseconds per query
    assert!(avg_latency < std::time::Duration::from_micros(100));
}
