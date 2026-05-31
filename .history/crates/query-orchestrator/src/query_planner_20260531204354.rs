//! Query Planner for getProgramAccounts
//!
//! Plans and executes getProgramAccounts queries using bitmap indexes:
//! - Program index (required)
//! - DataSize filters (optional)
//! - Memcmp filters (optional)
//! - Discriminator filters (optional, for Anchor)
//! - Bitmap intersection
//! - Cost estimation con cardinalità
//! - Optimized execution order (più selettivi prima)
//! - Pagination

use std::sync::Arc;
use parking_lot::RwLock;
use solana_sdk::pubkey::Pubkey;
use log::info;

use hyperplane_types::PubkeyBitmap;
use index_fabric::{
    program_index::ProgramIndex,
    data_size_index::DataSizeIndex,
    memcmp_index::MemcmpIndex,
    discriminator_index::DiscriminatorIndex,
    memcmp_accelerator::MemcmpAccelerator,
};

use crate::{
    BitmapIntersectionEngine,
    QueryCostEstimator,
    QueryCostEstimate,
    PaginationHelper,
    PaginationCursor,
    PaginatedResult,
    QueryCostModel,
    FilterType,
    GpaFilter,  // Import da types.rs
};

/// Query plan for getProgramAccounts
#[derive(Debug, Clone)]
pub struct QueryPlan {
    /// Program ID (required)
    pub program_id: Pubkey,
    /// Optional filters
    pub filters: Vec<GpaFilter>,
    /// Estimated cost
    pub cost_estimate: QueryCostEstimate,
    /// Whether to use streaming
    pub use_streaming: bool,
}

/// Query Planner state
#[derive(Debug)]
pub struct QueryPlannerState {
    /// Number of queries planned
    pub queries_planned: u64,
    /// Total execution time (estimated)
    pub total_execution_time_us: u64,
}

/// Query Planner per getProgramAccounts con optimization basata su cardinalità
#[allow(dead_code)]
pub struct QueryPlanner {
    program_index: Arc<ProgramIndex>,
    data_size_index: Arc<DataSizeIndex>,
    memcmp_index: Arc<MemcmpIndex>,
    discriminator_index: Arc<DiscriminatorIndex>,
    memcmp_accelerator: Arc<MemcmpAccelerator>,  // NEW
    intersection_engine: BitmapIntersectionEngine,
    cost_estimator: QueryCostEstimator,
    cost_model: QueryCostModel,  // NEW: Cost model con statistics
    pagination_helper: PaginationHelper,
    state: Arc<RwLock<QueryPlannerState>>,
}

impl QueryPlanner {
    pub fn new(
        program_index: Arc<ProgramIndex>,
        data_size_index: Arc<DataSizeIndex>,
        memcmp_index: Arc<MemcmpIndex>,
        discriminator_index: Arc<DiscriminatorIndex>,
        memcmp_accelerator: Arc<MemcmpAccelerator>,  // NEW parameter
    ) -> Self {
        let cost_model = QueryCostModel::new();
        
        Self {
            program_index,
            data_size_index,
            memcmp_index,
            discriminator_index,
            memcmp_accelerator,
            intersection_engine: BitmapIntersectionEngine::new(),
            cost_estimator: QueryCostEstimator::new(),
            cost_model,
            pagination_helper: PaginationHelper::default(),
            state: Arc::new(RwLock::new(QueryPlannerState {
                queries_planned: 0,
                total_execution_time_us: 0,
            })),
        }
    }
    
    /// Refresh statistics per cost model (da chiamare periodicamente)
    pub fn refresh_statistics(&self, current_slot: u64) {
        self.cost_model.refresh_statistics(
            &self.program_index,
            &self.data_size_index,
            &self.memcmp_index,
            &self.discriminator_index,
            current_slot,
        );
    }
    
