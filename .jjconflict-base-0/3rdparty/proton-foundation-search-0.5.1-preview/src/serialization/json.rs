//! Json SerDe impl compiled optional with the "json" feature
use super::*;

/// SerDe implementation with [`serde_json`]
#[derive(Debug, Default)]
pub struct Json;
impl Json {
    pub fn serialize<V: serde::Serialize>(
        value: &V,
    ) -> Result<Vec<u8>, Box<dyn Send + Sync + std::error::Error>> {
        Ok(serde_json::to_vec(value)?)
    }

    pub fn deserialize<V: for<'de> serde::Deserialize<'de>>(
        data: &[u8],
    ) -> Result<V, Box<dyn Send + Sync + std::error::Error>> {
        Ok(serde_json::from_slice(data)?)
    }
}
