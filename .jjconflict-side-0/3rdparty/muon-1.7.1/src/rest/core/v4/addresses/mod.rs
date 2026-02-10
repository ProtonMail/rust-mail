use crate::rest::core::v4::keys::Key;
use serde::{Deserialize, Serialize};

/// `GET /core/v4/addresses`
///
/// Gets the addresses of the currently authenticated user.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Get;

/// The response from a `GET /core/v4/addresses` request.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetRes {
    /// The addresses of the user.
    pub addresses: Vec<Address>,
}

/// A user object.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Address {
    /// The address's ID,
    #[serde(rename = "ID")]
    pub id: String,

    /// The address itself.
    pub email: String,

    /// The address's keys.
    pub keys: Vec<Key>,
}