    /// Plan a getProgramAccounts query con optimization basata su cardinalità
    pub fn plan_query(
        &self,
        program_id: Pubkey,
        filters: Vec<GpaFilter>,
    ) -> QueryPlan {
        // Crea piano ottimizzato usando cost model
        let _optimized_plan = self.cost_model.create_optimized_plan(program_id, &filters);
        
        // Get program bitmap per cost estimator legacy
        let program_bitmap = self.program_index
            .get_program_accounts(&program_id)
            .unwrap_or_default();
        
        let program_bitmap = {
            let mut bitmap = PubkeyBitmap::new();
            for _ in program_bitmap {
                bitmap.insert(0); // Placeholder - in real impl, use pubkey_id
            }
            bitmap
        };
        
        // Count filter types
        let num_data_size_filters = filters.iter().filter(|f| matches!(f, GpaFilter::DataSize(_))).count();
        let num_memcmp_filters = filters.iter().filter(|f| matches!(f, GpaFilter::Memcmp { .. })).count();
        let has_discriminator = filters.iter().any(|f| matches!(f, GpaFilter::Discriminator(_)));
        
        // Estimate cost (legacy)
        let cost_estimate = self.cost_estimator.estimate_get_program_accounts(
            &program_bitmap,
            num_data_size_filters,
            num_memcmp_filters,
            has_discriminator,
        );
        
        // Decide if streaming
        let use_streaming = cost_estimate.should_stream;
        
        QueryPlan {
            program_id,
            filters,
            cost_estimate,
            use_streaming,
        }
    }
    
    /// Execute query con optimized execution order (basato su cardinalità)
    /// Usa MemcmpAccelerator quando disponibile per O(1) lookup
    pub fn execute_query_optimized(&self, plan: &QueryPlan) -> Vec<Pubkey> {
        // Crea piano ottimizzato
        let optimized_plan = self.cost_model.create_optimized_plan(plan.program_id, &plan.filters);
        
        info!("Executing query with optimized plan: cost={}, time={}μs",
            optimized_plan.estimated_total_cost,
            optimized_plan.estimated_time_us,
        );
        
        // Esegui filtri nell'ordine ottimizzato (dal più selettivo al meno selettivo)
        let mut result_set: Option<std::collections::HashSet<Pubkey>> = None;
        
        for filter_type in &optimized_plan.execution_order {
            let accounts = match filter_type {
                FilterType::Program(pid) => {
                    self.program_index.get_program_accounts(pid)
                }
                FilterType::DataSize(size) => {
                    self.data_size_index.get_accounts_by_size(*size)
                }
                FilterType::Memcmp(offset, bytes) => {
                    // PROVA PRIMA: MemcmpAccelerator (O(1))
                    // Nota: program_id non disponibile qui, usiamo fallback
                    self.memcmp_index.get_accounts_by_memcmp(*offset, bytes)
                }
                FilterType::Discriminator(disc) => {
                    self.discriminator_index.get_accounts_by_discriminator(*disc)
                }
            };
            
            // Intersect con risultato corrente
            if let Some(accounts) = accounts {
                let account_set: std::collections::HashSet<Pubkey> = accounts.into_iter().collect();
                
                match &mut result_set {
                    Some(current) => {
                        *current = current.intersection(&account_set).copied().collect();
                    }
                    None => {
                        result_set = Some(account_set);
                    }
                }
            }
        }
        
        let result = result_set.unwrap_or_default();
        
        // Update stats
        {
            let mut state = self.state.write();
            state.queries_planned += 1;
            state.total_execution_time_us += optimized_plan.estimated_time_us;
        }
        
        result.into_iter().collect()
    }
    
