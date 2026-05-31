//! NUMA-Aware Fabric per Memory Pinning
//! 
//! Ottimizza l'allocazione della memoria in base ai NUMA nodes
//! per ridurre la latenza di accesso su sistemi multi-socket.

use std::sync::Arc;

/// NUMA Node ID
pub type NumaNodeId = u32;

/// Informazioni su un NUMA node
#[derive(Debug, Clone)]
pub struct NumaNodeInfo {
    /// Node ID
    pub node_id: NumaNodeId,
    /// Memoria totale (bytes)
    pub total_memory: u64,
    /// Memoria libera (bytes)
    pub free_memory: u64,
    /// CPU cores associati
    pub cpu_cores: Vec<u32>,
}

/// NUMA-Aware Memory Allocator
#[allow(dead_code)]
pub struct NumaAllocator {
    /// Node preferito
    preferred_node: NumaNodeId,
    /// Fallback node
    fallback_node: NumaNodeId,
    /// Policy di allocazione
    policy: NumaPolicy,
}

/// Policy di allocazione NUMA
#[derive(Debug, Clone, Copy)]
pub enum NumaPolicy {
    /// Alloca sul node preferito
    Preferred,
    /// Alloca localmente al core corrente
    Local,
    /// Interleaved su tutti i nodes
    Interleaved,
    /// Bind stretto al node
    Bind,
}

impl NumaAllocator {
    /// Crea un nuovo allocator NUMA
    pub fn new(preferred_node: NumaNodeId, policy: NumaPolicy) -> Self {
        Self {
            preferred_node,
            fallback_node: 0,
            policy,
        }
    }
    
    /// Ottieni il node preferito
    pub fn preferred_node(&self) -> NumaNodeId {
        self.preferred_node
    }
    
    /// Ottieni la policy
    pub fn policy(&self) -> NumaPolicy {
        self.policy
    }
    
    /// Determina il node ottimale per allocazione
    pub fn optimal_node(&self, current_core: u32) -> NumaNodeId {
        match self.policy {
            NumaPolicy::Preferred => self.preferred_node,
            NumaPolicy::Local => self.core_to_node(current_core),
            NumaPolicy::Interleaved => self.interleaved_node(),
            NumaPolicy::Bind => self.preferred_node,
        }
    }
    
    /// Mappa un core al suo NUMA node
    fn core_to_node(&self, core: u32) -> NumaNodeId {
        // Semplificazione: assume core 0-7 → node 0, core 8-15 → node 1
        if core < 8 {
            0
        } else {
            1
        }
    }
    
    /// Calcola node interleaved
    fn interleaved_node(&self) -> NumaNodeId {
        // Round-robin semplice
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        (count % 2) as NumaNodeId
    }
}

/// NUMA-Aware Buffer Pool
pub struct NumaBufferPool {
    /// Buffer pool per node
    pools: Vec<Arc<parking_lot::RwLock<Vec<Vec<u8>>>>>,
    /// Dimensione buffer
    buffer_size: usize,
    /// Allocator
    allocator: NumaAllocator,
}

impl NumaBufferPool {
    /// Crea un nuovo buffer pool NUMA-aware
    pub fn new(num_nodes: usize, buffer_size: usize, allocator: NumaAllocator) -> Self {
        let pools = (0..num_nodes)
            .map(|_| Arc::new(parking_lot::RwLock::new(Vec::with_capacity(1024))))
            .collect();
        
        Self {
            pools,
            buffer_size,
            allocator,
        }
    }
    
    /// Ottieni buffer dal node ottimale
    pub fn acquire_buffer(&self, current_core: u32) -> Vec<u8> {
        let node = self.allocator.optimal_node(current_core);
        let pool = &self.pools[node as usize];
        
        {
            let mut pool = pool.write();
            if let Some(buffer) = pool.pop() {
                return buffer;
            }
        }
        
        // Crea nuovo buffer
        vec![0u8; self.buffer_size]
    }
    
    /// Rilascia buffer nel pool del node
    pub fn release_buffer(&self, buffer: Vec<u8>, current_core: u32) {
        let node = self.allocator.optimal_node(current_core);
        let pool = &self.pools[node as usize];
        
        let mut pool = pool.write();
        if pool.len() < 1024 {
            pool.push(buffer);
        }
    }
    
    /// Statistics per node
    pub fn node_stats(&self) -> Vec<NumaPoolStats> {
        self.pools
            .iter()
            .enumerate()
            .map(|(node_id, pool)| {
                let pool = pool.read();
                NumaPoolStats {
                    node_id: node_id as NumaNodeId,
                    buffer_count: pool.len(),
                    buffer_size: self.buffer_size,
                }
            })
            .collect()
    }
}

