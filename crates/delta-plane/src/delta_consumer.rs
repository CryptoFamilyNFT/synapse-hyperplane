//! Delta Plane Consumer
//!
//! Consuma aggiornamenti dal ring buffer Geyser e li scrive nei delta segment.

use std::sync::Arc;
use std::path::PathBuf;
use std::time::Duration;
use parking_lot::RwLock;
use tracing::{info, warn, error};

use crate::segment_writer::{SegmentWriter, SegmentInfo};
use crate::delta_locator::{DeltaLocator, DeltaSegmentMeta};
use crate::update_reducer::UpdateReducer;
use geyser_bridge::ring_buffer::{RingBufferReader, RingBufferError};
use hyperplane_types::AccountLocation;
use solana_sdk::pubkey::Pubkey;

/// Delta consumer configuration
#[derive(Debug, Clone)]
pub struct DeltaConsumerConfig {
    /// Path al ring buffer
    pub ring_buffer_path: String,
    /// Path alla directory delta segments
    pub delta_path: PathBuf,
    /// Dimensione massima segmento prima di flush
    pub max_segment_size: u64,
    /// Intervallo di polling del ring buffer
    pub poll_interval_ms: u64,
    /// Batch size per flush
    pub flush_batch_size: usize,
}

impl Default for DeltaConsumerConfig {
    fn default() -> Self {
        Self {
            ring_buffer_path: "/tmp/synapse/ring_buffer.bin".to_string(),
            delta_path: PathBuf::from("/tmp/synapse/delta"),
            max_segment_size: 100 * 1024 * 1024, // 100 MB
            poll_interval_ms: 10,
            flush_batch_size: 1000,
        }
    }
}

/// Delta consumer state
#[derive(Debug)]
pub struct DeltaConsumerState {
    /// Running flag
    pub running: bool,
    /// Segmento corrente
    pub current_segment: Option<SegmentWriter>,
    /// Slot iniziale del segmento corrente
    pub current_segment_start_slot: Option<u64>,
    /// Entry count nel segmento corrente
    pub current_segment_entries: u64,
    /// Totale aggiornamenti processati
    pub total_processed: u64,
    /// Totale errori
    pub total_errors: u64,
}

impl Default for DeltaConsumerState {
    fn default() -> Self {
        Self {
            running: false,
            current_segment: None,
            current_segment_start_slot: None,
            current_segment_entries: 0,
            total_processed: 0,
            total_errors: 0,
        }
    }
}

/// Delta Consumer per Geyser updates
pub struct DeltaConsumer {
    config: DeltaConsumerConfig,
    state: Arc<RwLock<DeltaConsumerState>>,
    delta_locator: Arc<DeltaLocator>,
    update_reducer: Arc<UpdateReducer>,
}