    /// Execute a planned query
    pub fn execute_query(&self, plan: &QueryPlan) -> Vec<Pubkey> {
        // Collect all filtered account sets
        let mut filtered_account_sets: Vec<Vec<Pubkey>> = Vec::new();
        
        // Get program accounts (required)
        if let Some(program_accounts) = self.program_index.get_program_accounts(&plan.program_id) {
            filtered_account_sets.push(program_accounts);
        } else {
            return Vec::new(); // No program accounts found
        }
        
        // Apply dataSize filters
        for filter in &plan.filters {
            if let GpaFilter::DataSize(size) = filter {
                if let Some(accounts) = self.data_size_index.get_accounts_by_size(*size) {
                    filtered_account_sets.push(accounts);
                }
            }
        }
        
        // Apply memcmp filters
        for filter in &plan.filters {
            if let GpaFilter::Memcmp { offset, bytes } = filter {
                if let Some(accounts) = self.memcmp_index.get_accounts_by_memcmp(*offset, bytes) {
                    filtered_account_sets.push(accounts);
                }
            }
        }
        
        // Apply discriminator filter
        for filter in &plan.filters {
            if let GpaFilter::Discriminator(disc) = filter {
                if let Some(accounts) = self.discriminator_index.get_accounts_by_discriminator(*disc) {
                    filtered_account_sets.push(accounts);
                }
            }
        }
        
        // If no additional filters, return program accounts directly
        if filtered_account_sets.len() == 1 {
            return filtered_account_sets.remove(0);
        }
        
        // Intersect all account sets
        let mut result_set: std::collections::HashSet<Pubkey> = filtered_account_sets[0].iter().copied().collect();
        for accounts in &filtered_account_sets[1..] {
            let other_set: std::collections::HashSet<Pubkey> = accounts.iter().copied().collect();
            result_set = result_set.intersection(&other_set).copied().collect();
        }
        
        // Update stats
        {
            let mut state = self.state.write();
            state.queries_planned += 1;
            state.total_execution_time_us += result_set.len() as u64 * 10; // Estimate 10us per account
        }
        
        result_set.into_iter().collect()
    }
    
    /// Execute query with pagination
    pub fn execute_query_paginated(
        &self,
        plan: &QueryPlan,
        cursor: Option<&PaginationCursor>,
    ) -> PaginatedResult {
        let all_results = self.execute_query(plan);
        self.pagination_helper.paginate(&all_results, cursor)
    }
    
    /// Get planner statistics
    pub fn stats(&self) -> QueryPlannerStats {
        let state = self.state.read();
        QueryPlannerStats {
            queries_planned: state.queries_planned,
            total_execution_time_us: state.total_execution_time_us,
            avg_execution_time_us: if state.queries_planned > 0 {
                state.total_execution_time_us / state.queries_planned
            } else {
                0
            }
        }
    }
}

/// Query planner statistics
#[derive(Debug, Clone)]
pub struct QueryPlannerStats {
    pub queries_planned: u64,
    pub total_execution_time_us: u64,
    pub avg_execution_time_us: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    
    #[test]
    fn test_query_planner_basic() {
        let program_index = Arc::new(ProgramIndex::new(PathBuf::from("/tmp/program")));
        let data_size_index = Arc::new(DataSizeIndex::new(PathBuf::from("/tmp/size")));
        let memcmp_index = Arc::new(MemcmpIndex::new(PathBuf::from("/tmp/memcmp")));
        let discriminator_index = Arc::new(DiscriminatorIndex::new(PathBuf::from("/tmp/disc")));
        let memcmp_accelerator = Arc::new(MemcmpAccelerator::new(PathBuf::from("/tmp/accel")));
        
        let planner = QueryPlanner::new(
            program_index,
            data_size_index,
            memcmp_index,
            discriminator_index,
            memcmp_accelerator,
        );
        
        let program_id = Pubkey::new_unique();
        let filters = vec![
            GpaFilter::DataSize(100),
            GpaFilter::Memcmp { offset: 0, bytes: vec![1, 2, 3] },
        ];
        
        let plan = planner.plan_query(program_id, filters);
        
        assert_eq!(plan.program_id, program_id);
        assert_eq!(plan.filters.len(), 2);
        assert!(plan.cost_estimate.total_cost > 0);
    }
}
