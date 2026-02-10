use crate::index::prelude::{AttributeIndex, EntryIndex, EntryValues};
use crate::transaction::LoadEvent;

/// trait for indices supporting export
pub trait IndexExport {
    /// Dump the index contents
    fn export(&self, revision: u64) -> Box<dyn 'static + Send + Iterator<Item = IndexExportEvent>>;
}

/// An event representing an index store modification.
#[derive(Debug)]
pub enum IndexExportEvent {
    /// The index store requires storage load
    Load(LoadEvent),
    /// The index store requests storage save
    Entry {
        /// removed entry
        entry: EntryIndex,
        /// removed attribute
        attr: AttributeIndex,
        /// removed value
        value: EntryValues,
    },
}
