//! Query Cost Estimator
//!
//! Estimates query cost for getProgramAccounts queries based on:
//! - Bitmap cardinality
//! - Number of filters
//! - Index selectivity
//! - Expected result size

use hyperplane_types::PubkeyBitmap;

/// Query cost components
#[derive(Debug, Clone)]
pub struct QueryCostComponents {
    /// Base cost (fixed overhead)
    pub base_cost: u64,
    /// Bitmap intersection cost
    pub intersection_cost: u64,
    /// Data fetch cost (based on result size)
    pub fetch_cost: u64,
    /// Filtering cost (memcmp, dataSize)
    pub filter_cost: u64,
}

/// Query cost estimate
#[derive(Debug, Clone)]
pub struct QueryCostEstimate {
    /// Total estimated cost (arbitrary units)
    pub total_cost: u64,
    /// Estimated result cardinality
    pub estimated_cardinality: u64,
    /// Estimated time in microseconds
    pub estimated_time_us: u64,
    /// Cost breakdown
    pub components: QueryCostComponents,
    /// Whether query should use streaming
    pub should_stream: bool,
}

/// Query Cost Estimator
pub struct QueryCostEstimator {
    /// Base cost constant
    base_cost: u64,
    /// Cost per bitmap intersection
    intersection_cost_per_bitmap: u64,
    /// Cost per account fetched
    fetch_cost_per_account: u64,
}

impl QueryCostEstimator {
    pub fn new() -> Self {
        Self {
            base_cost: 100,
            intersection_cost_per_bitmap: 10,
            fetch_cost_per_account: 1,
        }
    }
    
    /// Estimate cost for a getProgramAccounts query
    pub fn estimate_get_program_accounts(
        &self,
        program_bitmap: &PubkeyBitmap,
        num_data_size_filters: usize,
        num_memcmp_filters: usize,
        has_discriminator_filter: bool,
    ) -> QueryCostEstimate {
        // Base cost
        let base_cost = self.base_cost;
        
        // Intersection cost (program + dataSize + memcmp + discriminator)
        let num_intersections = 1 + num_data_size_filters + num_memcmp_filters + if has_discriminator_filter { 1 } else { 0 };
        let intersection_cost = num_intersections as u64 * self.intersection_cost_per_bitmap;
        
        // Estimate result cardinality
        let estimated_cardinality = self.estimate_cardinality(
            program_bitmap,
            num_data_size_filters,
            num_memcmp_filters,
            has_discriminator_filter,
        );
        
        // Fetch cost
        let fetch_cost = estimated_cardinality * self.fetch_cost_per_account;
        
        // Filter cost (memcmp comparisons are expensive)
        let filter_cost = (num_memcmp_filters as u64) * estimated_cardinality * 10;
        
        // Total cost
        let total_cost = base_cost + intersection_cost + fetch_cost + filter_cost;
        
        // Estimate time (1 cost unit = 1 microsecond roughly)
        let estimated_time_us = total_cost;
        
        // Decide if streaming is beneficial (for large result sets)
        let should_stream = estimated_cardinality > 1000;
        
        QueryCostEstimate {
            total_cost,
            estimated_cardinality,
            estimated_time_us,
            components: QueryCostComponents {
                base_cost,
                intersection_cost,
                fetch_cost,
                filter_cost,
            },
            should_stream,
        }
    }
    
    /// Estimate result cardinality after filters
    fn estimate_cardinality(
        &self,
        program_bitmap: &PubkeyBitmap,
        num_data_size_filters: usize,
        num_memcmp_filters: usize,
        has_discriminator_filter: bool,
    ) -> u64 {
        let base_cardinality = program_bitmap.len();
        
        // Apply selectivity estimates for each filter type
        let mut cardinality = base_cardinality as f64;
        
        // DataSize filter: assume 10% selectivity (filters out 90%)
        for _ in 0..num_data_size_filters {
            cardinality *= 0.1;
        }
        
        // Memcmp filter: assume 5% selectivity per filter
        for _ in 0..num_memcmp_filters {
            cardinality *= 0.05;
        }
        
        // Discriminator filter: assume 1% selectivity (specific type)
        if has_discriminator_filter {
            cardinality *= 0.01;
        }
        
        cardinality.ceil() as u64
    }
}

impl Default for QueryCostEstimator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cost_estimator_basic() {
        let estimator = QueryCostEstimator::new();
        
        let mut bitmap = PubkeyBitmap::new();
        bitmap.insert(1);
        bitmap.insert(2);
        bitmap.insert(3);
        bitmap.insert(4);
        bitmap.insert(5);
        
        let estimate = estimator.estimate_get_program_accounts(&bitmap, 0, 0, false);
        
        assert!(estimate.total_cost > 0);
        assert_eq!(estimate.estimated_cardinality, 5);
    }
    
    #[test]
    fn test_cost_estimator_with_filters() {
        let estimator = QueryCostEstimator::new();
        
        let mut bitmap = PubkeyBitmap::new();
        for i in 0..1000 {
            bitmap.insert(i);
        }
        
        // No filters
        let estimate_no_filter = estimator.estimate_get_program_accounts(&bitmap, 0, 0, false);
        
        // With dataSize filter
        let estimate_with_filter = estimator.estimate_get_program_accounts(&bitmap, 1, 0, false);
        
        // Filter should reduce cardinality
        assert!(estimate_with_filter.estimated_cardinality < estimate_no_filter.estimated_cardinality);
    }
}
