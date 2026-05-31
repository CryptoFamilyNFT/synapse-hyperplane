//! Integration Tests for Query Cost Model & Memcmp Accelerator
//! 
//! Tests end-to-end query optimization with real indexes

use std::sync::Arc;
use std::path::PathBuf;
use index_fabric::{
    ProgramIndex, DataSizeIndex, MemcmpIndex, DiscriminatorIndex,
    MemcmpAccelerator, LsmBitmap, HotColdIndexManager,
};
use query_orchestrator::{QueryPlanner, QueryCostModel};
use solana_sdk::pubkey::Pubkey;

#[test]
fn test_cost_model_with_real_indexes() {
    let temp_dir = std::env::temp_dir().join("cost_model_test");
    
    // Create real indexes
    let program_index = Arc::new(ProgramIndex::new(temp_dir.join("program")));
    let data_size_index = Arc::new(DataSizeIndex::new(temp_dir.join("size")));
    let memcmp_index = Arc::new(MemcmpIndex::new(temp_dir.join("memcmp")));
    let discriminator_index = Arc::new(DiscriminatorIndex::new(temp_dir.join("disc")));
    let memcmp_accelerator = Arc::new(MemcmpAccelerator::new(temp_dir.join("accel")));
    
    // Populate indexes with test data
    for i in 0..1000 {
        let pubkey = Pubkey::new_unique();
        let program_id = Pubkey::new_unique();
        
        program_index.add_account(pubkey, program_id, 1000 + i);
        data_size_index.add_account(pubkey, 200 + (i % 50), 1000 + i);
        
        // Add memcmp data (first 8 bytes as discriminator)
        let disc = [(i % 256) as u8; 8];
        discriminator_index.add_account(pubkey, disc, 1000 + i);
    }
    
    // Create query planner
    let planner = QueryPlanner::new(
        program_index,
        data_size_index,
        memcmp_index,
        discriminator_index,
        memcmp_accelerator,
    );
    
    // Create test query with multiple filters
    let program_id = Pubkey::new_unique();
    let filters = vec![
        query_orchestrator::GpaFilter::DataSize(200),
        query_orchestrator::GpaFilter::Memcmp { offset: 0, bytes: vec![1; 8] },
    ];
    
    // Plan query (should order by cardinality)
    let plan = planner.plan_query(program_id, filters);
    
    assert_eq!(plan.program_id, program_id);
    assert_eq!(plan.filters.len(), 2);
    
    // Verify filters are ordered by estimated cardinality
    // DataSize filter should come first (lower cardinality)
    assert!(matches!(plan.filters[0], query_orchestrator::GpaFilter::DataSize(_)));
}

#[test]
fn test_memcmp_accelerator_integration() {
    let temp_dir = std::env::temp_dir().join("accel_test");
    
    // Create accelerator with predefined config
    let accelerator = MemcmpAccelerator::new(temp_dir.join("accel"));
    
    // Simulate common offset patterns
    let test_data = b"discriminator_data_here";
    accelerator.track_offset(0, 8); // Discriminator
    accelerator.track_offset(0, 32); // Pubkey
    
    // Query accelerator for stats
    let stats = accelerator.get_stats();
    
    assert!(stats.total_offsets >= 2);
    assert!(stats.hit_rate >= 0.0);
}

#[test]
fn test_query_cost_estimation_accuracy() {
    let temp_dir = std::env::temp_dir().join("cost_test");
    
    // Create indexes
    let program_index = Arc::new(ProgramIndex::new(temp_dir.join("program")));
    let data_size_index = Arc::new(DataSizeIndex::new(temp_dir.join("size")));
    
    // Populate with known distribution
    for i in 0..100 {
        let pubkey = Pubkey::new_unique();
        program_index.add_account(pubkey, Pubkey::new_unique(), 1000);
        data_size_index.add_account(pubkey, 100, 1000);
    }
    
    // Create cost model
    let cost_model = QueryCostModel::new();
    
    // Estimate cardinality for data size filter
    let est_cardinality = cost_model.estimate_cardinality(
        &query_orchestrator::GpaFilter::DataSize(100),
        &data_size_index,
    );
    
    // Should be approximately 100
    assert!(est_cardinality > 50 && est_cardinality < 150);
}

#[test]
#[ignore] // Performance test
fn test_query_planner_performance() {
    let temp_dir = std::env::temp_dir().join("planner_perf");
    
    // Create large indexes
    let program_index = Arc::new(ProgramIndex::new(temp_dir.join("program")));
    let data_size_index = Arc::new(DataSizeIndex::new(temp_dir.join("size")));
    let memcmp_index = Arc::new(MemcmpIndex::new(temp_dir.join("memcmp")));
    let discriminator_index = Arc::new(DiscriminatorIndex::new(temp_dir.join("disc")));
    let accelerator = Arc::new(MemcmpAccelerator::new(temp_dir.join("accel")));
    
    // Insert 100K accounts
    for i in 0..100_000 {
        let pubkey = Pubkey::new_unique();
        program_index.add_account(pubkey, Pubkey::new_unique(), 1000 + i);
        data_size_index.add_account(pubkey, 200 + (i % 100), 1000 + i);
    }
    
    let planner = QueryPlanner::new(
        program_index,
        data_size_index,
        memcmp_index,
        discriminator_index,
        accelerator,
    );
    
    // Benchmark query planning
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let filters = vec![
            query_orchestrator::GpaFilter::DataSize(200),
        ];
        let _plan = planner.plan_query(Pubkey::new_unique(), filters);
    }
    let elapsed = start.elapsed();
    
    // Should plan 1000 queries in <100ms
    println!("Query planning: 1000 queries in {:?}", elapsed);
    assert!(elapsed < std::time::Duration::from_millis(100));
}
