//! Cbor SerDe impl
use super::*;

/// SerDe implementation with [`ciborium`]
#[derive(Debug, Default)]
pub struct Cbor;
impl Cbor {
    pub fn serialize<V: serde::Serialize>(
        value: &V,
    ) -> Result<Vec<u8>, Box<dyn Send + Sync + std::error::Error>> {
        let mut buff = vec![];
        ciborium::into_writer(value, &mut buff)?;
        Ok(buff)
    }

    pub fn deserialize<V: for<'de> serde::Deserialize<'de>>(
        data: &[u8],
    ) -> Result<V, Box<dyn Send + Sync + std::error::Error>> {
        Ok(ciborium::from_reader(data)?)
    }
}
