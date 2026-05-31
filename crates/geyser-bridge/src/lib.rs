//! Geyser Bridge Plugin
//!
//! Minimal Geyser plugin that forwards account updates to Synapse Hyperplane
//! via shared memory ring buffer.
//!
//! # Safety
//!
//! This plugin MUST be fast and non-blocking. All persistence is async.

pub mod ring_buffer;

use agave_geyser_plugin_interface::geyser_plugin_interface::{
    GeyserPlugin, ReplicaAccountInfoVersions, Result,
};
use ring_buffer::RingBufferWriter;
use std::sync::Arc;
use parking_lot::RwLock;

/// Geyser plugin for Synapse Hyperplane
#[derive(Debug)]
pub struct GeyserBridgePlugin {
    /// Ring buffer writer (shared memory)
    ring_buffer: Arc<RwLock<Option<RingBufferWriter>>>,
}

impl GeyserBridgePlugin {
    pub fn new() -> Self {
        Self {
            ring_buffer: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Initialize ring buffer writer
    pub fn init_ring_buffer(&self, path: &str, capacity: usize) -> Result<()> {
        let writer = RingBufferWriter::create(path, capacity)
            .map_err(|e| {
                agave_geyser_plugin_interface::geyser_plugin_interface::GeyserPluginError::Custom(
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to create ring buffer: {}", e)))
                )
            })?;
        
        *self.ring_buffer.write() = Some(writer);
        tracing::info!("Ring buffer initialized at {} with capacity {}", path, capacity);
        Ok(())
    }
}

impl Default for GeyserBridgePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl GeyserPlugin for GeyserBridgePlugin {
    fn name(&self) -> &'static str {
        "SynapseHyperplaneGeyserBridge"
    }

    fn on_load(&mut self, _config_file: &str, _is_reload: bool) -> Result<()> {
        tracing::info!("Synapse Geyser Bridge plugin loaded");
        Ok(())
    }

    fn on_unload(&mut self) {
        tracing::info!("Synapse Geyser Bridge plugin unloaded");
    }

    fn update_account(
        &self,
        account: ReplicaAccountInfoVersions,
        slot: u64,
        _is_startup: bool,
    ) -> Result<()> {
        // Extract account info from ReplicaAccountInfoVersions
        match account {
            ReplicaAccountInfoVersions::V0_0_1(info) => {
                let pubkey: &[u8; 32] = info.pubkey.try_into().map_err(|_| {
                    agave_geyser_plugin_interface::geyser_plugin_interface::GeyserPluginError::Custom(
                        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid pubkey length"))
                    )
                })?;
                // Write to ring buffer if initialized (non-blocking)
                let ring_buffer = self.ring_buffer.read();
                if let Some(writer) = ring_buffer.as_ref() {
                    let _ = writer.write(slot, info.write_version, pubkey, info.data);
                }
            }
            ReplicaAccountInfoVersions::V0_0_2(info) => {
                let pubkey: &[u8; 32] = info.pubkey.try_into().map_err(|_| {
                    agave_geyser_plugin_interface::geyser_plugin_interface::GeyserPluginError::Custom(
                        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid pubkey length"))
                    )
                })?;
                // Write to ring buffer if initialized (non-blocking)
                let ring_buffer = self.ring_buffer.read();
                if let Some(writer) = ring_buffer.as_ref() {
                    let _ = writer.write(slot, info.write_version, pubkey, info.data);
                }
            }
            ReplicaAccountInfoVersions::V0_0_3(info) => {
                let pubkey: &[u8; 32] = info.pubkey.try_into().map_err(|_| {
                    agave_geyser_plugin_interface::geyser_plugin_interface::GeyserPluginError::Custom(
                        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid pubkey length"))
                    )
                })?;
                // Write to ring buffer if initialized (non-blocking)
                let ring_buffer = self.ring_buffer.read();
                if let Some(writer) = ring_buffer.as_ref() {
                    let _ = writer.write(slot, info.write_version, pubkey, info.data);
                }
            }
        };
        
        Ok(())
    }

    fn notify_end_of_startup(&self) -> Result<()> {
        tracing::info!("Geyser startup complete");
        Ok(())
    }
}

#[no_mangle]
#[allow(improper_ctypes_definitions)]
/// # Safety
///
/// This function returns a pointer to the Geyser plugin box implementing GeyserPlugin.
///
/// The Agave validator calls this function to load the plugin.
pub unsafe extern "C" fn _create_plugin() -> *mut dyn GeyserPlugin {
    let plugin = GeyserBridgePlugin::new();
    let plugin: Box<dyn GeyserPlugin> = Box::new(plugin);
    Box::into_raw(plugin)
}
