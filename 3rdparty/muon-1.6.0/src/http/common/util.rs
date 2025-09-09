#![allow(unused)]

use muon_proc::autoimpl;
use serde::Serialize;

#[autoimpl]
pub trait JsonExt {
    fn encode_json(&self) -> serde_json::Result<Vec<u8>>
    where
        Self: Serialize,
    {
        serde_json::to_vec(self)
    }

    fn decode_json<T>(&self) -> serde_json::Result<T>
    where
        Self: AsRef<[u8]>,
        T: for<'de> serde::de::Deserialize<'de>,
    {
        serde_json::from_slice(self.as_ref())
    }
}
