//! A search engine without IO (sans-IO) delegates IO operations to the application through Load/Save events.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::engine::builder::EngineBuilder;
use crate::index::collection::CollectionSansIo;
use crate::index::prelude::Index;
use crate::index::text::TextIndexSansIo;
use crate::index::trivial::Trivial;
use crate::processor::{Proc, Processor, ProcessorConfig};

pub mod builder;
mod cleanup;
mod export;
mod query;
mod reset;
#[cfg(feature = "wasm-bindgen")]
pub mod wasm;
mod write;
mod writerwal;
// WAL engine for write-ahead logging without Storage trait dependency
pub mod enginewal;

pub use cleanup::*;
pub use enginewal::*;
pub use query::*;
pub use write::*;
pub use writerwal::*;

const MANIFEST: &str = "manifest";

/// A search engine without async/IO.
///
/// IO is passed to the application as a write or query event.
///
/// Multiple concurrent reads and a single concurrent write is supported.
/// Readers see the last committed snapshot and can proceed during write operations.
///
/// # Example
///
/// ```rust
/// use proton_foundation_search::engine::*;
///
/// let engine = Engine::builder().build();
///
/// let _write = engine.write();
/// let _query = engine.query();
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct Engine {
    inner: Arc<InnerEngine>,
}

#[derive(Debug)]
struct InnerEngine {
    collection: CollectionSansIo,
    indices: BTreeMap<Box<str>, Box<dyn Index>>,
    processor: Box<dyn Proc>,
    writer: AtomicBool,
    current_batch: AtomicU32,
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl Engine {
    /// Create the engine with a builder
    pub fn builder() -> EngineBuilder {
        EngineBuilder {}
    }

    /// Get current engine statistics
    pub fn stats(&self) -> Stats {
        Stats {
            writing: self.inner.writer.load(Ordering::Relaxed),
            documents_loaded: 0,
            documents_total: self.inner.collection.len(),
        }
    }

    /// Set the current batch number for EntryIndex generation
    /// This is used to ensure unique EntryIndex values across batches using Cantor pairing
    pub fn set_current_batch(&self, batch_number: u32) {
        self.inner
            .current_batch
            .store(batch_number, Ordering::Relaxed);
    }

    /// Get the current batch number for EntryIndex generation
    pub fn get_current_batch(&self) -> u32 {
        self.inner.current_batch.load(Ordering::Relaxed)
    }

    /// Create a new search engine
    fn new(processor: Box<dyn Proc>, indices: BTreeMap<Box<str>, Box<dyn Index>>) -> Self {
        Self {
            inner: Arc::new(InnerEngine {
                collection: CollectionSansIo::default(),
                indices,
                processor,
                writer: Default::default(),
                current_batch: AtomicU32::new(0),
            }),
        }
    }
}

/// Statistics about the engine
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct Stats {
    /// Is the engine currently writing?
    pub writing: bool,
    /// Number of documents loaded in cache
    pub documents_loaded: usize,
    /// Number of total available documents
    pub documents_total: Option<usize>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifest {
    collection_revision: u64,
    index_revisions: BTreeMap<Box<str>, u64>,
    active_blobs: BTreeSet<Box<str>>,
    released_blobs: BTreeSet<Box<str>>,
}

#[derive(Debug)]
#[repr(C)]
struct EngineWriteGuard(Arc<InnerEngine>);
impl std::ops::Deref for EngineWriteGuard {
    type Target = InnerEngine;

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}
impl Drop for EngineWriteGuard {
    fn drop(&mut self) {
        self.0.writer.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests;
