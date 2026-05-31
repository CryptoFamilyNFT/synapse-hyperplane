//! Query Cost Model con Cardinalità Stimate
//!
//! Ottimizza l'ordine di esecuzione delle query basandosi sulla cardinalità:
//! - Tracka cardinalità per program, dataSize, memcmp, discriminator
//! - Ordina filtri per cardinalità crescente (più selettivi prima)
//! - Stima costo di ogni piano di esecuzione
//! - Sceglie piano ottimale

use std::collections::BTreeMap;
use std::sync::Arc;
use parking_lot::RwLock;
use solana_sdk::pubkey::Pubkey;
use log::info;

use index_fabric::{
    program_index::ProgramIndex,
    data_size_index::DataSizeIndex,
    memcmp_index::MemcmpIndex,
    discriminator_index::DiscriminatorIndex,
};

use crate::GpaFilter;

/// Statistics per index
#[derive(Debug, Clone, Default)]
pub struct IndexStatistics {
    /// Program ID → numero di account
    pub program_cardinality: BTreeMap<Pubkey, u64>,
    
    /// DataSize → numero di account
    pub datasize_cardinality: BTreeMap<u64, u64>,
    
    /// (offset, bytes) → numero di account
    pub memcmp_cardinality: BTreeMap<(usize, Vec<u8>), u64>,
    
    /// Discriminator → numero di account
    pub discriminator_cardinality: BTreeMap<[u8; 8], u64>,
    
    /// Totale account indicizzati
    pub total_accounts: u64,
    
    /// Slot dell'ultimo aggiornamento
    pub last_updated_slot: u64,
}

/// Stima della cardinalità di un filtro
#[derive(Debug, Clone)]
pub struct CardinalityEstimate {
    /// Tipo di filtro
    pub filter_type: FilterType,
    /// Cardinalità stimata (numero di account matching)
    pub estimated_cardinality: u64,
    /// Confidenza della stima (0.0 - 1.0)
    pub confidence: f64,
}

/// Tipo di filtro per ordinamento
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FilterType {
    /// Filtro su discriminator (più selettivo)
    Discriminator([u8; 8]),
    /// Filtro su memcmp (offset, bytes)
    Memcmp(usize, Vec<u8>),
    /// Filtro su dataSize
    DataSize(u64),
    /// Filtro su program (meno selettivo, sempre presente)
    Program(Pubkey),
}

/// Piano di esecuzione ottimizzato
#[derive(Debug, Clone)]
pub struct OptimizedExecutionPlan {
    /// Ordine ottimale dei filtri (dal più selettivo al meno selettivo)
    pub execution_order: Vec<FilterType>,
    /// Cardinalità stimata dopo ogni step
    pub intermediate_cardinalities: Vec<u64>,
    /// Costo totale stimato (in unità arbitrarie)
    pub estimated_total_cost: u64,
    /// Tempo di esecuzione stimato (microseconds)
    pub estimated_time_us: u64,
}

/// Cost Model per query planning
pub struct QueryCostModel {
    /// Statistics aggiornate periodicamente
    stats: Arc<RwLock<IndexStatistics>>,
    
    /// Pesi per calcolo costo (tunabili)
    cost_weights: CostWeights,
}

/// Pesi per il calcolo del costo
#[derive(Debug, Clone)]
pub struct CostWeights {
    /// Costo per bitmap lookup
    pub bitmap_lookup_cost: u64,
    
    /// Costo per bitmap intersection (per account)
    pub intersection_cost_per_account: u64,
    
    /// Costo per memory access (per account)
    pub memory_access_cost_per_account: u64,
}

impl Default for CostWeights {
    fn default() -> Self {
        Self {
            bitmap_lookup_cost: 10,           // 10 unità per lookup
            intersection_cost_per_account: 1, // 1 unità per account nell'intersection
            memory_access_cost_per_account: 2, // 2 unità per memory access
        }
    }
}

impl QueryCostModel {
    pub fn new() -> Self {
        Self {
            stats: Arc::new(RwLock::new(IndexStatistics::default())),
            cost_weights: CostWeights::default(),
        }
    }
    
