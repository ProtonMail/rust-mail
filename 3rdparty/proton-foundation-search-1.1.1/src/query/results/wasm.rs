use wasm_bindgen::prelude::wasm_bindgen;

use crate::document::wasm::Value;
use crate::query::results::{FoundEntry, Score};

#[wasm_bindgen]
impl FoundEntry {
    /// Get the entry identifier
    #[wasm_bindgen(js_name = "identifier")]
    pub fn identifier_wasm(&self) -> String {
        self.identifier().to_owned()
    }

    /// Matched terms
    #[wasm_bindgen(js_name = "matches")]
    pub fn matches_wasm(&self) -> Vec<MatchValue> {
        self.matches().cloned().map(MatchValue).collect()
    }
}

#[wasm_bindgen]
pub struct MatchValue(super::MatchValue);
impl From<super::MatchValue> for MatchValue {
    fn from(value: super::MatchValue) -> Self {
        Self(value)
    }
}
impl From<MatchValue> for super::MatchValue {
    fn from(MatchValue(value): MatchValue) -> Self {
        value
    }
}
#[wasm_bindgen]
pub struct MatchOccurrence(super::MatchOccurrence);
impl From<super::MatchOccurrence> for MatchOccurrence {
    fn from(value: super::MatchOccurrence) -> Self {
        Self(value)
    }
}
impl From<MatchOccurrence> for super::MatchOccurrence {
    fn from(MatchOccurrence(value): MatchOccurrence) -> Self {
        value
    }
}

#[wasm_bindgen]
impl MatchValue {
    /// Create a new MatchValue
    #[wasm_bindgen(constructor)]
    pub fn new_wasm(value: Value, score: Score, occurrences: Vec<MatchOccurrence>) -> Self {
        Self(super::MatchValue {
            value: value.into(),
            score,
            occurrences: occurrences.into_iter().map(Into::into).collect(),
        })
    }

    /// Get the match occurrences
    #[wasm_bindgen(js_name = "value")]
    pub fn value_wasm(&self) -> Value {
        self.0.value.clone().into()
    }
}

#[wasm_bindgen]
impl MatchOccurrence {
    /// Create a new match occurrence
    #[wasm_bindgen(constructor)]
    pub fn new_wasm(attribute: String, index: usize, position: usize) -> Self {
        Self(super::MatchOccurrence::new(attribute, index, position))
    }

    /// Get matched attribute
    #[wasm_bindgen(js_name = "attribute")]
    pub fn attribute_wasm(&self) -> String {
        self.0.attribute().to_string()
    }

    /// Get matched value index (offset within the attribute value set)
    #[wasm_bindgen(js_name = "index")]
    pub fn index_wasm(&self) -> usize {
        self.0.index().0
    }

    /// Get matched token position (positional value associated with a text token)
    #[wasm_bindgen(js_name = "position")]
    pub fn position_wasm(&self) -> usize {
        self.0.position().0
    }
}
