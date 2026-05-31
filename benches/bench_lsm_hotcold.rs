//! Benchmark: LSM Bitmap & Hot/Cold Index Performance
//! 
//! Measures lock-free write throughput and tiered query latency

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use index_fabric::{LsmBitmap, HotColdIndexManager};
use solana_sdk::pubkey::Pubkey;

fn bench_lsm_bitmap_inserts(c: &mut Criterion) {
    let bitmap = LsmBitmap::new();
    
    c.bench_function("lsm_bitmap_insert_10k", |b| {
        b.iter(|| {
            for i in 0..10_000 {
                bitmap.insert(black_box(i));
            }
        })
    });
}

fn bench_lsm_bitmap_concurrent(c: &mut Criterion) {
    let bitmap = Arc::new(LsmBitmap::new());
    
    c.bench_function("lsm_bitmap_concurrent_100_threads", |b| {
        b.iter(|| {
            let mut handles = vec![];
            for t in 0..100 {
                let bitmap_clone = Arc::clone(&bitmap);
                let handle = std::thread::spawn(move || {
                    for i in 0..1000 {
                        bitmap_clone.insert(t * 1000 + i);
                    }
                });
                handles.push(handle);
            }
            for handle in handles {
                handle.join().unwrap();
            }
        })
    });
}

fn bench_lsm_bitmap_query(c: &mut Criterion) {
    let bitmap = LsmBitmap::new();
    
    // Pre-populate
    for i in 0..100_000 {
        bitmap.insert(i);
    }
    
    c.bench_function("lsm_bitmap_query_contains", |b| {
        b.iter(|| {
            for i in 0..1000 {
                black_box(bitmap.contains(i));
            }
        })
    });
}

fn bench_hot_cold_insert(c: &mut Criterion) {
    let manager = HotColdIndexManager::new();
    let program_id = Pubkey::new_unique();
    let index = manager.get_or_create_index(program_id);
    
    c.bench_function("hot_cold_insert_account", |b| {
        b.iter(|| {
            let pubkey = Pubkey::new_unique();
            index.insert_account(black_box(1), pubkey);
        })
    });
}

fn bench_hot_cold_query(c: &mut Criterion) {
    let manager = HotColdIndexManager::new();
    let program_id = Pubkey::new_unique();
    let index = manager.get_or_create_index(program_id);
    
    // Pre-populate hot tier
    for i in 0..10_000 {
        let pubkey = Pubkey::new_unique();
        index.insert_account(i, pubkey);
    }
    
    c.bench_function("hot_cold_query_hot_tier", |b| {
        b.iter(|| {
            black_box(index.query_hot());
        })
    });
}

fn bench_hot_cold_access(c: &mut Criterion) {
    let manager = HotColdIndexManager::new();
    manager.update_slot(1000);
    
    let program_id = Pubkey::new_unique();
    let index = manager.get_or_create_index(program_id);
    
    // Insert accounts
    for i in 0..100 {
        let pubkey = Pubkey::new_unique();
        index.insert_account(i, pubkey);
    }
    
    c.bench_function("hot_cold_access_account", |b| {
        b.iter(|| {
            index.access_account(black_box(0), program_id);
        })
    });
}

criterion_group!(
    benches,
    bench_lsm_bitmap_inserts,
    bench_lsm_bitmap_concurrent,
    bench_lsm_bitmap_query,
    bench_hot_cold_insert,
    bench_hot_cold_query,
    bench_hot_cold_access,
);
criterion_main!(benches);
