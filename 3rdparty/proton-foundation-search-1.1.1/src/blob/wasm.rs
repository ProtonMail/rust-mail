//! Wasm specific blob implementations

use wasm_bindgen::JsError;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::blob::{BlobId, Cached};
use crate::serialization::SerDes;

#[wasm_bindgen]
impl BlobId {
    /// Convert blob ID into a string
    #[wasm_bindgen(js_name=toString)]
    pub fn wasm_to_string(&self) -> String {
        ToString::to_string(self)
    }
}

#[wasm_bindgen]
impl Cached {
    /// Serialize the cached data
    #[wasm_bindgen(js_name = "serialize")]
    pub fn serialize_wasm(&self, serdes: SerDes) -> Result<Vec<u8>, JsError> {
        self.serialize(&serdes)
            .map_err(|e| JsError::new(&e.to_string()))
    }
}
