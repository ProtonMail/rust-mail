use wasm_bindgen::prelude::wasm_bindgen;

use crate::entry::Entry;

#[wasm_bindgen]
impl Entry {
    /// Get the entry identifier
    #[wasm_bindgen(js_name = "identifier")]
    pub fn identifier_wasm(&self) -> String {
        self.identifier.as_ref().to_owned()
    }
    /// Get an attribute value
    #[wasm_bindgen(js_name = "attribute")]
    pub fn attribute_wasm(&self, attribute: &str) -> Vec<EntryValue> {
        self.attributes
            .get(attribute)
            .map(|a| a.iter().map(|v| v.clone().into()).collect())
            .unwrap_or_default()
    }
    /// Get a list of attribute names
    #[wasm_bindgen(js_name = "attributes")]
    pub fn attributes_wasm(&self) -> Vec<String> {
        self.attributes
            .keys()
            .map(|attr| attr.as_ref().to_owned())
            .collect()
    }
}

#[wasm_bindgen]
pub struct EntryValue(super::EntryValue);

impl From<EntryValue> for super::EntryValue {
    fn from(value: EntryValue) -> Self {
        value.0
    }
}
impl From<super::EntryValue> for EntryValue {
    fn from(value: super::EntryValue) -> Self {
        EntryValue(value)
    }
}
