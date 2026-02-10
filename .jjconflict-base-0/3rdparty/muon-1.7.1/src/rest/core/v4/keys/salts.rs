use serde::{Deserialize, Serialize};

/// `GET /core/v4/keys/salts`
///
/// Gets the salts for the currently authenticated user.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Get;

/// The response from a `GET /core/v4/keys/salts` request.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetRes {
    /// The key salts.
    #[serde(default)]
    pub key_salts: Vec<KeySalt>,
}

/// A key salt object.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct KeySalt {
    /// The key's ID.
    #[serde(rename = "ID")]
    pub id: String,

    /// The key's salt, base64 encoded, if any.
    pub key_salt: Option<String>,
}
