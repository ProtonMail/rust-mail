use std::sync::Arc;

mod content;
mod read;
mod write;
pub use content::*;
pub use read::*;
pub use write::*;

const COLLECTION: &str = "collection";

/// Representation of all the know documents in the current partition.
#[derive(Debug, Clone, Default)]
pub struct CollectionSansIo {}
