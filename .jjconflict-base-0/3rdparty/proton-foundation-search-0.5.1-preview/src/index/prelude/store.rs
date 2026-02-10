//! Index stores.

use std::sync::Arc;

pub use crate::entry::{EntryValue, EntryValues};
use crate::index::prelude::*;
use crate::transaction::{LoadEvent, ReleaseEvent, SaveEvent};

/// Trait representing index write transactions.
pub trait IndexStore {
    /// An ID that uniquely identifies the index instance
    /// for the purpose of tracking blob revisions accurately
    /// and to avoid having multiple identical indices in the engine (confusion)
    fn id(&self) -> &str;
    /// Starts a write transaction
    ///
    /// The sans-io transaction does not perform reads/writes itself. It asks the aplication through events instead.
    /// These Load/Save events are part of the iteration together with completed inserts and/or removals.
    /// The iterator represents a state machine that, when completed, updates the index state.
    ///
    /// In other words, incompletely iterated transaction is not committed.
    fn write(
        &self,
        revision: u64,
        operations: &[IndexStoreOperation],
    ) -> Box<dyn Send + Iterator<Item = IndexStoreEvent>>;

    /// Reset the engine state (caches and data)
    fn reset(&self);
}

/// A modification of the search index.
#[derive(Debug, Clone)]
pub enum IndexStoreOperation {
    /// Insert an entry attribute value.
    /// Or remove it by passing an empty value.
    Insert(EntryIndex, AttributeIndex, Arc<EntryValues>),
    /// Remove the whole entry.
    Remove(EntryIndex),
}

/// An event representing an index store modification.
#[derive(Debug)]
pub enum IndexStoreEvent {
    /// An entry attribute value has been inserted
    Inserted {
        /// inserted entry
        entry: EntryIndex,
        /// inserted attribute
        attr: AttributeIndex,
    },
    /// An entry attribute value has been removed
    Removed {
        /// removed entry
        entry: EntryIndex,
        /// removed attribute
        attr: AttributeIndex,
        /// removed value
        value: EntryValues,
    },
    /// The index store requires storage load
    Load(LoadEvent),
    /// The index store requests storage save
    Save(SaveEvent),
    /// The index store requests storage release (blob to delete)
    Release(ReleaseEvent),
}