    /// Aggiorna statistics dagli index
    pub fn refresh_statistics(
        &self,
        program_index: &ProgramIndex,
        data_size_index: &DataSizeIndex,
        memcmp_index: &MemcmpIndex,
        discriminator_index: &DiscriminatorIndex,
        current_slot: u64,
    ) {
        let mut stats = self.stats.write();
        
        // Aggiorna program cardinality
        stats.program_cardinality.clear();
        for program_id in program_index.get_all_programs() {
            if let Some(bitmap) = program_index.get_program_accounts(&program_id) {
                stats.program_cardinality.insert(program_id, bitmap.len() as u64);
            }
        }
        
        // Aggiorna datasize cardinality
        stats.datasize_cardinality.clear();
        for size in data_size_index.get_all_sizes() {
            if let Some(bitmap) = data_size_index.get_accounts_by_size(size) {
                stats.datasize_cardinality.insert(size, bitmap.len() as u64);
            }
        }
        
        // Aggiorna memcmp cardinality
        stats.memcmp_cardinality.clear();
        for (offset, bytes) in memcmp_index.get_all_memcmp_keys() {
            if let Some(bitmap) = memcmp_index.get_accounts_by_memcmp(offset, &bytes) {
                stats.memcmp_cardinality.insert((offset, bytes), bitmap.len() as u64);
            }
        }
        
        // Aggiorna discriminator cardinality
        stats.discriminator_cardinality.clear();
        for disc in discriminator_index.get_all_discriminators() {
            if let Some(bitmap) = discriminator_index.get_accounts_by_discriminator(disc) {
                stats.discriminator_cardinality.insert(disc, bitmap.len() as u64);
            }
        }
        
        // Aggiorna slot
        stats.last_updated_slot = current_slot;
        
        info!("QueryCostModel: refreshed statistics for {} programs, {} sizes, {} memcmps, {} discriminators",
            stats.program_cardinality.len(),
            stats.datasize_cardinality.len(),
            stats.memcmp_cardinality.len(),
            stats.discriminator_cardinality.len(),
        );
    }
    
    /// Stima cardinalità per un filtro
    pub fn estimate_cardinality(&self, filter: &GpaFilter) -> CardinalityEstimate {
        let stats = self.stats.read();
        
        match filter {
            GpaFilter::DataSize(size) => {
                let cardinality = stats.datasize_cardinality.get(size)
                    .copied()
                    .unwrap_or(0);
                
                CardinalityEstimate {
                    filter_type: FilterType::DataSize(*size),
                    estimated_cardinality: cardinality,
                    confidence: if cardinality > 0 { 0.9 } else { 0.5 },
                }
            }
            
            GpaFilter::Memcmp { offset, bytes } => {
                let cardinality = stats.memcmp_cardinality.get(&(*offset, bytes.clone()))
                    .copied()
                    .unwrap_or(0);
                
                CardinalityEstimate {
                    filter_type: FilterType::Memcmp(*offset, bytes.clone()),
                    estimated_cardinality: cardinality,
                    confidence: if cardinality > 0 { 0.9 } else { 0.5 },
                }
            }
            
            GpaFilter::Discriminator(disc) => {
                let cardinality = stats.discriminator_cardinality.get(disc)
                    .copied()
                    .unwrap_or(0);
                
                CardinalityEstimate {
                    filter_type: FilterType::Discriminator(*disc),
                    estimated_cardinality: cardinality,
                    confidence: if cardinality > 0 { 0.95 } else { 0.5 },
                }
            }
        }
    }
    
    /// Crea piano di esecuzione ottimizzato
    pub fn create_optimized_plan(
        &self,
        program_id: Pubkey,
        filters: &[GpaFilter],
    ) -> OptimizedExecutionPlan {
        let stats = self.stats.read();
        
        // Raccogli tutti i filtri con cardinalità stimate
        let mut filter_estimates: Vec<(FilterType, u64)> = Vec::new();
        
        // Program filter (sempre presente, meno selettivo)
        let program_cardinality = stats.program_cardinality.get(&program_id)
            .copied()
            .unwrap_or(1_000_000); // Default 1M se non trovato
        filter_estimates.push((FilterType::Program(program_id), program_cardinality));
        
        // Altri filtri
        for filter in filters {
            let estimate = self.estimate_cardinality(filter);
            filter_estimates.push((estimate.filter_type, estimate.estimated_cardinality));
        }
        
        // Ordina per cardinalità crescente (più selettivi prima)
        filter_estimates.sort_by_key(|(_, cardinality)| *cardinality);
        
        // Estrai ordine di esecuzione
        let execution_order: Vec<FilterType> = filter_estimates
            .iter()
            .map(|(filter_type, _)| filter_type.clone())
            .collect();
        
        // Calcola cardinalità intermedie
        let mut intermediate_cardinalities = Vec::new();
        let mut current_cardinality = program_cardinality;
        
        for (_, cardinality) in &filter_estimates {
            // Intersection: cardinalità finale è min delle due
            current_cardinality = std::cmp::min(current_cardinality, *cardinality);
            intermediate_cardinalities.push(current_cardinality);
        }
        
        // Calcola costo totale
        let estimated_total_cost = self.calculate_cost(&filter_estimates);
        
        // Stima tempo di esecuzione (1 unità costo ≈ 1μs)
        let estimated_time_us = estimated_total_cost;
        
        OptimizedExecutionPlan {
            execution_order,
            intermediate_cardinalities,
            estimated_total_cost,
            estimated_time_us,
        }
    }
    
