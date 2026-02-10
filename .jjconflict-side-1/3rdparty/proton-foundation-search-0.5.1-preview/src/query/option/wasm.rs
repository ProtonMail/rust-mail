use wasm_bindgen::prelude::wasm_bindgen;

use crate::query::option::QueryOptions;
use crate::query::option::text::{MaximumDistance, MinimumSimilarity};

/// If the crate is used directly from JS, we don't need to worry about extensibility, all features are set
#[wasm_bindgen]
impl QueryOptions {
    /// Set levenshtein max distance for fuzzy text matching
    #[wasm_bindgen(js_name = "setMaximumDistance")]
    pub fn set_maximum_distance(&mut self, max_distance: usize) {
        self.set(MaximumDistance::from(max_distance));
    }
    /// Set levenshtein max distance for fuzzy text matching
    #[wasm_bindgen(js_name = "setMinimumSimilarity")]
    pub fn set_minimum_similarity(&mut self, min_similarity: f64) {
        self.set(MinimumSimilarity::from(min_similarity));
    }
}