/// Statistics per NUMA pool
#[derive(Debug, Clone)]
pub struct NumaPoolStats {
    pub node_id: NumaNodeId,
    pub buffer_count: usize,
    pub buffer_size: usize,
}

/// NUMA-Aware Index Storage
pub struct NumaIndexStorage {
    /// Storage per node
    storage_per_node: Vec<Arc<parking_lot::RwLock<Vec<u8>>>>,
    /// Allocator
    allocator: NumaAllocator,
}

impl NumaIndexStorage {
    /// Crea nuovo storage NUMA-aware
    pub fn new(num_nodes: usize, initial_capacity: usize, allocator: NumaAllocator) -> Self {
        let storage = (0..num_nodes)
            .map(|_| {
                Arc::new(parking_lot::RwLock::new(vec![0u8; initial_capacity]))
            })
            .collect();
        
        Self {
            storage_per_node: storage,
            allocator,
        }
    }
    
    /// Ottieni storage per il node ottimale
    pub fn get_storage(&self, current_core: u32) -> Arc<parking_lot::RwLock<Vec<u8>>> {
        let node = self.allocator.optimal_node(current_core);
        self.storage_per_node[node as usize].clone()
    }
    
    /// Scrivi dati nel node ottimale
    pub fn write(&self, current_core: u32, offset: usize, data: &[u8]) -> std::io::Result<()> {
        let node = self.allocator.optimal_node(current_core);
        let mut storage = self.storage_per_node[node as usize].write();
        
        if offset + data.len() > storage.len() {
            storage.resize(offset + data.len(), 0);
        }
        
        storage[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }
    
    /// Leggi dati dal node
    pub fn read(&self, node: NumaNodeId, offset: usize, len: usize) -> Option<Vec<u8>> {
        let storage = self.storage_per_node[node as usize].read();
        
        if offset + len > storage.len() {
            return None;
        }
        
        Some(storage[offset..offset + len].to_vec())
    }
}

/// Rileva configurazione NUMA del sistema
pub fn detect_numa_config() -> Vec<NumaNodeInfo> {
    // Rilevamento semplificato per macOS (che non ha NUMA reale)
    // Su Linux reale, userebbe /sys/devices/system/node/
    
    let num_cpus = num_cpus::get();
    
    // Assume 1 node su macOS, 2 node su sistemi dual-socket
    let num_nodes = if num_cpus > 16 { 2 } else { 1 };
    
    (0..num_nodes)
        .map(|node_id| {
            let cores_per_node = num_cpus / num_nodes;
            let start_core = node_id * cores_per_node;
            let end_core = start_core + cores_per_node;
            
            NumaNodeInfo {
                node_id: node_id as NumaNodeId,
                total_memory: 16 * 1024 * 1024 * 1024, // 16GB stimati
                free_memory: 8 * 1024 * 1024 * 1024,   // 8GB liberi
                cpu_cores: (start_core..end_core).map(|x| x as u32).collect(),
            }
        })
        .collect()
}

/// Ottieni core corrente
pub fn current_core_id() -> u32 {
    // Su Linux: sched_getcpu()
    // Su macOS: fallback a 0
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_numa_allocator_preferred() {
        let allocator = NumaAllocator::new(0, NumaPolicy::Preferred);
        
        assert_eq!(allocator.optimal_node(0), 0);
        assert_eq!(allocator.optimal_node(15), 0);
    }
    
    #[test]
    fn test_numa_allocator_local() {
        let allocator = NumaAllocator::new(0, NumaPolicy::Local);
        
        assert_eq!(allocator.optimal_node(0), 0);
        assert_eq!(allocator.optimal_node(10), 1);
    }
    
    #[test]
    fn test_numa_buffer_pool() {
        let allocator = NumaAllocator::new(0, NumaPolicy::Preferred);
        let pool = NumaBufferPool::new(2, 4096, allocator);
        
        let buffer = pool.acquire_buffer(0);
        assert_eq!(buffer.len(), 4096);
        
        pool.release_buffer(buffer, 0);
        
        let stats = pool.node_stats();
        assert!(stats.len() >= 1);
    }
    
    #[test]
    fn test_numa_index_storage() {
        let allocator = NumaAllocator::new(0, NumaPolicy::Preferred);
        let storage = NumaIndexStorage::new(2, 1024, allocator);
        
        let data = b"NUMA test data";
        storage.write(0, 0, data).unwrap();
        
        let read = storage.read(0, 0, data.len()).unwrap();
        assert_eq!(read, data);
    }
    
    #[test]
    fn test_detect_numa_config() {
        let config = detect_numa_config();
        assert!(!config.is_empty());
        
        for node in config {
            assert!(!node.cpu_cores.is_empty());
            assert!(node.total_memory > 0);
        }
    }
}
