use wasm_bindgen::prelude::wasm_bindgen;

use crate::document::wasm::Value;
use crate::query::results::{FoundEntry, MatchOccurrence, MatchValue, Score};

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
        self.matches().cloned().collect()
    }
}

#[wasm_bindgen]
impl MatchValue {
    /// Create a new MatchValue
    #[wasm_bindgen(constructor)]
    pub fn new_wasm(value: Value, score: Score, occurrences: Vec<MatchOccurrence>) -> Self {
        Self {
            value: value.into(),
            score,
            occurrences,
        }
    }

    /// Get the match occurrences
    #[wasm_bindgen(js_name = "value")]
    pub fn value_wasm(&self) -> Value {
        self.value.clone().into()
    }
}

#[wasm_bindgen]
impl MatchOccurrence {
    /// Create a new match occurrence
    #[wasm_bindgen(constructor)]
    pub fn new_wasm(attribute: String, index: usize, position: usize) -> Self {
        Self::new(attribute, index, position)
    }

    /// Get matched attribute
    #[wasm_bindgen(js_name = "attribute")]
    pub fn attribute_wasm(&self) -> String {
        self.attribute().to_owned()
    }

    /// Get matched value index (offset within the attribute value set)
    #[wasm_bindgen(js_name = "index")]
    pub fn index_wasm(&self) -> usize {
        self.index().0
    }

    /// Get matched token position (positional value associated with a text token)
    #[wasm_bindgen(js_name = "position")]
    pub fn position_wasm(&self) -> usize {
        self.position().0
    }
}
