//! serde abstractions for search engine

mod cbor;
mod json;

use std::error::Error;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use tracing::trace;

/// List of available serde implementations
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub enum SerDes {
    /// Json SerDe implementation
    Json,
    /// Cbor SerDe implementation
    #[default]
    Cbor,
}

impl SerDes {
    /// Serialize using chosen serde impl
    pub fn serialize<V: Serialize + Debug>(
        &self,
        value: &V,
    ) -> Result<Vec<u8>, Box<dyn Send + Sync + std::error::Error>> {
        trace!("{value:#?}");
        match self {
            SerDes::Json => json::Json::serialize(value),
            SerDes::Cbor => cbor::Cbor::serialize(value),
        }
    }

    /// Deserialize using chosen serde impl
    pub fn deserialize<V: for<'de> Deserialize<'de>>(
        &self,
        data: &[u8],
    ) -> Result<V, Box<dyn Send + Sync + Error>> {
        match self {
            SerDes::Json => json::Json::deserialize(data),
            SerDes::Cbor => cbor::Cbor::deserialize(data),
        }
    }
}
