use crate::rest::core::v4::keys::Key;
use serde::{Deserialize, Serialize};

/// `GET /core/v4/users`
///
/// Gets the currently authenticated user's data.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Get;

/// The response from a `GET /core/v4/users` request.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetRes {
    /// The user object.
    pub user: User,
}

/// A user object.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct User {
    /// The user's ID,
    #[serde(rename = "ID")]
    pub id: String,

    /// The user's username.
    pub name: String,

    /// The user's email.
    pub email: String,

    /// The user's keys.
    pub keys: Vec<Key>,
}