    /// Calcola costo di un piano
    fn calculate_cost(&self, filters: &[(FilterType, u64)]) -> u64 {
        let mut total_cost = 0u64;
        
        // Costo bitmap lookup per ogni filtro
        total_cost += filters.len() as u64 * self.cost_weights.bitmap_lookup_cost;
        
        // Costo intersection
        if filters.len() > 1 {
            let mut running_cardinality = filters[0].1;
            
            for (_, cardinality) in &filters[1..] {
                // Intersection cost: proporzionale alla cardinalità corrente
                let intersection_cost = running_cardinality * self.cost_weights.intersection_cost_per_account;
                total_cost += intersection_cost;
                
                // Running cardinality: min delle due
                running_cardinality = std::cmp::min(running_cardinality, *cardinality);
            }
            
            // Memory access cost per risultato finale
            total_cost += running_cardinality * self.cost_weights.memory_access_cost_per_account;
        }
        
        total_cost
    }
    
    /// Get statistics
    pub fn get_statistics(&self) -> IndexStatistics {
        self.stats.read().clone()
    }
    
    /// Aggiorna statistics con nuovo account
    pub fn update_account_statistics(
        &self,
        program_id: &Pubkey,
        data_size: u64,
        data: &[u8],
    ) {
        let mut stats = self.stats.write();
        
        // Aggiorna program cardinality
        *stats.program_cardinality.entry(*program_id).or_insert(0) += 1;
        
        // Aggiorna datasize cardinality
        *stats.datasize_cardinality.entry(data_size).or_insert(0) += 1;
        
        // Aggiorna discriminator (primi 8 bytes)
        if data.len() >= 8 {
            let mut disc = [0u8; 8];
            disc.copy_from_slice(&data[0..8]);
            *stats.discriminator_cardinality.entry(disc).or_insert(0) += 1;
        }
        
        // Aggiorna totale
        stats.total_accounts += 1;
    }
}

impl Default for QueryCostModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cost_model_basic() {
        let cost_model = QueryCostModel::new();
        
        // Simula statistics
        {
            let mut stats = cost_model.stats.write();
            stats.program_cardinality.insert(Pubkey::new_unique(), 1000);
            stats.datasize_cardinality.insert(165, 500);
            stats.memcmp_cardinality.insert((32, vec![1, 2, 3]), 100);
            stats.discriminator_cardinality.insert([0, 1, 2, 3, 4, 5, 6, 7], 50);
        }
        
        // Crea filtri
        let filters = vec![
            GpaFilter::DataSize(165),
            GpaFilter::Memcmp { offset: 32, bytes: vec![1, 2, 3] },
            GpaFilter::Discriminator([0, 1, 2, 3, 4, 5, 6, 7]),
        ];
        
        // Crea piano ottimizzato
        let plan = cost_model.create_optimized_plan(Pubkey::new_unique(), &filters);
        
        // Verifica ordinamento (dal più selettivo al meno selettivo)
        assert!(matches!(plan.execution_order[0], FilterType::Discriminator(_)));
        assert!(matches!(plan.execution_order[1], FilterType::Memcmp(_, _)));
        assert!(matches!(plan.execution_order[2], FilterType::DataSize(_)));
        
        // Verifica cardinalità decrescenti
        assert!(plan.intermediate_cardinalities.windows(2).all(|w| w[0] <= w[1]));
    }
}
