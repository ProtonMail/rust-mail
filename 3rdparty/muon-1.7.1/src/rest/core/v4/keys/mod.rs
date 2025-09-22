use crate::rest::Bool;
use serde::{Deserialize, Serialize};

/// `/core/v4/keys/salts`
pub mod salts;

/// A key object.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Key {
    /// The key's ID.
    #[serde(rename = "ID")]
    pub id: String,

    /// The private key.
    pub private_key: String,

    /// The key's token, if any.
    pub token: Option<String>,

    /// The key's signature, if any.
    pub signature: Option<String>,

    /// Whether this is the primary key.
    pub primary: Bool,

    /// Whether this key is active.
    pub active: Bool,
}
