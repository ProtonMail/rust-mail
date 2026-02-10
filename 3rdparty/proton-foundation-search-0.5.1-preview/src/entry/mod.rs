//! Engine import/export data types

mod value;
#[cfg(feature = "wasm-bindgen")]
mod wasm;

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub use self::value::*;

/// Processed Document.
///
/// It is an engine import/export article.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
#[cfg_attr(feature = "facet", derive(facet::Facet))]
pub struct Entry {
    /// Entry id
    identifier: Box<str>,
    /// Attribute values
    attributes: BTreeMap<Box<str>, Arc<EntryValues>>,
}

impl Entry {
    /// Create a new entry
    pub fn new<I>(identifier: I, attributes: BTreeMap<Box<str>, Arc<EntryValues>>) -> Self
    where
        I: Into<Box<str>>,
    {
        Self {
            identifier: identifier.into(),
            attributes,
        }
    }

    /// Get the entry identifier
    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Get entry attributes
    pub fn attributes(&self) -> &BTreeMap<Box<str>, Arc<EntryValues>> {
        &self.attributes
    }
}

impl From<Entry> for (Box<str>, BTreeMap<Box<str>, Arc<EntryValues>>) {
    fn from(value: Entry) -> Self {
        let Entry {
            identifier,
            attributes,
        } = value;
        (identifier, attributes)
    }
}