impl DeltaConsumer {
    /// Crea un nuovo delta consumer
    pub fn new(config: DeltaConsumerConfig, delta_locator: Arc<DeltaLocator>) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(DeltaConsumerState::default())),
            delta_locator,
            update_reducer: Arc::new(UpdateReducer::new()),
        }
    }
    
    /// Avvia il consumer in background
    pub fn start(&self) -> Result<(), DeltaConsumerError> {
        let mut state = self.state.write();
        if state.running {
            return Err(DeltaConsumerError::AlreadyRunning);
        }
        
        state.running = true;
        info!("Delta consumer started");
        
        // Crea directory delta segments se non esiste
        std::fs::create_dir_all(&self.config.delta_path)?;
        
        Ok(())
    }
    
    /// Ferma il consumer
    pub fn stop(&self) {
        let mut state = self.state.write();
        state.running = false;
        
        // Finalizza segmento corrente
        if let Some(segment) = state.current_segment.take() {
            match segment.finalize() {
                Ok(info) => {
                    info!("Finalized segment: {:?}", info);
                }
                Err(e) => {
                    error!("Failed to finalize segment: {}", e);
                }
            }
        }
        
        info!("Delta consumer stopped");
    }
    
    /// Legge aggiornamenti dal ring buffer e li processa
    pub fn process_updates(&self) -> Result<u64, DeltaConsumerError> {
        let mut ring_reader = RingBufferReader::open(&self.config.ring_buffer_path)?;
        let mut processed = 0;
        
        while let Some(entry) = ring_reader.read_next()? {
            // Estrai dati dall'entry
            let pubkey = Pubkey::try_from(entry.pubkey.as_slice())
                .map_err(|_| DeltaConsumerError::InvalidPubkey)?;
            
            // Crea AccountLocation per delta layer
            let location = AccountLocation {
                file_id: 0, // Assegnato dal segment writer
                offset: 0,  // Assegnato dal segment writer
                stored_size: entry.data.len() as u32,
                data_offset: 0,
                data_len: entry.data.len() as u32,
                slot: entry.slot,
                write_version: entry.write_version,
                storage_type: hyperplane_types::StorageType::Delta,
            };
            
            // Aggiungi al reducer per deduplicazione
            self.update_reducer.add_update(
                pubkey,
                entry.slot,
                entry.write_version,
                location,
            );
            
            // Scrivi nel segmento corrente
            self.write_to_segment(entry.slot, entry.write_version, &entry.pubkey, entry.data)?;
            
            processed += 1;
        }
        
        // Aggiorna stats
        {
            let mut state = self.state.write();
            state.total_processed += processed;
        }
        
        // Check se dobbiamo flushare il segmento
        self.maybe_flush_segment()?;
        
        Ok(processed)
    }
    
    /// Scrive un aggiornamento al segmento corrente
    fn write_to_segment(
        &self,
        slot: u64,
        write_version: u64,
        pubkey: &[u8; 32],
        data: &[u8],
    ) -> Result<(), DeltaConsumerError> {
        let mut state = self.state.write();
        
        // Crea nuovo segmento se necessario
        if state.current_segment.is_none() {
            let segment_path = self.config.delta_path.join(format!(
                "segment_{}.bin",
                slot
            ));
            
            let writer = SegmentWriter::create(&segment_path, slot)?;
            state.current_segment = Some(writer);
            state.current_segment_start_slot = Some(slot);
            state.current_segment_entries = 0;
        }
        
        // Scrivi al segmento
        if let Some(segment) = state.current_segment.as_mut() {
            segment.append(slot, write_version, pubkey, data)?;
            state.current_segment_entries += 1;
        }
        
        Ok(())
    }
    
    /// Flush del segmento corrente se necessario
    fn maybe_flush_segment(&self) -> Result<(), DeltaConsumerError> {
        let mut state = self.state.write();
        
        let should_flush = state.current_segment_entries >= self.config.flush_batch_size as u64;
        
        if should_flush {
            if let Some(segment) = state.current_segment.take() {
                let info = segment.finalize()?;
                
                // Registra segmento nel locator
                let meta = DeltaSegmentMeta {
                    path: self.config.delta_path.join(format!(
                        "segment_{}.bin",
                        state.current_segment_start_slot.unwrap_or(0)
                    )),
                    start_slot: info.start_slot,
                    end_slot: info.end_slot,
                    entry_count: info.entry_count,
                    data_size: info.data_size,
                    created_at: std::time::SystemTime::now(),
                };
                
                self.delta_locator.register_segment(meta);
                
                info!("Flushed segment: {} entries, {} bytes", info.entry_count, info.data_size);
                
                state.current_segment = None;
                state.current_segment_start_slot = None;
                state.current_segment_entries = 0;
            }
        }
        
        Ok(())
    }
    
    /// Get statistics
    pub fn stats(&self) -> DeltaConsumerStats {
        let state = self.state.read();
        DeltaConsumerStats {
            running: state.running,
            total_processed: state.total_processed,
            total_errors: state.total_errors,
            current_segment_entries: state.current_segment_entries,
            pending_updates: self.update_reducer.pending_count(),
        }
    }
    
    /// Run loop principale (blocking)
    pub fn run(&self) -> Result<(), DeltaConsumerError> {
        self.start()?;
        
        info!("Delta consumer run loop started");
        
        while self.state.read().running {
            match self.process_updates() {
                Ok(count) => {
                    if count == 0 {
                        // Nessun aggiornamento, aspetta
                        std::thread::sleep(Duration::from_millis(self.config.poll_interval_ms));
                    }
                }
                Err(e) => {
                    warn!("Error processing updates: {}", e);
                    let mut state = self.state.write();
                    state.total_errors += 1;
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
        
        Ok(())
    }
}

/// Delta consumer statistics
#[derive(Debug, Clone)]
pub struct DeltaConsumerStats {
    pub running: bool,
    pub total_processed: u64,
    pub total_errors: u64,
    pub current_segment_entries: u64,
    pub pending_updates: usize,
}

/// Delta consumer errors
#[derive(Debug, thiserror::Error)]
pub enum DeltaConsumerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Ring buffer error: {0}")]
    RingBuffer(#[from] RingBufferError),
    
    #[error("Segment writer error: {0}")]
    SegmentWriter(#[from] crate::segment_writer::SegmentError),
    
    #[error("Consumer already running")]
    AlreadyRunning,
    
    #[error("Invalid pubkey length")]
    InvalidPubkey,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_delta_consumer_config() {
        let config = DeltaConsumerConfig::default();
        assert_eq!(config.poll_interval_ms, 10);
        assert_eq!(config.flush_batch_size, 1000);
    }
}
