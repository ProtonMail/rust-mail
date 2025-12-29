//! WAL Engine - Engine with WAL-based writer
//!
//! This provides a thin wrapper around Engine that uses WriterWAL instead of Writer.

use super::{Engine, Query, WriterWAL};
use crate::engine::Cleanup;

/// WAL-enabled search engine extending Engine
#[derive(Debug, Clone)]
pub struct EngineWAL {
    base: Engine,
}

impl Default for EngineWAL {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineWAL {
    /// Create a new WAL engine from schema
    pub fn new() -> Self {
        Self {
            base: Engine::builder().build(),
        }
    }

    /// Create a new WAL engine from an existing Engine instance
    /// Needed in order to add WAL functionality to an existing Engine instance
    pub fn from_base(base: Engine) -> Self {
        Self { base }
    }

    /// Start a WAL write operation  
    pub fn write(&self) -> Option<WriterWAL> {
        self.base.write().map(WriterWAL::new)
    }

    /// Delegate query to base engine
    pub fn query(&self) -> Query {
        self.base.query()
    }

    /// Delegate reset to base engine
    pub fn reset(&mut self) -> Option<Cleanup> {
        // TODO: check if the wal reset releases all blobs in the chain
        self.base.reset()
    }

    /// Set the current batch number for EntryIndex generation
    /// This is used to ensure unique EntryIndex values across batches using Cantor pairing
    pub fn set_current_batch(&self, batch_number: u32) {
        self.base.set_current_batch(batch_number);
    }

    /// Get the current batch number for EntryIndex generation
    pub fn get_current_batch(&self) -> u32 {
        self.base.get_current_batch()
    }
}
