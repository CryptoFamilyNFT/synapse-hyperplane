//! Benchmark: Query Cost Model Performance
//! 
//! Compares optimized query planning vs baseline

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;
use std::path::PathBuf;
use index_fabric::{ProgramIndex, DataSizeIndex, MemcmpIndex, DiscriminatorIndex, MemcmpAccelerator};
use query_orchestrator::QueryPlanner;
use solana_sdk::pubkey::Pubkey;

fn create_test_indexes(size: usize) -> (
    Arc<ProgramIndex>,
    Arc<DataSizeIndex>,
    Arc<MemcmpIndex>,
    Arc<DiscriminatorIndex>,
    Arc<MemcmpAccelerator>,
) {
    let temp_dir = std::env::temp_dir().join("bench_indexes");
    
    let program_index = Arc::new(ProgramIndex::new(temp_dir.join("program")));
    let data_size_index = Arc::new(DataSizeIndex::new(temp_dir.join("size")));
    let memcmp_index = Arc::new(MemcmpIndex::new(temp_dir.join("memcmp")));
    let discriminator_index = Arc::new(DiscriminatorIndex::new(temp_dir.join("disc")));
    let accelerator = Arc::new(MemcmpAccelerator::new(temp_dir.join("accel")));
    
    // Populate indexes
    for i in 0..size {
        let pubkey = Pubkey::new_unique();
        program_index.add_account(pubkey, Pubkey::new_unique(), 1000 + i);
        data_size_index.add_account(pubkey, 200 + (i % 100), 1000 + i);
    }
    
    (program_index, data_size_index, memcmp_index, discriminator_index, accelerator)
}

fn bench_query_planning_small(c: &mut Criterion) {
    let (program, size, memcmp, disc, accel) = create_test_indexes(1_000);
    let planner = QueryPlanner::new(program, size, memcmp, disc, accel);
    
    c.bench_function("query_plan_1k_accounts", |b| {
        b.iter(|| {
            let filters = vec![
                query_orchestrator::GpaFilter::DataSize(200),
            ];
            let _plan = planner.plan_query(black_box(Pubkey::new_unique()), filters);
        })
    });
}

fn bench_query_planning_large(c: &mut Criterion) {
    let (program, size, memcmp, disc, accel) = create_test_indexes(100_000);
    let planner = QueryPlanner::new(program, size, memcmp, disc, accel);
    
    c.bench_function("query_plan_100k_accounts", |b| {
        b.iter(|| {
            let filters = vec![
                query_orchestrator::GpaFilter::DataSize(200),
                query_orchestrator::GpaFilter::Memcmp { offset: 0, bytes: vec![1; 8] },
            ];
            let _plan = planner.plan_query(black_box(Pubkey::new_unique()), filters);
        })
    });
}

fn bench_memcmp_accelerator_lookup(c: &mut Criterion) {
    let temp_dir = std::env::temp_dir().join("bench_accel");
    let accelerator = MemcmpAccelerator::new(temp_dir.join("accel"));
    
    // Pre-populate with common offsets
    for offset in 0..100 {
        accelerator.track_offset(offset, 8);
    }
    
    c.bench_function("memcmp_accelerator_lookup", |b| {
        b.iter(|| {
            let _stats = accelerator.get_stats();
        })
    });
}

criterion_group!(
    benches,
    bench_query_planning_small,
    bench_query_planning_large,
    bench_memcmp_accelerator_lookup,
);
criterion_main!(benches);
