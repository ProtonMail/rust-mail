//! A search engine without IO (sans-IO) delegates IO operations to the application through Load/Save events.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::blob::BlobId;
use crate::engine::builder::EngineBuilder;
use crate::index::collection::CollectionSansIo;
use crate::index::prelude::Index;
use crate::index::text::TextIndex;
use crate::index::trivial::Trivial;
use crate::processor::{Processor, ProcessorConfig, TextProcessor};

pub mod builder;
mod cleanup;
mod export;
mod query;
mod reset;
#[cfg(feature = "wasm-bindgen")]
pub mod wasm;
mod write;

pub use cleanup::*;
pub use query::*;
pub use write::*;

const MANIFEST: &str = "manifest";
const MANIFEST_BLOB_ID: BlobId = BlobId::new(0, 0);

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
    /// Indices are stored as Arc to allow cloning for lazy search creation
    /// in multi-token AND queries with progressive universe filtering
    indices: BTreeMap<Box<str>, Arc<dyn Index>>,
    processor: Box<dyn Processor>,
    writer: AtomicBool,
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl Engine {
    /// Create the engine with a builder
    pub fn builder() -> EngineBuilder {
        EngineBuilder {}
    }

    /// Create a new search engine
    fn new(processor: Box<dyn Processor>, indices: BTreeMap<Box<str>, Arc<dyn Index>>) -> Self {
        Self {
            inner: Arc::new(InnerEngine {
                collection: CollectionSansIo::default(),
                indices,
                processor,
                writer: Default::default(),
            }),
        }
    }
}

impl Engine {
    /// Get the configured processor for this engine
    pub fn processor(&self) -> &dyn Processor {
        self.inner.processor.as_ref()
    }
}

/// Statistics about the engine
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct Stats {
    /// Number of total available documents
    pub documents: usize,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Manifest {
    collection_revision: Option<BlobId>,
    index_revisions: BTreeMap<Box<str>, BlobId>,
    active_blobs: BTreeSet<BlobId>,
    released_blobs: BTreeSet<BlobId>,
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
